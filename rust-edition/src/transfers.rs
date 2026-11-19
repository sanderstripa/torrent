//! Owns the Rust application's ordered torrent handles and legacy profile records.
use crate::engine::{Session, Torrent};
use crate::profile::{self, ResumeEntry};
use std::path::{Path, PathBuf};

pub struct Transfer<'session> {
    pub key: String,
    pub stopped: bool,
    pub selection_pending: bool,
    pub handle: Torrent<'session>,
}

pub struct Transfers<'session> {
    session: &'session Session,
    root: PathBuf,
    entries: Vec<Transfer<'session>>,
}

impl<'session> Transfers<'session> {
    pub fn open(session: &'session Session, root: &Path) -> Result<(Self, Vec<String>), String> {
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        for saved in profile::load_resume_entries(root).map_err(|error| error.to_string())? {
            match session.restore(&saved.data) {
                Ok(handle) => entries.push(Transfer {
                    key: saved.key,
                    stopped: saved.stopped,
                    selection_pending: saved.selection_pending,
                    handle,
                }),
                Err(error) => warnings.push(format!("{}: {error}", saved.key)),
            }
        }
        Ok((
            Self {
                session,
                root: root.to_owned(),
                entries,
            },
            warnings,
        ))
    }

    pub fn ordered(&self) -> &[Transfer<'session>] {
        &self.entries
    }

    pub fn add(&mut self, source: &str, destination: &str, select: bool) -> Result<usize, String> {
        let handle = self.session.add(source, destination, select)?;
        let key = handle.key()?;
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            return Ok(index);
        }
        self.entries.push(Transfer {
            key,
            stopped: false,
            selection_pending: select,
            handle,
        });
        Ok(self.entries.len() - 1)
    }

    pub fn save(&self) -> Result<(), String> {
        // Capture live resume data first. A failed checkpoint leaves all legacy
        // profile files intact, and disk replacements are atomic per file.
        let saved = self
            .entries
            .iter()
            .map(|entry| {
                Ok(ResumeEntry {
                    key: entry.key.clone(),
                    stopped: entry.stopped,
                    selection_pending: entry.selection_pending,
                    data: entry.handle.checkpoint()?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        for entry in &saved {
            entry.save(&self.root).map_err(|error| error.to_string())?;
        }
        let keys: Vec<_> = saved.iter().map(|entry| entry.key.as_str()).collect();
        profile::save_queue_order(&self.root, &keys).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[test]
fn open_save_and_restore_deduplicated_ordered_queue() -> Result<(), Box<dyn std::error::Error>> {
    use crate::queue_policy::PREVIEW_POLICY;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("torrent-re-queue-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&root)?;
    let result = (|| {
        let first = "0123456789012345678901234567890123456789";
        let second = "abcdef0123456789abcdef0123456789abcdef01";
        {
            let session = Session::open(PREVIEW_POLICY, true)?;
            let (mut queue, warnings) = Transfers::open(&session, &root)?;
            assert!(warnings.is_empty());
            let first_uri = format!("magnet:?xt=urn:btih:{first}");
            let second_uri = format!("magnet:?xt=urn:btih:{second}");
            assert_eq!(queue.add(&first_uri, ".", true)?, 0);
            assert_eq!(queue.add(&second_uri, ".", true)?, 1);
            assert_eq!(queue.add(&first_uri, ".", true)?, 0);
            queue.save()?;
        }
        let reopened = Session::open(PREVIEW_POLICY, true)?;
        let (queue, warnings) = Transfers::open(&reopened, &root)?;
        assert!(warnings.is_empty(), "{warnings:?}");
        let keys: Vec<_> = queue
            .ordered()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(keys, [first, second]);
        assert!(queue.ordered().iter().all(|entry| entry.selection_pending));
        Ok(())
    })();
    std::fs::remove_dir_all(root)?;
    result
}
