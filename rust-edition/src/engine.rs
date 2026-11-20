//! Rust ownership and operations for the independent libtorrent bridge.
use crate::queue_policy::QueuePolicy;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr::NonNull;

#[repr(C)]
#[derive(Debug, Default)]
pub struct TransferStatus {
    pub wanted: i64,
    pub wanted_done: i64,
    pub progress_ppm: i32,
    pub download_rate: i32,
    pub upload_rate: i32,
    pub peers: i32,
    pub paused: bool,
    pub has_metadata: bool,
    pub seeding: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TorrentFile {
    pub size: i64,
    pub path: String,
}

#[link(name = "torrent_re_libtorrent")]
extern "C" {
    fn re_session_open(
        downloads: i32,
        seeds: i32,
        active: i32,
        connections: i32,
        offline: bool,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn re_session_close(session: *mut c_void);
    fn re_add(
        session: *mut c_void,
        source: *const c_char,
        destination: *const c_char,
        magnet: bool,
        select: bool,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn re_handle_close(handle: *mut c_void);
    fn re_remove(
        session: *mut c_void,
        handle: *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;
    fn re_pause(
        handle: *mut c_void,
        pause: bool,
        auto_managed: bool,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;
    fn re_paused(
        handle: *mut c_void,
        result: *mut bool,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;
    fn re_read_policy(
        session: *mut c_void,
        values: *mut i32,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;
    fn re_file_priorities(
        handle: *mut c_void,
        values: *mut u8,
        length: usize,
        count: *mut usize,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;
    fn re_select_files(
        handle: *mut c_void,
        values: *const u8,
        length: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;

    fn re_restore(
        session: *mut c_void,
        bytes: *const u8,
        length: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn re_checkpoint(
        session: *mut c_void,
        handle: *mut c_void,
        receive: unsafe extern "C" fn(*mut c_void, *const u8, usize),
        context: *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;

    fn re_status(
        handle: *mut c_void,
        status: *mut TransferStatus,
        name: unsafe extern "C" fn(*mut c_void, *const u8, usize),
        context: *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;
    fn re_files(
        handle: *mut c_void,
        receive: unsafe extern "C" fn(*mut c_void, i64, *const u8, usize),
        context: *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;

    #[cfg(test)]
    fn re_listen_port(session: *mut c_void) -> u16;
    #[cfg(test)]
    fn re_connect_local_peer(
        handle: *mut c_void,
        port: u16,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;

    fn re_info_hash(
        handle: *mut c_void,
        output: *mut c_char,
        error: *mut c_char,
        capacity: usize,
    ) -> bool;

    fn re_move_queue(handle: *mut c_void, up: bool, error: *mut c_char, capacity: usize) -> bool;

}

fn checked<T>(operation: impl FnOnce(*mut c_char, usize) -> Option<T>) -> Result<T, String> {
    let mut error = [0 as c_char; 1024];
    operation(error.as_mut_ptr(), error.len()).ok_or_else(|| {
        // The buffer is zero-initialized, and the bridge writes a terminated error string.
        unsafe { CStr::from_ptr(error.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

pub struct Session {
    native: NonNull<c_void>,
    checkpoint_failed: std::cell::Cell<bool>,
}

impl Session {
    pub fn open(policy: QueuePolicy, offline: bool) -> Result<Self, String> {
        checked(|error, capacity| unsafe {
            NonNull::new(re_session_open(
                policy.active_downloads,
                policy.active_seeds,
                policy.active_limit,
                policy.connections_limit,
                offline,
                error,
                capacity,
            ))
            .map(|native| Self {
                native,
                checkpoint_failed: std::cell::Cell::new(false),
            })
        })
    }

    pub fn settings(&self) -> Result<QueuePolicy, String> {
        let mut values = [0; 4];
        checked(|error, capacity| unsafe {
            re_read_policy(self.native.as_ptr(), values.as_mut_ptr(), error, capacity).then_some(())
        })?;
        let [active_downloads, active_seeds, active_limit, connections_limit] = values;
        Ok(QueuePolicy {
            active_downloads,
            active_seeds,
            active_limit,
            connections_limit,
        })
    }

    pub fn add(
        &self,
        source: &str,
        destination: &str,
        select: bool,
    ) -> Result<Torrent<'_>, String> {
        let magnet = source.starts_with("magnet:");
        let source = CString::new(source).map_err(|e| e.to_string())?;
        let destination = CString::new(destination).map_err(|e| e.to_string())?;
        checked(|error, capacity| unsafe {
            NonNull::new(re_add(
                self.native.as_ptr(),
                source.as_ptr(),
                destination.as_ptr(),
                magnet,
                select,
                error,
                capacity,
            ))
            .map(|handle| Torrent {
                session: self,
                handle,
            })
        })
    }
    pub fn restore(&self, bytes: &[u8]) -> Result<Torrent<'_>, String> {
        if bytes.is_empty() {
            return Err("Empty resume data".into());
        }
        checked(|error, capacity| unsafe {
            NonNull::new(re_restore(
                self.native.as_ptr(),
                bytes.as_ptr(),
                bytes.len(),
                error,
                capacity,
            ))
            .map(|handle| Torrent {
                session: self,
                handle,
            })
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { re_session_close(self.native.as_ptr()) }
    }
}

pub struct Torrent<'session> {
    session: &'session Session,
    handle: NonNull<c_void>,
}

impl Torrent<'_> {
    pub fn set_paused(&self, paused: bool, auto_managed: bool) -> Result<(), String> {
        checked(|error, capacity| unsafe {
            re_pause(self.handle.as_ptr(), paused, auto_managed, error, capacity).then_some(())
        })
    }

    pub fn is_paused(&self) -> Result<bool, String> {
        let mut paused = false;
        checked(|error, capacity| unsafe {
            re_paused(self.handle.as_ptr(), &mut paused, error, capacity).then_some(())
        })?;
        Ok(paused)
    }

    pub fn file_priorities(&self) -> Result<Vec<u8>, String> {
        let mut count = 0;
        checked(|error, capacity| unsafe {
            re_file_priorities(
                self.handle.as_ptr(),
                std::ptr::null_mut(),
                0,
                &mut count,
                error,
                capacity,
            )
            .then_some(())
        })?;
        let mut values = vec![0; count];
        if !values.is_empty() {
            checked(|error, capacity| unsafe {
                re_file_priorities(
                    self.handle.as_ptr(),
                    values.as_mut_ptr(),
                    values.len(),
                    &mut count,
                    error,
                    capacity,
                )
                .then_some(())
            })?;
        }
        Ok(values)
    }

    pub fn select_files(&self, priorities: &[u8]) -> Result<(), String> {
        checked(|error, capacity| unsafe {
            re_select_files(
                self.handle.as_ptr(),
                priorities.as_ptr(),
                priorities.len(),
                error,
                capacity,
            )
            .then_some(())
        })
    }

    pub fn key(&self) -> Result<String, String> {
        let mut bytes = [0 as c_char; 65];
        checked(|error, capacity| unsafe {
            re_info_hash(self.handle.as_ptr(), bytes.as_mut_ptr(), error, capacity).then_some(())
        })?;
        Ok(unsafe { CStr::from_ptr(bytes.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    pub fn status(&self) -> Result<(String, TransferStatus), String> {
        unsafe extern "C" fn name(output: *mut c_void, bytes: *const u8, length: usize) {
            let value = unsafe { std::slice::from_raw_parts(bytes, length) };
            unsafe { *output.cast::<String>() = String::from_utf8_lossy(value).into_owned() };
        }
        let mut label = String::new();
        let mut status = TransferStatus::default();
        checked(|error, capacity| unsafe {
            re_status(
                self.handle.as_ptr(),
                &mut status,
                name,
                (&mut label as *mut String).cast(),
                error,
                capacity,
            )
            .then_some(())
        })?;
        Ok((label, status))
    }

    /// The returned order is libtorrent's file index order.
    pub fn files(&self) -> Result<Vec<TorrentFile>, String> {
        unsafe extern "C" fn entry(
            output: *mut c_void,
            size: i64,
            bytes: *const u8,
            length: usize,
        ) {
            let value = unsafe { std::slice::from_raw_parts(bytes, length) };
            let files = unsafe { &mut *output.cast::<Vec<TorrentFile>>() };
            files.push(TorrentFile {
                size,
                path: String::from_utf8_lossy(value).into_owned(),
            });
        }
        let mut files = Vec::<TorrentFile>::new();
        checked(|error, capacity| unsafe {
            re_files(
                self.handle.as_ptr(),
                entry,
                (&mut files as *mut Vec<TorrentFile>).cast(),
                error,
                capacity,
            )
            .then_some(())
        })?;
        Ok(files)
    }

    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        if self.session.checkpoint_failed.get() {
            return Err("Reopen the session before requesting another checkpoint".into());
        }
        unsafe extern "C" fn receive(context: *mut c_void, bytes: *const u8, length: usize) {
            let result = unsafe { &mut *context.cast::<Vec<u8>>() };
            if length != 0 {
                result.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, length) });
            }
        }
        let mut data = Vec::<u8>::new();
        let result = checked(|error, capacity| unsafe {
            re_checkpoint(
                self.session.native.as_ptr(),
                self.handle.as_ptr(),
                receive,
                (&mut data as *mut Vec<u8>).cast(),
                error,
                capacity,
            )
            .then_some(())
        });
        // A timed-out request can still post its alert. Preserve the previous checkpoint
        // and require a fresh session before accepting another result.
        if result.is_err() {
            self.session.checkpoint_failed.set(true);
        }
        result.map(|()| data)
    }

    pub fn move_queue(&self, up: bool) -> Result<(), String> {
        checked(|error, capacity| unsafe {
            re_move_queue(self.handle.as_ptr(), up, error, capacity).then_some(())
        })
    }

    pub fn remove(&self) -> Result<(), String> {
        checked(|error, capacity| unsafe {
            re_remove(
                self.session.native.as_ptr(),
                self.handle.as_ptr(),
                error,
                capacity,
            )
            .then_some(())
        })
    }
}

impl Drop for Torrent<'_> {
    fn drop(&mut self) {
        unsafe { re_handle_close(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue_policy::PREVIEW_POLICY;

    #[test]
    fn real_session_uses_new_limit_and_processes_pause_resume() -> Result<(), String> {
        let session = Session::open(PREVIEW_POLICY, true)?;
        assert_eq!(session.settings()?, PREVIEW_POLICY);
        assert!(session.add("magnet:", ".", true).is_err());
        let torrent = session.add(
            "magnet:?xt=urn:btih:0123456789012345678901234567890123456789",
            ".",
            true,
        )?;
        assert!(!torrent.is_paused()?);
        torrent.set_paused(true, false)?;
        assert!(torrent.is_paused()?);
        torrent.set_paused(false, false)?;
        assert!(!torrent.is_paused()?);
        torrent.remove()
    }

    #[test]
    fn checkpoint_preserves_file_priorities_across_session_restart(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("torrent-re-engine-{}", std::process::id()));
        std::fs::create_dir(&dir)?;
        let result = (|| {
            let source = dir.join("test.torrent");
            let mut metadata = b"d4:infod5:filesld6:lengthi1e4:pathl1:aeed6:lengthi1e4:pathl1:beee4:name6:sample12:piece lengthi16384e6:pieces20:".to_vec();
            metadata.extend([42; 20]);
            metadata.extend(b"ee");
            std::fs::write(&source, metadata)?;
            let session = Session::open(PREVIEW_POLICY, true)?;
            let torrent = session.add(source.to_str().unwrap(), dir.to_str().unwrap(), true)?;
            let (title, status) = torrent.status()?;
            assert_eq!(title, "sample");
            assert!(status.has_metadata);
            let files = torrent.files()?;
            assert_eq!(files.len(), 2);
            assert!(files[0].path.ends_with('a') && files[1].path.ends_with('b'));
            assert_eq!((files[0].size, files[1].size), (1, 1));
            assert!(torrent.select_files(&[4]).is_err());
            assert!(torrent.select_files(&[9, 0]).is_err());
            torrent.select_files(&[0, 7])?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let actual = torrent.file_priorities()?;
                if actual == [0, 7] {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "priorities stayed at {actual:?}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let checkpoint = torrent.checkpoint()?;
            let key = torrent.key()?;
            crate::profile::ResumeEntry {
                key: key.clone(),
                stopped: true,
                selection_pending: false,
                data: checkpoint,
            }
            .save(&dir)?;
            crate::profile::save_queue_order(&dir, &[&key])?;
            drop(torrent);
            drop(session);

            let directory = crate::profile::load_resume_entries(&dir)?;
            let reopened = Session::open(PREVIEW_POLICY, true)?;
            assert_eq!(directory[0].key, key);
            assert!(directory[0].stopped);
            let restored = reopened.restore(&directory[0].data)?;
            assert_eq!(restored.key()?, key);
            assert_eq!(restored.file_priorities()?, [0, 7]);
            restored.remove()?;
            Ok(())
        })();
        std::fs::remove_dir_all(dir)?;
        result
    }
    #[test]
    #[ignore = "requires libtorrent to open system network sockets"]
    fn three_torrents_download_over_loopback() -> Result<(), Box<dyn std::error::Error>> {
        use std::time::{Duration, Instant};
        let root = std::env::temp_dir().join(format!("torrent-re-loopback-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        let result = (|| {
            let sources = root.join("source");
            let output = root.join("output");
            std::fs::create_dir(&sources)?;
            std::fs::create_dir(&output)?;
            let seed = Session::open(
                QueuePolicy {
                    active_seeds: 3,
                    ..PREVIEW_POLICY
                },
                true,
            )?;
            let download = Session::open(PREVIEW_POLICY, true)?;
            let mut seeds = Vec::new();
            let mut downloads = Vec::new();
            let fixtures: [(&str, &[u8; 4], [u8; 20]); 3] = [
                (
                    "a",
                    b"aaaa",
                    [
                        112, 200, 129, 212, 162, 105, 132, 221, 206, 121, 95, 111, 113, 129, 124,
                        156, 244, 72, 14, 121,
                    ],
                ),
                (
                    "b",
                    b"bbbb",
                    [
                        138, 237, 19, 34, 229, 69, 11, 173, 176, 120, 225, 251, 96, 168, 23, 161,
                        223, 37, 162, 202,
                    ],
                ),
                (
                    "c",
                    b"cccc",
                    [
                        75, 234, 173, 98, 146, 183, 219, 15, 147, 84, 224, 216, 185, 21, 236, 13,
                        187, 192, 58, 90,
                    ],
                ),
            ];
            for (name, data, digest) in fixtures {
                std::fs::write(sources.join(name), data)?;
                let mut metadata =
                    format!("d4:infod6:lengthi4e4:name1:{name}12:piece lengthi16384e6:pieces20:")
                        .into_bytes();
                metadata.extend(digest);
                metadata.extend(b"ee");
                let path = root.join(format!("{name}.torrent"));
                std::fs::write(&path, metadata)?;
                seeds.push(seed.add(path.to_str().unwrap(), sources.to_str().unwrap(), false)?);
                downloads.push(download.add(
                    path.to_str().unwrap(),
                    output.to_str().unwrap(),
                    false,
                )?);
            }
            let deadline = Instant::now() + Duration::from_secs(25);
            let port = loop {
                let port = unsafe { re_listen_port(seed.native.as_ptr()) };
                let active = seeds.iter().all(|torrent| {
                    torrent
                        .status()
                        .is_ok_and(|(_, state)| state.seeding && !state.paused)
                }) && downloads
                    .iter()
                    .all(|torrent| torrent.status().is_ok_and(|(_, state)| !state.paused));
                if port != 0 && active {
                    break port;
                }
                assert!(
                    Instant::now() < deadline,
                    "three torrents never became active together; port {port}, seeds {:?}, downloads {:?}", seeds.iter().map(Torrent::status).collect::<Vec<_>>(), downloads.iter().map(Torrent::status).collect::<Vec<_>>()
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            for torrent in &downloads {
                checked(|error, capacity| unsafe {
                    re_connect_local_peer(torrent.handle.as_ptr(), port, error, capacity)
                        .then_some(())
                })?;
            }
            let deadline = Instant::now() + Duration::from_secs(25);
            while !downloads
                .iter()
                .all(|tor| tor.status().is_ok_and(|(_, state)| state.seeding))
            {
                assert!(
                    Instant::now() < deadline,
                    "loopback transfers did not all finish"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
            // A torrent can enter seeding before the disk thread drains its write queue.
            drop(downloads);
            drop(download);
            drop(seeds);
            drop(seed);
            for (name, data, _) in fixtures {
                assert_eq!(std::fs::read(output.join(name))?, data);
            }
            Ok(())
        })();
        std::fs::remove_dir_all(root)?;
        result
    }
}
