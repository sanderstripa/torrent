use crate::queue_policy::QueuePolicy;
use librqbit::{
    api::TorrentIdOrHash, AddTorrent, AddTorrentOptions, AddTorrentResponse, Magnet,
    ManagedTorrent, SessionOptions,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

const CHECKPOINT_FORMAT: u8 = 1;

#[derive(Debug, Default)]
pub struct TransferStatus {
    pub wanted: i64,
    pub wanted_done: i64,
    pub progress_ppm: i32,
    pub download_rate: i32,
    pub paused: bool,
    pub has_metadata: bool,
    pub seeding: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TorrentFile {
    pub size: i64,
    pub path: String,
}

#[derive(Clone, Serialize, Deserialize)]
enum Source {
    Magnet(String),
    Metadata(Vec<u8>),
}

#[derive(Clone, Serialize, Deserialize)]
struct Checkpoint {
    version: u8,
    key: String,
    source: Source,
    destination: String,
    paused: bool,
    selecting: bool,
    priorities: Option<Vec<u8>>,
}

pub struct Session {
    backend: Arc<librqbit::Session>,
    runtime: Runtime,
}

impl Session {
    pub fn open(policy: QueuePolicy, offline: bool) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .map_err(|error| error.to_string())?;
        let mut options = SessionOptions {
            peer_limit: usize::try_from(policy.connections_limit).ok(),
            ..SessionOptions::default()
        };
        if offline {
            options.dht = None;
            options.listen = None;
            options.disable_trackers = true;
            options.disable_local_service_discovery = true;
            options.disable_web_seeds = true;
        }
        let backend = runtime
            .block_on(librqbit::Session::new_with_opts(PathBuf::new(), options))
            .map_err(|error| error.to_string())?;
        Ok(Self { backend, runtime })
    }

    pub fn add(
        &self,
        source: &str,
        destination: &str,
        select: bool,
    ) -> Result<Torrent<'_>, String> {
        let (source, key, label) = if source.starts_with("magnet:") {
            let parsed = Magnet::parse(source).map_err(|error| error.to_string())?;
            let key = parsed
                .as_id32()
                .map(|hash| hash.as_string())
                .or_else(|| parsed.as_id20().map(|hash| hash.as_string()))
                .ok_or("Magnet has no info hash")?;
            (Source::Magnet(source.to_owned()), key.clone(), key)
        } else {
            let bytes = std::fs::read(source).map_err(|error| error.to_string())?;
            let metadata =
                librqbit::torrent_from_bytes(&bytes).map_err(|error| error.to_string())?;
            let key = metadata
                .info_hash_v2
                .map(|hash| hash.as_string())
                .unwrap_or_else(|| metadata.info_hash.as_string());
            let label = Path::new(source)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (Source::Metadata(bytes), key, label)
        };
        Ok(self.start(
            Checkpoint {
                version: CHECKPOINT_FORMAT,
                key,
                source,
                destination: destination.to_owned(),
                selecting: select,
                paused: false,
                priorities: None,
            },
            label,
        ))
    }

    pub fn restore(&self, bytes: &[u8]) -> Result<Torrent<'_>, String> {
        let saved: Checkpoint = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if saved.version != CHECKPOINT_FORMAT {
            return Err("Unsupported Rust Edition checkpoint version".into());
        }
        let label = saved.key.clone();
        Ok(self.start(saved, label))
    }

    fn start(&self, saved: Checkpoint, label: String) -> Torrent<'_> {
        Torrent {
            session: self,
            label,
            saved: RefCell::new(saved),
            state: RefCell::new(State::NotStarted),
        }
    }

    fn add_pending(&self, pending: Checkpoint) -> JoinHandle<Result<Arc<ManagedTorrent>, String>> {
        let session = self.backend.clone();
        self.runtime.spawn(async move {
            let source = match pending.source {
                Source::Magnet(text) => AddTorrent::from_url(text),
                Source::Metadata(bytes) => AddTorrent::from_bytes(bytes),
            };
            let response = session
                .add_torrent(
                    source,
                    Some(AddTorrentOptions {
                        paused: true,
                        overwrite: true,
                        output_folder: Some(pending.destination),
                        only_files: pending.selecting.then(Vec::new),
                        ..Default::default()
                    }),
                )
                .await
                .map_err(|error| error.to_string())?;
            match response {
                AddTorrentResponse::Added(_, handle) => Ok(handle),
                AddTorrentResponse::AlreadyManaged(_, _) => {
                    Err("Torrent is already in the queue".to_owned())
                }
                AddTorrentResponse::ListOnly(_) => Err("Torrent was not added".to_owned()),
            }
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.runtime.block_on(self.backend.stop());
    }
}

enum State {
    NotStarted,
    Pending(JoinHandle<Result<Arc<ManagedTorrent>, String>>),
    Initializing(Arc<ManagedTorrent>),
    Ready(Arc<ManagedTorrent>),
    Failed(String),
}

pub struct Torrent<'a> {
    session: &'a Session,
    label: String,
    saved: RefCell<Checkpoint>,
    state: RefCell<State>,
}

