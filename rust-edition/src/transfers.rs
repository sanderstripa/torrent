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

    pub fn pause(&mut self, index: usize, stopped: bool) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(index)
            .ok_or("Torrent is no longer in the queue")?;
        entry.handle.set_paused(true, false)?;
        entry.stopped = stopped;
        Ok(())
    }

    pub fn resume(&mut self, index: usize) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(index)
            .ok_or("Torrent is no longer in the queue")?;
        entry.handle.set_paused(false, !entry.selection_pending)?;
        entry.stopped = false;
        Ok(())
    }

    pub fn select_files(&mut self, index: usize, priorities: &[u8]) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(index)
            .ok_or("Torrent is no longer in the queue")?;
        entry.handle.select_files(priorities)?;
        entry.selection_pending = false;
        entry.stopped = false;
        Ok(())
    }

    pub fn move_row(&mut self, index: usize, up: bool) -> Result<usize, String> {
        let entry = self
            .entries
            .get(index)
            .ok_or("Torrent is no longer in the queue")?;
        let other = if up {
            index.saturating_sub(1)
        } else {
            (index + 1).min(self.entries.len() - 1)
        };
        if index != other {
            entry.handle.move_queue(up)?;
            self.entries.swap(index, other);
            self.save_order()?;
        }
        Ok(other)
    }

    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        let entry = self
            .entries
            .get(index)
            .ok_or("Torrent is no longer in the queue")?;
        profile::remove_resume_entry(&self.root, &entry.key).map_err(|error| error.to_string())?;
        entry.handle.remove()?;
        self.entries.remove(index);
        self.save_order()
    }

    fn save_order(&self) -> Result<(), String> {
        let keys: Vec<_> = self
            .entries
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        profile::save_queue_order(&self.root, &keys).map_err(|error| error.to_string())
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
        self.save_order()
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
            assert_eq!(queue.move_row(1, true)?, 0);
            queue.pause(0, true)?;
            queue.save()?;
        }
        let reopened = Session::open(PREVIEW_POLICY, true)?;
        let (mut queue, warnings) = Transfers::open(&reopened, &root)?;
        assert!(warnings.is_empty(), "{warnings:?}");
        let keys: Vec<_> = queue
            .ordered()
            .iter()
            .map(|entry| entry.key.as_str())
            .collect();
        assert_eq!(keys, [second, first]);
        assert!(queue.ordered()[0].stopped);
        assert!(queue.ordered().iter().all(|entry| entry.selection_pending));
        queue.remove(0)?;
        assert_eq!(profile::load_resume_entries(&root)?[0].key, first);
        assert_eq!(profile::load_resume_entries(&root)?.len(), 1);
        Ok(())
    })();
    std::fs::remove_dir_all(root)?;
    result
}
