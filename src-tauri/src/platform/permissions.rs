//! OS permission status and deep links to the settings panes.
use serde::Serialize;

#[derive(Serialize, Clone, Copy, Debug)]
pub struct PermissionStatus {
    pub microphone: bool,
    /// Needed to send the paste keystroke.
    pub accessibility: bool,
    /// Needed to observe the global hotkey (macOS "Input Monitoring").
    pub input_monitoring: bool,
}

#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        pub fn AXIsProcessTrusted() -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        // macOS 10.15+
        pub fn CGPreflightListenEventAccess() -> bool;
    }
}

#[cfg(target_os = "macos")]
pub fn status() -> PermissionStatus {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    let microphone = unsafe {
        match AVMediaTypeAudio {
            Some(kind) => {
                AVCaptureDevice::authorizationStatusForMediaType(kind)
                    == AVAuthorizationStatus::Authorized
            }
            None => false,
        }
    };
    PermissionStatus {
        microphone,
        accessibility: unsafe { mac::AXIsProcessTrusted() },
        input_monitoring: unsafe { mac::CGPreflightListenEventAccess() },
    }
}

#[cfg(not(target_os = "macos"))]
pub fn status() -> PermissionStatus {
    PermissionStatus {
        microphone: true,
        accessibility: true,
        input_monitoring: true,
    }
}

/// Opens the matching System Settings pane (no-op on Windows).
pub fn open_settings(kind: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let pane = match kind {
            "microphone" => "Privacy_Microphone",
            "accessibility" => "Privacy_Accessibility",
            "input_monitoring" => "Privacy_ListenEvent",
            other => return Err(format!("unknown permission {other}")),
        };
        std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{pane}"
            ))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = kind;
        Ok(())
    }
}
