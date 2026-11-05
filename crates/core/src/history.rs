//! SQLite-backed dictation history with FTS5 search, audio retention and export.

use crate::settings::Retention;
use crate::wav::write_wav;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, UNIX_EPOCH};
use ulid::Ulid;

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, HistoryError>;

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pasted,
    CopiedOnly,
    NoSpeech,
    Error,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pasted => "pasted",
            Status::CopiedOnly => "copied_only",
            Status::NoSpeech => "no_speech",
            Status::Error => "error",
        }
    }

    fn parse(s: &str) -> Status {
        match s {
            "pasted" => Status::Pasted,
            "copied_only" => Status::CopiedOnly,
            "no_speech" => Status::NoSpeech,
            _ => Status::Error,
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct NewDictation {
    pub created_at_ms: i64,
    pub duration_ms: i64,
    pub text: String,
    pub raw_text: String,
    pub model: String,
    pub target_app: Option<String>,
    pub status: Status,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Dictation {
    pub id: String,
    pub created_at_ms: i64,
    pub duration_ms: i64,
    pub text: String,
    pub raw_text: String,
    pub model: String,
    pub target_app: Option<String>,
    pub status: Status,
    pub audio_path: Option<String>,
}

const COLS: &str =
    "id, created_at, duration_ms, text, raw_text, model, target_app, status, audio_path";

fn row_to_dictation(r: &Row) -> rusqlite::Result<Dictation> {
    let status: String = r.get(7)?;
    Ok(Dictation {
        id: r.get(0)?,
        created_at_ms: r.get(1)?,
        duration_ms: r.get(2)?,
        text: r.get(3)?,
        raw_text: r.get(4)?,
        model: r.get(5)?,
        target_app: r.get(6)?,
        status: Status::parse(&status),
        audio_path: r.get(8)?,
    })
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS dictations (
    id          TEXT PRIMARY KEY,
    created_at  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    text        TEXT NOT NULL,
    raw_text    TEXT NOT NULL,
    model       TEXT NOT NULL,
    target_app  TEXT,
    status      TEXT NOT NULL,
    audio_path  TEXT
);
CREATE INDEX IF NOT EXISTS dictations_created_at ON dictations(created_at);
CREATE VIRTUAL TABLE IF NOT EXISTS dictations_fts USING fts5(
    text, content='dictations', content_rowid='rowid'
);
CREATE TRIGGER IF NOT EXISTS dictations_ai AFTER INSERT ON dictations BEGIN
    INSERT INTO dictations_fts(rowid, text) VALUES (new.rowid, new.text);
END;
CREATE TRIGGER IF NOT EXISTS dictations_ad AFTER DELETE ON dictations BEGIN
    INSERT INTO dictations_fts(dictations_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
END;
CREATE TRIGGER IF NOT EXISTS dictations_au AFTER UPDATE ON dictations BEGIN
    INSERT INTO dictations_fts(dictations_fts, rowid, text) VALUES ('delete', old.rowid, old.text);
    INSERT INTO dictations_fts(rowid, text) VALUES (new.rowid, new.text);
END;
PRAGMA user_version = 1;
";

pub struct History {
    dir: PathBuf,
    conn: Mutex<Connection>,
}

/// Quotes each whitespace-separated token and makes the last one a prefix match.
fn sanitize_query(q: &str) -> String {
    let toks: Vec<String> = q
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    let mut out = toks.join(" ");
    if !out.is_empty() {
        out.push('*');
    }
    out
}

fn remove_file_if_present(path: &Path) -> io::Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn dir_size(dir: &Path) -> io::Result<u64> {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e),
    };
    let mut total = 0;
    for entry in rd {
        let entry = entry?;
        let meta = entry.metadata()?;
        total += if meta.is_dir() {
            dir_size(&entry.path())?
        } else {
            meta.len()
        };
    }
    Ok(total)
}

impl History {
    pub fn open(data_dir: &Path) -> Result<History> {
        std::fs::create_dir_all(data_dir)?;
        let conn = Connection::open(data_dir.join("history.db"))?;
        conn.execute_batch(SCHEMA)?;
        Ok(History {
            dir: data_dir.to_path_buf(),
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn abs_path(&self, rel: &str) -> PathBuf {
        rel.split('/')
            .fold(self.dir.clone(), |p, part| p.join(part))
    }

    pub fn add(&self, d: NewDictation, audio: Option<&[f32]>) -> Result<Dictation> {
        let when = UNIX_EPOCH + Duration::from_millis(d.created_at_ms.max(0) as u64);
        let id = Ulid::from_datetime(when).to_string();
        let audio_path = match audio {
            Some(samples) => {
                let dt = chrono::DateTime::<chrono::Utc>::from(when);
                let rel = format!("recordings/{}/{}.wav", dt.format("%Y/%m"), id);
                write_wav(&self.abs_path(&rel), samples)?;
                Some(rel)
            }
            None => None,
        };
        let x = Dictation {
            id,
            created_at_ms: d.created_at_ms,
            duration_ms: d.duration_ms,
            text: d.text,
            raw_text: d.raw_text,
            model: d.model,
            target_app: d.target_app,
            status: d.status,
            audio_path,
        };
        self.conn().execute(
            "INSERT INTO dictations (id, created_at, duration_ms, text, raw_text, model, \
             target_app, status, audio_path) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                x.id,
                x.created_at_ms,
                x.duration_ms,
                x.text,
                x.raw_text,
                x.model,
                x.target_app,
                x.status.as_str(),
                x.audio_path
            ],
        )?;
        Ok(x)
    }

    pub fn get(&self, id: &str) -> Result<Option<Dictation>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {COLS} FROM dictations WHERE id = ?1"),
                [id],
                row_to_dictation,
            )
            .optional()?)
    }

    pub fn list(&self, before_id: Option<&str>, limit: u32) -> Result<Vec<Dictation>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM dictations WHERE (?1 IS NULL OR id < ?1) \
             ORDER BY id DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![before_id, limit], row_to_dictation)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn search(&self, q: &str, limit: u32) -> Result<Vec<Dictation>> {
        let m = sanitize_query(q);
        if m.is_empty() {
            return self.list(None, limit);
        }
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT d.id, d.created_at, d.duration_ms, d.text, d.raw_text, d.model, \
             d.target_app, d.status, d.audio_path FROM dictations d \
             WHERE d.rowid IN (SELECT rowid FROM dictations_fts WHERE dictations_fts MATCH ?1) \
             ORDER BY d.id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![m, limit], row_to_dictation)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn update_transcript(
        &self,
        id: &str,
        raw: &str,
        text: &str,
        model: &str,
        status: Status,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE dictations SET raw_text = ?2, text = ?3, model = ?4, status = ?5 \
             WHERE id = ?1",
            params![id, raw, text, model, status.as_str()],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let rel = self.get(id)?.and_then(|d| d.audio_path);
        self.conn()
            .execute("DELETE FROM dictations WHERE id = ?1", [id])?;
        if let Some(rel) = rel {
            remove_file_if_present(&self.abs_path(&rel))?;
        }
        Ok(())
    }

    /// Removes audio files per the retention policy; returns how many files were deleted.
    pub fn apply_retention(&self, now_ms: i64, r: Retention) -> Result<u32> {
        let cutoff = match r {
            Retention::Forever => return Ok(0),
            Retention::Days(n) => now_ms - i64::from(n) * 86_400_000,
            Retention::Never => i64::MAX,
        };
        let conn = self.conn();
        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT id, audio_path FROM dictations \
                 WHERE audio_path IS NOT NULL AND created_at < ?1",
            )?;
            let rows = stmt.query_map([cutoff], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut removed = 0;
        for (id, rel) in rows {
            if remove_file_if_present(&self.abs_path(&rel))? {
                removed += 1;
            }
            conn.execute(
                "UPDATE dictations SET audio_path = NULL WHERE id = ?1",
                [id],
            )?;
        }
        Ok(removed)
    }

    pub fn storage_bytes(&self) -> Result<u64> {
        Ok(dir_size(&self.dir.join("recordings"))?)
    }

    fn all(&self) -> Result<Vec<Dictation>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM dictations ORDER BY id DESC"))?;
        let rows = stmt.query_map([], row_to_dictation)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn export_json(&self, out: &Path) -> Result<()> {
        let json = serde_json::to_vec_pretty(&self.all()?).map_err(io::Error::from)?;
        std::fs::write(out, json)?;
        Ok(())
    }

    pub fn export_csv(&self, out: &Path) -> Result<()> {
        let mut s =
            String::from("id,created_at,duration_ms,text,raw_text,model,target_app,status\n");
        for d in self.all()? {
            let fields = [
                csv_field(&d.id),
                d.created_at_ms.to_string(),
                d.duration_ms.to_string(),
                csv_field(&d.text),
                csv_field(&d.raw_text),
                csv_field(&d.model),
                csv_field(d.target_app.as_deref().unwrap_or("")),
                d.status.as_str().to_string(),
            ];
            s.push_str(&fields.join(","));
            s.push('\n');
        }
        std::fs::write(out, s)?;
        Ok(())
    }

    pub fn export_audio_zip(&self, out: &Path) -> Result<()> {
        let to_io = |e: zip::result::ZipError| io::Error::other(e);
        let mut zw = zip::ZipWriter::new(std::fs::File::create(out)?);
        let opts = zip::write::SimpleFileOptions::default();
        for d in self.all()? {
            let Some(rel) = d.audio_path else { continue };
            let bytes = match std::fs::read(self.abs_path(&rel)) {
                Ok(b) => b,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            let name = rel.strip_prefix("recordings/").unwrap_or(&rel);
            zw.start_file(name, opts).map_err(to_io)?;
            zw.write_all(&bytes)?;
        }
        zw.finish().map_err(to_io)?;
        Ok(())
    }
}
