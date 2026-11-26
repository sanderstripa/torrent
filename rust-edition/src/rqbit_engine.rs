use crate::queue_policy::QueuePolicy;
use librqbit::{
    api::TorrentIdOrHash, AddTorrent, AddTorrentOptions, AddTorrentResponse, Magnet,
    ManagedTorrent, SessionOptions,
};
use serde::{Deserialize, Serialize};
use std::cell::{Ref, RefCell};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Network {
    Direct,
    Blocked,
    Proxy(u16),
}

struct RunningSession {
    backend: Arc<librqbit::Session>,
    runtime: Runtime,
}

impl RunningSession {
    fn open(policy: QueuePolicy, network: Network) -> Result<Self, String> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .map_err(|error| error.to_string())?;
        let mut options = SessionOptions {
            peer_limit: usize::try_from(policy.connections_limit).ok(),
            ..SessionOptions::default()
        };
        match network {
            Network::Direct => {
                options.listen = Some(librqbit::ListenerOptions {
                    mode: librqbit::ListenerMode::TcpAndUtp,
                    ..Default::default()
                });
                options.dht = Some(librqbit::DhtSessionConfig {
                    persistence: None,
                    ..Default::default()
                });
            }
            Network::Blocked => {
                options.dht = None;
                options.listen = None;
                options.disable_trackers = true;
                options.disable_local_service_discovery = true;
                options.disable_web_seeds = true;
                options.connect = Some(librqbit::ConnectionOptions {
                    proxy_url: Some("socks5h://127.0.0.1:0".to_owned()),
                    ..Default::default()
                });
            }
            Network::Proxy(port) => {
                options.connect = Some(librqbit::ConnectionOptions {
                    proxy_url: Some(format!("socks5h://127.0.0.1:{port}")),
                    ..Default::default()
                });
            }
        }
        let backend = runtime
            .block_on(librqbit::Session::new_with_opts(PathBuf::new(), options))
            .map_err(|error| error.to_string())?;
        Ok(Self { backend, runtime })
    }
}

impl Drop for RunningSession {
    fn drop(&mut self) {
        self.runtime.block_on(self.backend.stop());
        // Dropping the runtime waits for its workers to end before another transport starts.
    }
}

pub struct Session {
    policy: QueuePolicy,
    current: RefCell<Option<RunningSession>>,
}

impl Session {
    pub fn open(policy: QueuePolicy, offline: bool) -> Result<Self, String> {
        let network = if offline {
            Network::Blocked
        } else {
            Network::Direct
        };
        Ok(Self {
            policy,
            current: RefCell::new(Some(RunningSession::open(policy, network)?)),
        })
    }

