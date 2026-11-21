//! Local SQLite state: prevents uploading the same video twice and allows resuming stage by stage.
//! The schema is the same as AutoUpload 1.2 (`PRAGMA user_version = 1`), so existing databases keep working.

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 1;

pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub manifest_id: String,
    pub video_id: Option<String>,
    pub stages: Vec<String>,
    pub session: Option<String>,
}

impl Record {
    pub fn has(&self, stage: &str) -> bool {
        self.stages.iter().any(|s| s == stage)
    }
}

pub struct State {
    con: Connection,
}

fn now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
}

impl State {
    pub fn open(db: &Path) -> rusqlite::Result<State> {
        if let Some(dir) = db.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let con = Connection::open(db)?;
        con.busy_timeout(std::time::Duration::from_secs(30))?;
        con.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        con.execute_batch(
            "CREATE TABLE IF NOT EXISTS uploads(
                manifest_id TEXT NOT NULL, sha256 TEXT NOT NULL, channel_id TEXT NOT NULL,
                video_id TEXT, stages TEXT NOT NULL DEFAULT '', updated TEXT NOT NULL,
                PRIMARY KEY(sha256, channel_id));
             CREATE TABLE IF NOT EXISTS log(ts TEXT NOT NULL, manifest_id TEXT, event TEXT NOT NULL, detail TEXT);",
        )?;
        let st = State { con };
        st.migrate()?;
        Ok(st)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        let version = self.schema_version()?;
        if version > SCHEMA_VERSION {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "state database is from a newer AutoUpload (schema {version}); update AutoUpload"
            )));
        }
        if version < 1 {
            let mut stmt = self.con.prepare("PRAGMA table_info(uploads)")?;
            let cols: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<_, _>>()?;
            if !cols.iter().any(|c| c == "session") {
                self.con
                    .execute_batch("ALTER TABLE uploads ADD COLUMN session TEXT")?;
            }
        }
        self.con
            .execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION}"))
    }

    pub fn schema_version(&self) -> rusqlite::Result<i64> {
        self.con.query_row("PRAGMA user_version", [], |r| r.get(0))
    }

    pub fn find(&self, sha: &str, channel_id: &str) -> rusqlite::Result<Option<Record>> {
        self.con
            .query_row(
                "SELECT manifest_id, video_id, stages, session FROM uploads WHERE sha256=?1 AND channel_id=?2",
                params![sha, channel_id],
                |r| {
                    let stages: String = r.get(2)?;
                    Ok(Record {
                        manifest_id: r.get(0)?,
                        video_id: r.get(1)?,
                        stages: stages.split(',').filter(|s| !s.is_empty()).map(String::from).collect(),
                        session: r.get(3)?,
                    })
                },
            )
            .optional()
    }

    /// Adds a stage and/or sets the video ID or the upload session. Once a video ID exists the session is dropped.
    pub fn record(
        &self,
        manifest_id: &str,
        sha: &str,
        channel_id: &str,
        video_id: Option<&str>,
        stage: Option<&str>,
        session: Option<&str>,
    ) -> rusqlite::Result<()> {
        let cur = self.find(sha, channel_id)?;
        let mut stages = cur.as_ref().map(|c| c.stages.clone()).unwrap_or_default();
        if let Some(s) = stage {
            if !stages.iter().any(|x| x == s) {
                stages.push(s.to_string());
            }
        }
        let vid = video_id
            .map(String::from)
            .or_else(|| cur.as_ref().and_then(|c| c.video_id.clone()));
        let mut sess = session
            .map(String::from)
            .or_else(|| cur.as_ref().and_then(|c| c.session.clone()));
        if vid.is_some() {
            sess = None;
        }
        self.con.execute(
            "INSERT INTO uploads(manifest_id, sha256, channel_id, video_id, stages, updated, session)
             VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(sha256, channel_id) DO UPDATE SET
             manifest_id=excluded.manifest_id, video_id=excluded.video_id, stages=excluded.stages,
             updated=excluded.updated, session=excluded.session",
            params![manifest_id, sha, channel_id, vid, stages.join(","), now(), sess],
        )?;
        Ok(())
    }

    pub fn clear_session(&self, sha: &str, channel_id: &str) -> rusqlite::Result<()> {
        self.con.execute(
            "UPDATE uploads SET session=NULL WHERE sha256=?1 AND channel_id=?2",
            params![sha, channel_id],
        )?;
        Ok(())
    }

    pub fn forget(&self, sha: &str, channel_id: &str) -> rusqlite::Result<()> {
        self.con.execute(
            "DELETE FROM uploads WHERE sha256=?1 AND channel_id=?2",
            params![sha, channel_id],
        )?;
        Ok(())
    }

    pub fn event(&self, manifest_id: &str, event: &str, detail: &str) -> rusqlite::Result<()> {
        self.con.execute(
            "INSERT INTO log VALUES(?1,?2,?3,?4)",
            params![now(), manifest_id, event, detail],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_a_1_1_database() {
        let d = tempfile::tempdir().unwrap();
        let db = d.path().join("state.sqlite");
        {
            let con = Connection::open(&db).unwrap();
            con.execute_batch(
                "CREATE TABLE uploads(manifest_id TEXT NOT NULL, sha256 TEXT NOT NULL, channel_id TEXT NOT NULL,
                 video_id TEXT, stages TEXT NOT NULL DEFAULT '', updated TEXT NOT NULL, PRIMARY KEY(sha256, channel_id));
                 INSERT INTO uploads VALUES('m','s','c','V9','validated,uploaded','2026-11-20');",
            )
            .unwrap();
        }
        let st = State::open(&db).unwrap();
        assert_eq!(st.schema_version().unwrap(), SCHEMA_VERSION);
        let r = st.find("s", "c").unwrap().unwrap();
        assert_eq!(r.video_id.as_deref(), Some("V9"));
        assert_eq!(r.stages, vec!["validated", "uploaded"]);
        assert_eq!(r.session, None);
    }

    #[test]
    fn session_is_dropped_once_uploaded() {
        let d = tempfile::tempdir().unwrap();
        let st = State::open(&d.path().join("s.sqlite")).unwrap();
        st.record(
            "m",
            "s",
            "c",
            None,
            Some("upload_started"),
            Some("https://u/1"),
        )
        .unwrap();
        assert_eq!(
            st.find("s", "c").unwrap().unwrap().session.as_deref(),
            Some("https://u/1")
        );
        st.record("m", "s", "c", Some("V1"), Some("uploaded"), None)
            .unwrap();
        let r = st.find("s", "c").unwrap().unwrap();
        assert!(r.session.is_none() && r.has("uploaded") && r.has("upload_started"));
    }
}
