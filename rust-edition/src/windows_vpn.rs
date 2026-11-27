use std::fs;
use std::io::{self, Write};
use std::net::TcpListener;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};
use torrent_re_sort::{engine::Network, transfers::Transfers};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

pub struct Vpn {
    directory: PathBuf,
    worker: Option<Worker>,
}

struct Worker {
    process: Child,
    workspace: tempfile::TempDir,
    port: u16,
    connected: bool,
}

impl Drop for Worker {
    fn drop(&mut self) {
        drop(self.process.stdin.take());
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            if self.process.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

impl Vpn {
    pub fn new(profile: &Path) -> Self {
        Self {
            directory: profile.join("VPN"),
            worker: None,
        }
    }

    pub fn key(&self) -> Result<String, String> {
        match fs::read(self.directory.join("connection-key.dat")) {
            Ok(bytes) => {
                String::from_utf8(protect(bytes, false)?).map_err(|error| error.to_string())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save_key(&self, key: &str) -> Result<(), String> {
        let encrypted = protect(key.as_bytes().to_vec(), true)?;
        fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        let mut temp =
            tempfile::NamedTempFile::new_in(&self.directory).map_err(|error| error.to_string())?;
        temp.write_all(&encrypted)
            .map_err(|error| error.to_string())?;
        temp.as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temp.persist(self.directory.join("connection-key.dat"))
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn connect(&mut self, queue: &Transfers<'_>, key: &str) -> Result<(), String> {
        // Close every direct socket before starting the helper or resolving a new tracker.
        queue.change_network(Network::Blocked)?;
        self.worker = None;
        let helper = std::env::current_exe()
            .map_err(|error| error.to_string())?
            .parent()
            .ok_or("Invalid application path")?
            .join("vpn")
            .join("TorrentVpnHost.exe");
        if !helper.is_file() {
            return Err("VPN component is not installed".into());
        }
        if !key.starts_with("vless://") && !key.starts_with("sudoku://") {
            return Err("Enter a vless:// or sudoku:// connection key".into());
        }

        fs::create_dir_all(&self.directory).map_err(|error| error.to_string())?;
        let workspace = tempfile::Builder::new()
            .prefix("tunnel-")
            .tempdir_in(&self.directory)
            .map_err(|error| error.to_string())?;
        let key_path = workspace.path().join("key.tmp");
        let status = workspace.path().join("status.txt");
        fs::write(&key_path, key).map_err(|error| error.to_string())?;
        let port = TcpListener::bind("127.0.0.1:0")
            .and_then(|listener| listener.local_addr())
            .map_err(|error| error.to_string())?
            .port();
        let process = Command::new(helper)
            .args(["--run"])
            .arg(key_path)
            .arg(status)
            .arg(port.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| error.to_string())?;
        self.worker = Some(Worker {
            process,
            workspace,
            port,
            connected: false,
        });
        Ok(())
    }

    pub fn disconnect(&mut self, queue: &Transfers<'_>) -> Result<(), String> {
        queue.change_network(Network::Blocked)?;
        self.worker = None;
        queue.change_network(Network::Direct)
    }

    pub fn state(&self, enabled: bool) -> u8 {
        match &self.worker {
            Some(worker) if worker.connected => 2,
            Some(_) => 1,
            None if enabled => 3,
            None => 0,
        }
    }

    pub fn tick(&mut self, queue: &Transfers<'_>) -> Result<(), String> {
        let Some(worker) = &mut self.worker else {
            return Ok(());
        };
        let status = worker.workspace.path().join("status.txt");
        let state = fs::read_to_string(&status).ok();
        let closure = match worker.process.try_wait() {
            Ok(Some(_)) => Some("VPN stopped. Torrent traffic is blocked.".to_owned()),
            Ok(None) => None,
            Err(error) => Some(error.to_string()),
        };
        let failure = state
            .as_deref()
            .and_then(|value| value.strip_prefix("error:"))
            .map(str::to_owned)
            .or(closure);
        if let Some(error) = failure {
            let isolation = queue.change_network(Network::Blocked);
            self.worker = None;
            isolation?;
            return Err(error);
        }
        if !worker.connected && state.as_deref() == Some("ready") {
            if let Err(error) = queue.change_network(Network::Proxy(worker.port)) {
                self.worker = None;
                return Err(error);
            }
            worker.connected = true;
        }
        Ok(())
    }
}

fn protect(mut input: Vec<u8>, encrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(input.len()).map_err(|error| error.to_string())?,
        pbData: input.as_mut_ptr(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                windows_sys::w!("Torrent isolated VPN key"),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                null(),
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(io::Error::last_os_error().to_string());
    }
    let bytes = if output.cbData == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec()
    };
    unsafe { LocalFree(output.pbData.cast()) };
    Ok(bytes)
}
