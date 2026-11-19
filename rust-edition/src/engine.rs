//! Rust ownership and operations for the independent libtorrent bridge.
use crate::queue_policy::QueuePolicy;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr::NonNull;

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

pub struct Session(NonNull<c_void>);

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
            .map(Self)
        })
    }

    pub fn settings(&self) -> Result<QueuePolicy, String> {
        let mut values = [0; 4];
        checked(|error, capacity| unsafe {
            re_read_policy(self.0.as_ptr(), values.as_mut_ptr(), error, capacity).then_some(())
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
                self.0.as_ptr(),
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
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { re_session_close(self.0.as_ptr()) }
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

    pub fn remove(self) -> Result<(), String> {
        checked(|error, capacity| unsafe {
            re_remove(
                self.session.0.as_ptr(),
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
    fn torrent_file_keeps_original_priority_indices() -> Result<(), Box<dyn std::error::Error>> {
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
            torrent.remove()?;
            Ok(())
        })();
        std::fs::remove_dir_all(dir)?;
        result
    }
}