impl Torrent<'_> {
    fn handle(&self) -> Result<Option<Arc<ManagedTorrent>>, String> {
        let mut state = self.state.borrow_mut();
        if matches!(*state, State::NotStarted) {
            *state = State::Pending(self.session.add_pending(self.saved.borrow().clone()));
        }
        if let State::Pending(task) = &mut *state {
            if !task.is_finished() {
                return Ok(None);
            }
            *state = match self.session.runtime.block_on(task) {
                Ok(Ok(handle)) => State::Initializing(handle),
                Ok(Err(error)) => State::Failed(error),
                Err(error) => State::Failed(error.to_string()),
            };
        }

        if let State::Initializing(handle) = &*state {
            let stats = handle.stats();
            if let Some(error) = stats.error {
                return Err(error);
            } else if !matches!(
                stats.state,
                librqbit::TorrentStatsState::Initializing { .. }
            ) {
                let handle = handle.clone();
                let saved = self.saved.borrow();
                self.session
                    .runtime
                    .block_on(async {
                        if let Some(priorities) = &saved.priorities {
                            self.session
                                .backend
                                .update_file_priorities(&handle, priorities)
                                .await?;
                        }
                        if !saved.selecting && !saved.paused {
                            self.session.backend.unpause(&handle).await?;
                        }
                        Ok::<_, Box<dyn std::error::Error>>(())
                    })
                    .map_err(|error| error.to_string())?;
                *state = State::Ready(handle);
            }
        }

        match &*state {
            State::NotStarted | State::Pending(_) | State::Initializing(_) => Ok(None),
            State::Ready(handle) => Ok(Some(handle.clone())),
            State::Failed(error) => Err(error.clone()),
        }
    }

    pub fn key(&self) -> Result<String, String> {
        Ok(self.saved.borrow().key.clone())
    }

    pub fn set_paused(&self, paused: bool, _auto_managed: bool) -> Result<(), String> {
        if let Some(handle) = self.handle()? {
            if handle.is_paused() != paused && !self.saved.borrow().selecting {
                self.session
                    .runtime
                    .block_on(async {
                        if paused {
                            self.session.backend.pause(&handle).await
                        } else {
                            self.session.backend.unpause(&handle).await
                        }
                    })
                    .map_err(|error| error.to_string())?;
            }
        }
        self.saved.borrow_mut().paused = paused;
        Ok(())
    }

    pub fn status(&self) -> Result<(String, TransferStatus), String> {
        let Some(handle) = self.handle()? else {
            return Ok((
                self.label.clone(),
                TransferStatus {
                    paused: self.saved.borrow().paused,
                    ..Default::default()
                },
            ));
        };
        let stats = handle.stats();
        if let Some(error) = stats.error {
            return Err(error);
        }
        let progress = if stats.total_bytes == 0 {
            0
        } else {
            (u128::from(stats.progress_bytes) * 1_000_000 / u128::from(stats.total_bytes))
                .min(1_000_000) as i32
        };
        Ok((
            handle.name().unwrap_or_else(|| self.label.clone()),
            TransferStatus {
                wanted: i64::try_from(stats.total_bytes).unwrap_or(i64::MAX),
                wanted_done: i64::try_from(stats.progress_bytes).unwrap_or(i64::MAX),
                progress_ppm: progress,
                download_rate: stats
                    .live
                    .map(|live| i32::try_from(live.download_speed.as_bytes()).unwrap_or(i32::MAX))
                    .unwrap_or(0),
                paused: self.saved.borrow().paused,
                has_metadata: true,
                seeding: stats.finished,
            },
        ))
    }

    pub fn files(&self) -> Result<Vec<TorrentFile>, String> {
        let handle = self.handle()?.ok_or("Torrent metadata is still loading")?;
        handle
            .with_metadata(|metadata| {
                metadata
                    .file_infos
                    .iter()
                    .map(|file| TorrentFile {
                        size: i64::try_from(file.len).unwrap_or(i64::MAX),
                        path: file.relative_filename.to_string_lossy().into_owned(),
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    pub fn file_priorities(&self) -> Result<Vec<u8>, String> {
        if let Some(priorities) = &self.saved.borrow().priorities {
            return Ok(priorities.clone());
        }
        Ok(vec![4; self.files()?.len()])
    }

    pub fn select_files(&self, priorities: &[u8]) -> Result<(), String> {
        let handle = self.handle()?.ok_or("Torrent metadata is still loading")?;
        let resume = self.saved.borrow().selecting && !self.saved.borrow().paused;
        self.session
            .runtime
            .block_on(async {
                self.session
                    .backend
                    .update_file_priorities(&handle, priorities)
                    .await?;
                if resume {
                    self.session.backend.unpause(&handle).await?;
                }
                Ok::<_, Box<dyn std::error::Error>>(())
            })
            .map_err(|error| error.to_string())?;
        let mut saved = self.saved.borrow_mut();
        saved.selecting = false;
        saved.priorities = Some(priorities.to_vec());
        Ok(())
    }

    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        let handle = self.handle().ok().flatten();
        let mut saved = self.saved.borrow_mut();
        if let Some(handle) = handle {
            if matches!(saved.source, Source::Magnet(_)) {
                if let Ok(bytes) = handle.with_metadata(|metadata| metadata.torrent_bytes.to_vec())
                {
                    saved.source = Source::Metadata(bytes);
                }
            }
        }
        serde_json::to_vec(&*saved).map_err(|error| error.to_string())
    }

    pub fn move_queue(&self, _up: bool) -> Result<(), String> {
        // This backend runs the whole queue concurrently. Transfers persists the displayed order.
        Ok(())
    }

    pub fn remove(&self) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        let handle = match &mut *state {
            State::Pending(task) => {
                task.abort();
                self.session
                    .runtime
                    .block_on(task)
                    .ok()
                    .and_then(Result::ok)
            }
            State::Initializing(handle) | State::Ready(handle) => Some(handle.clone()),
            State::NotStarted | State::Failed(_) => None,
        };
        if let Some(handle) = handle {
            self.session
                .runtime
                .block_on(
                    self.session
                        .backend
                        .delete(TorrentIdOrHash::Id(handle.id()), false),
                )
                .map_err(|error| error.to_string())?;
        }
        *state = State::Failed("Torrent was removed".to_owned());
        Ok(())
    }
}

impl Drop for Torrent<'_> {
    fn drop(&mut self) {
        if let State::Pending(task) = self.state.get_mut() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue_policy::PREVIEW_POLICY;
    use std::time::{Duration, Instant};

    fn wait_for_files(transfer: &Torrent<'_>) -> Result<Vec<TorrentFile>, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if transfer.status()?.1.has_metadata {
                return transfer.files();
            }
            if Instant::now() >= deadline {
                return Err("Metadata did not become ready".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn add_select_and_restore_a_local_torrent() -> Result<(), String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let source = directory.path().join("fixture.torrent");
        let mut metadata =
            b"d4:infod6:lengthi4e4:name7:fixture12:piece lengthi16384e6:pieces20:".to_vec();
        metadata.extend_from_slice(&[0; 20]);
        metadata.extend_from_slice(b"ee");
        std::fs::write(&source, metadata).map_err(|error| error.to_string())?;
        let destination = directory.path().join("download");
        let (key, saved) = {
            let session = Session::open(PREVIEW_POLICY, true)?;
            let transfer = session.add(
                source.to_str().unwrap(),
                destination.to_str().unwrap(),
                true,
            )?;
            let files = wait_for_files(&transfer)?;
            assert_eq!(
                files,
                [TorrentFile {
                    size: 4,
                    path: "fixture".into()
                }]
            );
            transfer.select_files(&[7])?;
            transfer.set_paused(true, false)?;
            assert!(transfer.status()?.1.paused);
            (transfer.key()?, transfer.checkpoint()?)
        };
        std::fs::remove_file(source).map_err(|error| error.to_string())?;
        let session = Session::open(PREVIEW_POLICY, true)?;
        let transfer = session.restore(&saved)?;
        wait_for_files(&transfer)?;
        assert_eq!(transfer.key()?, key);
        assert_eq!(transfer.file_priorities()?, [7]);
        assert!(transfer.status()?.1.paused);
        transfer.remove()?;
        assert!(session.backend.with_torrents(|torrents| torrents.count()) == 0);
        Ok(())
    }
}