    fn running(&self) -> Result<Ref<'_, RunningSession>, String> {
        Ref::filter_map(self.current.borrow(), Option::as_ref)
            .map_err(|_| "Torrent network session is unavailable".to_owned())
    }

    pub(crate) fn change_network(&self, network: Network) -> Result<(), String> {
        drop(self.current.borrow_mut().take());
        self.current
            .replace(Some(RunningSession::open(self.policy, network)?));
        Ok(())
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

    fn add_pending(
        &self,
        pending: Checkpoint,
    ) -> Result<JoinHandle<Result<Arc<ManagedTorrent>, String>>, String> {
        let running = self.running()?;
        let session = running.backend.clone();
        Ok(running.runtime.spawn(async move {
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
        }))
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
        let running = self.session.running()?;
        let mut state = self.state.borrow_mut();
        if matches!(*state, State::NotStarted) {
            *state = State::Pending(self.session.add_pending(self.saved.borrow().clone())?);
        }
        if let State::Pending(task) = &mut *state {
            if !task.is_finished() {
                return Ok(None);
            }
            *state = match running.runtime.block_on(task) {
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
                running
                    .runtime
                    .block_on(async {
                        if let Some(priorities) = &saved.priorities {
                            running
                                .backend
                                .update_file_priorities(&handle, priorities)
                                .await?;
                        }
                        if !saved.selecting && !saved.paused {
                            running.backend.unpause(&handle).await?;
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
        let running = self.session.running()?;
        if let Some(handle) = self.handle()? {
            if handle.is_paused() != paused && !self.saved.borrow().selecting {
                running
                    .runtime
                    .block_on(async {
                        if paused {
                            running.backend.pause(&handle).await
                        } else {
                            running.backend.unpause(&handle).await
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
        let running = self.session.running()?;
        let handle = self.handle()?.ok_or("Torrent metadata is still loading")?;
        let resume = self.saved.borrow().selecting && !self.saved.borrow().paused;
        running
            .runtime
            .block_on(async {
                running
                    .backend
                    .update_file_priorities(&handle, priorities)
                    .await?;
                if resume {
                    running.backend.unpause(&handle).await?;
                }
                Ok::<_, Box<dyn std::error::Error>>(())
            })
            .map_err(|error| error.to_string())?;
        let mut saved = self.saved.borrow_mut();
        saved.selecting = false;
        saved.priorities = Some(priorities.to_vec());
        Ok(())
    }

    fn cache_metadata(&self, handle: &ManagedTorrent) {
        let mut saved = self.saved.borrow_mut();
        if matches!(saved.source, Source::Magnet(_)) {
            if let Ok(bytes) = handle.with_metadata(|metadata| metadata.torrent_bytes.to_vec()) {
                saved.source = Source::Metadata(bytes);
            }
        }
    }

    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        if let Some(handle) = self.handle().ok().flatten() {
            self.cache_metadata(&handle);
        }
        serde_json::to_vec(&*self.saved.borrow()).map_err(|error| error.to_string())
    }

    pub(crate) fn detach(&self) {
        match self.state.replace(State::NotStarted) {
            State::Pending(task) => task.abort(),
            State::Initializing(handle) | State::Ready(handle) => self.cache_metadata(&handle),
            State::NotStarted | State::Failed(_) => {}
        }
    }

    pub fn move_queue(&self, _up: bool) -> Result<(), String> {
        // This backend runs the whole queue concurrently. Transfers persists the displayed order.
        Ok(())
    }

    pub fn remove(&self) -> Result<(), String> {
        let running = self.session.running()?;
        let mut state = self.state.borrow_mut();
        let handle = match &mut *state {
            State::Pending(task) => {
                task.abort();
                running.runtime.block_on(task).ok().and_then(Result::ok)
            }
            State::Initializing(handle) | State::Ready(handle) => Some(handle.clone()),
            State::NotStarted | State::Failed(_) => None,
        };
        if let Some(handle) = handle {
            running
                .runtime
                .block_on(
                    running
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

    fn local_fixture(directory: &Path) -> Result<(PathBuf, PathBuf), String> {
        let source = directory.join("fixture.torrent");
        let mut metadata =
            b"d4:infod6:lengthi4e4:name7:fixture12:piece lengthi16384e6:pieces20:".to_vec();
        metadata.extend_from_slice(&[0; 20]);
        metadata.extend_from_slice(b"ee");
        std::fs::write(&source, metadata).map_err(|error| error.to_string())?;
        Ok((source, directory.join("download")))
    }

    #[test]
    fn switching_to_proxy_closes_an_existing_direct_peer() -> Result<(), String> {
        use crate::transfers::Transfers;
        use std::io::{Read, Write};
        use std::net::{Ipv6Addr, TcpListener, TcpStream};

        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let (source, destination) = local_fixture(directory.path())?;
        let session = Session::open(PREVIEW_POLICY, false)?;
        let (mut queue, warnings) = Transfers::open(&session, &directory.path().join("queue"))?;
        assert!(warnings.is_empty());
        queue.add(
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
            false,
        )?;
        let transfer = &queue.ordered()[0].handle;
        wait_for_files(transfer)?;
        let port = session
            .running()?
            .backend
            .listen_addr()
            .ok_or("Missing direct listener")?
            .port();
        let info_hash = transfer.handle()?.ok_or("Torrent is missing")?.info_hash();
        let mut peer =
            TcpStream::connect((Ipv6Addr::LOCALHOST, port)).map_err(|error| error.to_string())?;
        peer.set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|error| error.to_string())?;
        let mut handshake = [0u8; 68];
        handshake[0] = 19;
        handshake[1..20].copy_from_slice(b"BitTorrent protocol");
        handshake[28..48].copy_from_slice(&info_hash.0);
        handshake[48..].copy_from_slice(b"-XX0001-TESTFIXTURE1");
        peer.write_all(&handshake)
            .map_err(|error| error.to_string())?;
        peer.read_exact(&mut handshake)
            .map_err(|error| error.to_string())?;
        assert_eq!(&handshake[1..20], b"BitTorrent protocol");
        let proxy = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
        queue.change_network(Network::Proxy(
            proxy
                .local_addr()
                .map_err(|error| error.to_string())?
                .port(),
        ))?;
        assert!(session.running()?.backend.listen_addr().is_none());
        assert!(session.running()?.backend.get_dht().is_none());
        let mut trailing = Vec::new();
        if let Err(error) = peer.read_to_end(&mut trailing) {
            if error.kind() != std::io::ErrorKind::ConnectionReset {
                return Err(format!("Direct peer survived VPN transition: {error}"));
            }
        }
        wait_for_files(transfer)?;
        queue.change_network(Network::Blocked)?;
        assert!(session.running()?.backend.listen_addr().is_none());
        assert!(!transfer.status()?.1.paused);
        Ok(())
    }

    #[test]
    fn add_select_and_restore_a_local_torrent() -> Result<(), String> {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let (source, destination) = local_fixture(directory.path())?;
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
        assert!(
            session
                .running()?
                .backend
                .with_torrents(|torrents| torrents.count())
                == 0
        );
        Ok(())
    }
}
