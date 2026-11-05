use dictation_core::history::{History, NewDictation, Status};
use dictation_core::settings::Retention;
use tempfile::tempdir;

fn nd(ms: i64, text: &str) -> NewDictation {
    NewDictation {
        created_at_ms: ms,
        duration_ms: 1000,
        text: text.into(),
        raw_text: text.into(),
        model: "m".into(),
        target_app: None,
        status: Status::Pasted,
    }
}

#[test]
fn add_with_audio_writes_file() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    let x = h
        .add(nd(1_762_300_000_000, "hello"), Some(&[0.1; 1600]))
        .unwrap();
    let rel = x.audio_path.clone().unwrap();
    assert!(rel.starts_with("recordings/2025/11/") && rel.ends_with(&format!("{}.wav", x.id)));
    assert!(h.abs_path(&rel).exists());
    assert_eq!(h.get(&x.id).unwrap(), Some(x));
}

#[test]
fn list_newest_first_and_paginates() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    h.add(nd(1000, "one"), None).unwrap();
    let b = h.add(nd(2000, "two"), None).unwrap();
    h.add(nd(3000, "three"), None).unwrap();
    let page: Vec<i64> = h
        .list(None, 2)
        .unwrap()
        .iter()
        .map(|x| x.created_at_ms)
        .collect();
    assert_eq!(page, vec![3000, 2000]);
    let page: Vec<i64> = h
        .list(Some(&b.id), 2)
        .unwrap()
        .iter()
        .map(|x| x.created_at_ms)
        .collect();
    assert_eq!(page, vec![1000]);
}

#[test]
fn search_prefix_and_word() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    h.add(nd(1000, "We deploy on Kubernetes"), None).unwrap();
    h.add(nd(2000, "Lunch at noon"), None).unwrap();
    let r = h.search("kube", 10).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].text, "We deploy on Kubernetes");
    assert_eq!(h.search("noon", 10).unwrap().len(), 1);
}

#[test]
fn search_tolerates_fts_syntax() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    h.add(nd(1, r#"he said "hi" (AND) more"#), None).unwrap();
    for q in [r#"he said "hi"#, "AND (", "*", "\"", "NEAR(a b)", ""] {
        assert!(h.search(q, 10).is_ok(), "{q}");
    }
    assert_eq!(h.search("(AND)", 10).unwrap().len(), 1);
}

#[test]
fn delete_removes_row_file_and_index() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    let x = h.add(nd(5000, "gamma"), Some(&[0.1; 1600])).unwrap();
    let path = h.abs_path(x.audio_path.as_deref().unwrap());
    assert!(path.exists());
    h.delete(&x.id).unwrap();
    assert_eq!(h.get(&x.id).unwrap(), None);
    assert!(!path.exists());
    assert_eq!(h.search("gamma", 10).unwrap().len(), 0);
}

#[test]
fn update_transcript_reindexes() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    let x = h.add(nd(5000, "alpha"), None).unwrap();
    h.update_transcript(&x.id, "beta", "beta", "m2", Status::Error)
        .unwrap();
    assert_eq!(h.search("beta", 10).unwrap().len(), 1);
    assert_eq!(h.search("alpha", 10).unwrap().len(), 0);
    let got = h.get(&x.id).unwrap().unwrap();
    assert_eq!(got.status, Status::Error);
    assert_eq!(got.model, "m2");
}

#[test]
fn retention_days_and_never() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    let day = 86_400_000i64;
    let now = 100 * day;
    let old = h.add(nd(now - 8 * day, "old"), Some(&[0.1; 1600])).unwrap();
    let new = h.add(nd(now - day, "new"), Some(&[0.1; 1600])).unwrap();
    let old_path = h.abs_path(old.audio_path.as_deref().unwrap());
    let new_path = h.abs_path(new.audio_path.as_deref().unwrap());

    assert_eq!(h.apply_retention(now, Retention::Days(7)).unwrap(), 1);
    assert_eq!(h.get(&old.id).unwrap().unwrap().audio_path, None);
    assert!(!old_path.exists());
    assert_eq!(h.get(&new.id).unwrap(), Some(new.clone()));
    assert!(new_path.exists());

    std::fs::remove_file(&new_path).unwrap();
    assert_eq!(h.apply_retention(now, Retention::Never).unwrap(), 0);
    assert_eq!(h.get(&new.id).unwrap().unwrap().audio_path, None);
}

#[test]
fn storage_bytes_counts_recordings() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    assert_eq!(h.storage_bytes().unwrap(), 0);
    h.add(nd(1000, "x"), Some(&vec![0.1; 16000])).unwrap();
    assert!(h.storage_bytes().unwrap() >= 32_000);
}

#[test]
fn exports() {
    let d = tempdir().unwrap();
    let h = History::open(d.path()).unwrap();
    h.add(nd(1000, "a, \"b\"\nc"), Some(&[0.1; 1600])).unwrap();
    h.add(nd(2000, "plain"), None).unwrap();
    let out = tempdir().unwrap();

    let j = out.path().join("h.json");
    h.export_json(&j).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&j).unwrap()).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 2);

    let c = out.path().join("h.csv");
    h.export_csv(&c).unwrap();
    let s = std::fs::read_to_string(&c).unwrap();
    assert_eq!(
        s.lines().next().unwrap(),
        "id,created_at,duration_ms,text,raw_text,model,target_app,status"
    );
    assert!(s.contains("\"a, \"\"b\"\"\nc\""));

    let z = out.path().join("a.zip");
    h.export_audio_zip(&z).unwrap();
    let ar = zip::ZipArchive::new(std::fs::File::open(&z).unwrap()).unwrap();
    assert_eq!(ar.len(), 1);
}
