//! Profile file formats used by the Windows client.
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preferences {
    pub folder: PathBuf,
    pub select_files: bool,
    pub theme: Theme,
    pub english: bool,
    pub close_to_tray: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light = 0,
    Dark = 1,
    System = 2,
}

impl Preferences {
    pub fn load(root: &Path, default_folder: PathBuf) -> io::Result<Self> {
        let mut defaults = Self {
            folder: default_folder,
            select_files: false,
            theme: Theme::Light,
            english: false,
            close_to_tray: true,
        };
        match fs::read(root.join("settings.txt")) {
            Ok(bytes) if !bytes.is_empty() => {
                let text = String::from_utf8_lossy(&bytes);
                let (folder, options) = text.split_once('\n').unwrap_or((&text, ""));
                // The original opens this file in binary mode.
                defaults.folder = PathBuf::from(folder);
                let mut options = options
                    .split_ascii_whitespace()
                    .map_while(|v| v.parse::<i32>().ok());
                defaults.select_files = options.next().unwrap_or_default() > 0;
                defaults.theme = match options.next().unwrap_or_default().clamp(0, 2) {
                    1 => Theme::Dark,
                    2 => Theme::System,
                    _ => Theme::Light,
                };
                defaults.english = options.next().unwrap_or_default() > 0;
                defaults.close_to_tray = options.next().unwrap_or(1) > 0;
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        Ok(defaults)
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let folder = self
            .folder
            .to_str()
            .filter(|s| !s.contains('\n'))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "folder cannot be stored in settings.txt",
                )
            })?;
        let text = format!(
            "{folder}\n{} {} {} {}",
            u8::from(self.select_files),
            self.theme as u8,
            u8::from(self.english),
            u8::from(self.close_to_tray),
        );
        atomic_write(
            &root.join("settings.txt"),
            &root.join("settings.tmp"),
            text.as_bytes(),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
    pub work: Rect,
}

impl WindowPosition {
    pub fn decode(text: &str) -> Option<Self> {
        let mut values = text.split_ascii_whitespace();
        if values.next()?.parse::<i32>().ok()? != 1 {
            return None;
        }
        let mut coordinate = || {
            values
                .next()?
                .parse::<i32>()
                .ok()
                .filter(|x| (-1_000_000..=1_000_000).contains(x))
        };
        let x = coordinate()?;
        let y = coordinate()?;
        let work = Rect {
            left: coordinate()?,
            top: coordinate()?,
            right: coordinate()?,
            bottom: coordinate()?,
        };
        (work.right > work.left && work.bottom > work.top).then_some(Self { x, y, work })
    }

    pub fn restore(self, width: i32, height: i32, work: Rect) -> (i32, i32) {
        let left = self.x.max(work.left);
        let top = self.y.max(work.top);
        let right = self.x.saturating_add(width).min(work.right);
        let bottom = self.y.saturating_add(height.min(62)).min(work.bottom);
        if self.work == work
            && right.saturating_sub(left) >= width.min(100)
            && bottom.saturating_sub(top) >= 24
        {
            return (self.x, self.y);
        }
        (
            self.x
                .clamp(work.left, work.left.max(work.right.saturating_sub(width))),
            self.y
                .clamp(work.top, work.top.max(work.bottom.saturating_sub(height))),
        )
    }

    pub fn save(self, root: &Path) -> io::Result<()> {
        let Rect {
            left,
            top,
            right,
            bottom,
        } = self.work;
        let text = format!("1 {} {} {left} {top} {right} {bottom}\r\n", self.x, self.y);
        atomic_write(
            &root.join("window-position.txt"),
            &root.join("window-position.txt.tmp"),
            text.as_bytes(),
        )
    }
}

// Torrent's single-instance owner serializes all writes to a profile.
fn atomic_write(target: &Path, temporary: &Path, bytes: &[u8]) -> io::Result<()> {
    {
        let mut file = fs::File::create(temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(temporary, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_profile_round_trip() -> io::Result<()> {
        let dir = std::env::temp_dir().join(format!("torrent-re-profile-{}", std::process::id()));
        fs::create_dir(&dir)?;
        let result = (|| {
            let fallback = PathBuf::from("Downloads");
            let prefs = Preferences::load(&dir, fallback.clone())?;
            assert_eq!(prefs.folder, fallback);
            let sample = "D:\\Торренты\\Фильмы\n1 2 1";
            fs::write(dir.join("settings.txt"), sample)?;
            let prefs = Preferences::load(&dir, fallback.clone())?;
            assert_eq!(prefs.folder, PathBuf::from("D:\\Торренты\\Фильмы"));
            assert_eq!(prefs.theme, Theme::System);
            assert!(prefs.english && prefs.select_files && prefs.close_to_tray);
            prefs.save(&dir)?;
            assert_eq!(
                fs::read_to_string(dir.join("settings.txt"))?,
                format!("{sample} 1")
            );
            fs::write(dir.join("settings.txt"), format!("{sample} 0 0"))?;
            assert!(!Preferences::load(&dir, fallback.clone())?.close_to_tray);

            fs::write(dir.join("settings.txt"), "\n99 -1 x")?;
            let prefs = Preferences::load(&dir, fallback)?;
            assert!(prefs.select_files);
            assert_eq!(prefs.theme, Theme::Light);
            assert!(!prefs.english);
            assert!(prefs.folder.as_os_str().is_empty());
            let position = WindowPosition::decode("1 -1200 31 -1920 0 0 1040").unwrap();
            position.save(&dir)?;
            assert_eq!(
                WindowPosition::decode(&fs::read_to_string(dir.join("window-position.txt"))?),
                Some(position)
            );
            Ok(())
        })();
        fs::remove_dir_all(dir)?;
        result
    }

    #[test]
    fn preserves_reachable_title_and_clamps_after_monitor_change() {
        let position = WindowPosition::decode("1 -40 20 0 0 1920 1040").unwrap();
        assert_eq!(position.restore(940, 720, position.work), (-40, 20));
        let new_work = Rect {
            right: 1280,
            ..position.work
        };
        assert_eq!(position.restore(940, 720, new_work), (0, 20));
        assert!(WindowPosition::decode("1 -1000001 0 0 0 1920 1040").is_none());
        assert!(WindowPosition::decode("2 0 0 0 0 1920 1040").is_none());
        assert!(WindowPosition::decode("1 0 0 10 0 0 1040").is_none());
    }
}

/// A legacy session entry. libtorrent owns the serialization of resume data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeEntry {
    pub key: String,
    pub stopped: bool,
    pub selection_pending: bool,
    pub data: Vec<u8>,
}

impl ResumeEntry {
    /// Writes the legacy payload and its sidecar. The caller owns the profile.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        if !valid_key(&self.key) || self.data.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid resume entry",
            ));
        }
        fs::create_dir_all(root)?;
        let state = format!(
            "{} {}",
            u8::from(self.stopped),
            u8::from(self.selection_pending)
        );
        atomic_write(
            &root.join(format!("{}.state", self.key)),
            &root.join(format!("{}.state.tmp", self.key)),
            state.as_bytes(),
        )?;
        atomic_write(
            &root.join(format!("{}.resume", self.key)),
            &root.join(format!("{}.resume.tmp", self.key)),
            &self.data,
        )
    }
}

