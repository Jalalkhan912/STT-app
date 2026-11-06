//! Microphone capture with cpal.
//!
//! A cpal `Stream` is not `Send`, so a dedicated audio thread owns it and the
//! `CpalRecorder` (which must be `Send` for the controller) talks to that
//! thread over a channel. The stream callback downmixes to mono, resamples to
//! 16 kHz and pushes into a shared `CaptureBuffer`.
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};
use dictation_core::capture::{rms, CaptureBuffer, PushResult, SAMPLE_RATE};
use dictation_core::controller::Recorder;
use dictation_core::events::Event;
use rubato::{FftFixedIn, Resampler};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PREROLL_MS: u32 = 200;
const MAX_MS: u32 = 600_000;
/// Time after the key is released for the last audio to arrive.
const TAIL_MS: u64 = 150;
/// About 33 ms of 16 kHz audio per level update.
const LEVEL_SAMPLES: usize = 528;
const RESAMPLE_CHUNK: usize = 1024;

struct Shared {
    buf: Mutex<CaptureBuffer>,
    err: AtomicBool,
    max_sent: AtomicBool,
    levels: Mutex<Box<dyn Fn(f32) + Send>>,
    events: Sender<Event>,
}

enum Cmd {
    Open(Sender<Result<(), String>>),
    Close,
}

pub struct CpalRecorder {
    cmd: Sender<Cmd>,
    shared: Arc<Shared>,
    always_on: bool,
    open: bool,
}

impl CpalRecorder {
    pub fn new(
        device: Option<String>,
        always_on: bool,
        levels: Box<dyn Fn(f32) + Send>,
        events: Sender<Event>,
    ) -> Self {
        let shared = Arc::new(Shared {
            buf: Mutex::new(CaptureBuffer::new(PREROLL_MS, MAX_MS)),
            err: AtomicBool::new(false),
            max_sent: AtomicBool::new(false),
            levels: Mutex::new(levels),
            events,
        });
        let (cmd, rx) = mpsc::channel::<Cmd>();
        let thread_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("audio".into())
            .spawn(move || audio_thread(device, thread_shared, rx));
        if let Err(e) = spawned {
            log::error!("cannot start audio thread: {e}");
        }
        let mut rec = CpalRecorder {
            cmd,
            shared,
            always_on,
            open: false,
        };
        if always_on {
            if let Err(e) = rec.open_stream() {
                log::warn!("microphone not available at startup: {e}");
            }
        }
        rec
    }

    fn open_stream(&mut self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.cmd
            .send(Cmd::Open(tx))
            .map_err(|_| "Audio thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "Audio thread stopped".to_string())??;
        self.open = true;
        Ok(())
    }

    fn close_stream(&mut self) {
        let _ = self.cmd.send(Cmd::Close);
        self.open = false;
    }
}

impl Recorder for CpalRecorder {
    fn start(&mut self) -> Result<(), String> {
        // A stream that errored (e.g. unplugged) is rebuilt from scratch.
        if self.shared.err.swap(false, Ordering::SeqCst) && self.open {
            self.close_stream();
        }
        if !self.open {
            self.open_stream()?;
        }
        self.shared.max_sent.store(false, Ordering::SeqCst);
        lock(&self.shared.buf).start();
        Ok(())
    }

    fn stop(&mut self) -> Result<Vec<f32>, String> {
        std::thread::sleep(Duration::from_millis(TAIL_MS));
        if self.shared.err.load(Ordering::SeqCst) {
            lock(&self.shared.buf).discard();
            self.close_stream();
            return Err("Microphone disconnected".into());
        }
        let samples = lock(&self.shared.buf).take();
        if !self.always_on {
            self.close_stream();
        }
        Ok(samples)
    }

