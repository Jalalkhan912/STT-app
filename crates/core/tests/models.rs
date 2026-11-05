use dictation_core::models::*;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

type Ranges = Arc<Mutex<Vec<Option<String>>>>;

fn serve(body: Vec<u8>) -> (String, Ranges) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let seen: Ranges = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let range = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("Range"))
                .map(|h| h.value.as_str().to_string());
            seen2.lock().unwrap().push(range.clone());
            let start = range
                .as_deref()
                .and_then(|r| r.strip_prefix("bytes="))
                .and_then(|r| r.strip_suffix('-'))
                .and_then(|n| n.parse::<usize>().ok());
            let resp = match start {
                Some(s) => {
                    let len = body.len();
                    tiny_http::Response::from_data(body[s..].to_vec())
                        .with_status_code(206)
                        .with_header(
                            tiny_http::Header::from_bytes(
                                "Content-Range",
                                format!("bytes {}-{}/{}", s, len - 1, len),
                            )
                            .unwrap(),
                        )
                }
                None => tiny_http::Response::from_data(body.clone()),
            };
            let _ = req.respond(resp);
        }
    });
    (base, seen)
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

fn info(base: &str, body: &[u8]) -> ModelInfo {
    ModelInfo {
        id: "m".into(),
        label: "M".into(),
        kind: EngineKind::Whisper,
        files: vec![ModelFile {
            name: "f.bin".into(),
            url: format!("{base}/f.bin"),
            sha256: sha(body),
            size: body.len() as u64,
        }],
    }
}

fn body(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8).collect()
}

#[test]
fn catalog_has_default_parakeet() {
    let m = find("parakeet-tdt-0.6b-v2-int8").unwrap();
    assert_eq!(m.kind, EngineKind::Parakeet);
    assert_eq!(m.files.len(), 4);
    assert!(find("whisper-large-v3-turbo-q5").is_some());
}

#[test]
fn installs_and_reports_installed() {
    let b = body(3 * 1024 * 1024);
    let (base, _) = serve(b.clone());
    let m = info(&base, &b);
    let dir = tempfile::tempdir().unwrap();
    assert!(!is_installed(dir.path(), &m));
    let last = Mutex::new((0, 0));
    install(
        dir.path(),
        &m,
        &|d, t| *last.lock().unwrap() = (d, t),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(is_installed(dir.path(), &m));
    let n = b.len() as u64;
    assert_eq!(*last.lock().unwrap(), (n, n));
    assert_eq!(
        std::fs::read(model_dir(dir.path(), "m").join("f.bin")).unwrap(),
        b
    );
}

#[test]
fn resumes_partial_download() {
    let b = body(3 * 1024 * 1024);
    let (base, ranges) = serve(b.clone());
    let m = info(&base, &b);
    let dir = tempfile::tempdir().unwrap();
    let md = model_dir(dir.path(), "m");
    std::fs::create_dir_all(&md).unwrap();
    std::fs::write(md.join("f.bin.part"), &b[..1024 * 1024]).unwrap();
    install(dir.path(), &m, &|_, _| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(std::fs::read(md.join("f.bin")).unwrap(), b);
    assert_eq!(
        *ranges.lock().unwrap(),
        vec![Some("bytes=1048576-".to_string())]
    );
}

#[test]
fn checksum_mismatch_leaves_no_file() {
    let b = body(100_000);
    let (base, _) = serve(b.clone());
    let mut m = info(&base, &b);
    m.files[0].sha256 = sha(b"other");
    let dir = tempfile::tempdir().unwrap();
    let r = install(dir.path(), &m, &|_, _| {}, &AtomicBool::new(false));
    assert!(matches!(r, Err(ModelError::ChecksumMismatch { ref file }) if file == "f.bin"));
    let md = model_dir(dir.path(), "m");
    assert!(!md.join("f.bin").exists());
    assert!(!md.join("f.bin.part").exists());
    assert!(!is_installed(dir.path(), &m));
}

#[test]
fn cancel_keeps_part() {
    let b = body(100_000);
    let (base, _) = serve(b.clone());
    let m = info(&base, &b);
    let dir = tempfile::tempdir().unwrap();
    let r = install(dir.path(), &m, &|_, _| {}, &AtomicBool::new(true));
    assert!(matches!(r, Err(ModelError::Cancelled)));
    assert!(!model_dir(dir.path(), "m").join("f.bin").exists());
}

#[test]
fn import_verifies() {
    let b = body(5000);
    let m = info("http://unused", &b);
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src.bin");
    std::fs::write(&src, &b).unwrap();
    let root = dir.path().join("models");
    import_file(&root, &m, "f.bin", &src).unwrap();
    assert!(is_installed(&root, &m));

    let bad = dir.path().join("bad.bin");
    std::fs::write(&bad, b"nope").unwrap();
    let root2 = dir.path().join("models2");
    let r = import_file(&root2, &m, "f.bin", &bad);
    assert!(matches!(r, Err(ModelError::ChecksumMismatch { .. })));
    assert!(!is_installed(&root2, &m));
    let r = import_file(&root2, &m, "x.bin", Path::new(&src));
    assert!(matches!(r, Err(ModelError::UnknownFile(ref n)) if n == "x.bin"));
}