pub fn remove_resume_entry(root: &Path, key: &str) -> io::Result<()> {
    if !valid_key(key) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid resume key",
        ));
    }
    // Delete the resume payload first: discovery only considers entries with one.
    for extension in ["resume", "state"] {
        match fs::remove_file(root.join(format!("{key}.{extension}"))) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            result => result?,
        }
    }
    Ok(())
}

pub fn save_queue_order(root: &Path, keys: &[&str]) -> io::Result<()> {
    let mut order = String::new();
    let mut seen = std::collections::HashSet::new();
    for key in keys {
        if !valid_key(key) || !seen.insert(key) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid queue entry",
            ));
        }
        order.push_str(key);
        order.push('\n');
    }
    fs::create_dir_all(root)?;
    atomic_write(
        &root.join("queue.txt"),
        &root.join("queue.txt.tmp"),
        order.as_bytes(),
    )
}

fn valid_key(key: &str) -> bool {
    matches!(key.len(), 40 | 64) && key.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Reads the legacy queue first, then discovers resume files missing from it.
pub fn load_resume_entries(root: &Path) -> io::Result<Vec<ResumeEntry>> {
    let queue = match fs::read_to_string(root.join("queue.txt")) {
        Ok(queue) => queue,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut keys = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut append = |key: &str| {
        if valid_key(key) && seen.insert(key.to_owned()) {
            keys.push(key.to_owned());
        }
    };
    for key in queue.lines() {
        append(key);
    }
    let directory = match fs::read_dir(root) {
        Ok(directory) => directory,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    for entry in directory {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "resume") {
            if let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) {
                append(key);
            }
        }
    }
    let mut entries = Vec::new();
    for key in keys {
        let data = match fs::read(root.join(format!("{key}.resume"))) {
            Ok(data) => data,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let state = match fs::read_to_string(root.join(format!("{key}.state"))) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        let mut flags = state.split_ascii_whitespace();
        let stopped = flags.next() == Some("1");
        let selection_pending = flags.next() == Some("1");
        entries.push(ResumeEntry {
            key,
            stopped,
            selection_pending,
            data,
        });
    }
    Ok(entries)
}

#[cfg(test)]
#[test]
fn legacy_session_keeps_queue_order_and_opaque_resume_data() -> io::Result<()> {
    let dir = std::env::temp_dir().join(format!("torrent-re-session-{}", std::process::id()));
    fs::create_dir(&dir)?;
    let result = (|| {
        let v1 = "a".repeat(40);
        let v2 = "b".repeat(64);
        let orphan = "c".repeat(40);
        fs::write(
            dir.join("queue.txt"),
            format!("{v2}\r\n../unsafe\r\n{v1}\r\n{v2}\r\n"),
        )?;
        for key in [&v1, &v2, &orphan] {
            fs::write(dir.join(format!("{key}.resume")), [0, 255, 7])?;
        }
        fs::write(dir.join(format!("{v2}.state")), "1 1")?;
        let entries = load_resume_entries(&dir)?;
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[0],
            ResumeEntry {
                key: v2,
                stopped: true,
                selection_pending: true,
                data: vec![0, 255, 7],
            }
        );
        assert_eq!(entries[1].key, v1);
        assert_eq!(entries[2].key, orphan);
        Ok(())
    })();
    fs::remove_dir_all(dir)?;
    result
}