    fn cancel(&mut self) {
        lock(&self.shared.buf).discard();
        if !self.always_on {
            self.close_stream();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn audio_thread(device: Option<String>, shared: Arc<Shared>, rx: mpsc::Receiver<Cmd>) {
    let mut stream: Option<Stream> = None;
    for cmd in rx {
        match cmd {
            Cmd::Open(reply) => {
                let result = if stream.is_some() {
                    Ok(())
                } else {
                    open_stream(device.as_deref(), &shared).map(|s| {
                        stream = Some(s);
                    })
                };
                let _ = reply.send(result);
            }
            Cmd::Close => stream = None,
        }
    }
}

fn find_device(name: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if let Some(wanted) = name {
        if let Ok(devices) = host.input_devices() {
            for d in devices {
                if d.name().is_ok_and(|n| n == wanted) {
                    return Ok(d);
                }
            }
        }
        log::warn!("selected input device not found; using the default");
    }
    host.default_input_device()
        .ok_or_else(|| "No microphone found".to_string())
}

fn open_stream(name: Option<&str>, shared: &Arc<Shared>) -> Result<Stream, String> {
    let device = find_device(name)?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Microphone unavailable: {e}"))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.config();
    {
        let mut buf = lock(&shared.buf);
        buf.discard();
    }
    shared.err.store(false, Ordering::SeqCst);
    let pipe = Pipeline::new(shared.clone(), config.sample_rate.0)?;
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, pipe),
        SampleFormat::I16 => build::<i16>(&device, &config, pipe),
        SampleFormat::U16 => build::<u16>(&device, &config, pipe),
        SampleFormat::I32 => build::<i32>(&device, &config, pipe),
        other => return Err(format!("Unsupported microphone format {other:?}")),
    }
    .map_err(|e| format!("Cannot open microphone: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("Cannot start microphone: {e}"))?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut pipe: Pipeline,
) -> Result<Stream, cpal::BuildStreamError>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels).max(1);
    let shared = pipe.shared.clone();
    let mut mono: Vec<f32> = Vec::new();
    device.build_input_stream(
        config,
        move |data: &[T], _| {
            mono.clear();
            for frame in data.chunks(channels) {
                let sum: f32 = frame.iter().map(|&v| f32::from_sample(v)).sum();
                mono.push(sum / frame.len() as f32);
            }
            pipe.feed(&mono);
        },
        move |e| {
            log::warn!("audio stream error: {e}");
            shared.err.store(true, Ordering::SeqCst);
        },
        None,
    )
}

/// Opens the default input briefly so the OS shows its microphone prompt.
/// Runs on a background thread (a cpal `Stream` is not `Send`).
pub fn probe_default_input() {
    std::thread::spawn(|| {
        let Some(device) = cpal::default_host().default_input_device() else {
            return;
        };
        let Ok(supported) = device.default_input_config() else {
            return;
        };
        let config: StreamConfig = supported.config();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => probe::<f32>(&device, &config),
            SampleFormat::I16 => probe::<i16>(&device, &config),
            SampleFormat::U16 => probe::<u16>(&device, &config),
            SampleFormat::I32 => probe::<i32>(&device, &config),
            _ => return,
        };
        if let Ok(stream) = stream {
            let _ = stream.play();
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
    });
}

fn probe<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &StreamConfig,
) -> Result<Stream, cpal::BuildStreamError> {
    device.build_input_stream(config, |_: &[T], _| {}, |_| {}, None)
}

/// Resamples to 16 kHz and feeds the capture buffer.
struct Pipeline {
    shared: Arc<Shared>,
    resampler: Option<FftFixedIn<f32>>,
    pending: Vec<f32>,
    level_acc: Vec<f32>,
}

impl Pipeline {
    fn new(shared: Arc<Shared>, rate: u32) -> Result<Self, String> {
        let resampler = if rate == SAMPLE_RATE {
            None
        } else {
            Some(
                FftFixedIn::<f32>::new(rate as usize, SAMPLE_RATE as usize, RESAMPLE_CHUNK, 2, 1)
                    .map_err(|e| format!("Cannot resample microphone audio: {e}"))?,
            )
        };
        Ok(Pipeline {
            shared,
            resampler,
            pending: Vec::new(),
            level_acc: Vec::new(),
        })
    }

    fn feed(&mut self, mono: &[f32]) {
        let mut outs: Vec<Vec<f32>> = Vec::new();
        {
            let Some(r) = self.resampler.as_mut() else {
                self.push(mono);
                return;
            };
            self.pending.extend_from_slice(mono);
            loop {
                let need = r.input_frames_next();
                if self.pending.len() < need {
                    break;
                }
                let chunk: Vec<f32> = self.pending.drain(..need).collect();
                match r.process(&[chunk], None) {
                    Ok(mut out) => outs.push(out.remove(0)),
                    Err(e) => {
                        log::warn!("resample failed: {e}");
                        break;
                    }
                }
            }
        }
        for out in outs {
            self.push(&out);
        }
    }

    fn push(&mut self, samples: &[f32]) {
        let (result, recording) = {
            let mut buf = lock(&self.shared.buf);
            let result = buf.push(samples);
            (result, buf.is_recording())
        };
        if recording {
            self.level_acc.extend_from_slice(samples);
            if self.level_acc.len() >= LEVEL_SAMPLES {
                let level = rms(&self.level_acc);
                self.level_acc.clear();
                let report = lock(&self.shared.levels);
                (*report)(level);
            }
        } else {
            self.level_acc.clear();
        }
        if result == PushResult::MaxReached && !self.shared.max_sent.swap(true, Ordering::SeqCst) {
            let _ = self.shared.events.send(Event::MaxDuration);
        }
    }
}

/// Names of the available input devices.
pub fn list_input_devices() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|it| it.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}
