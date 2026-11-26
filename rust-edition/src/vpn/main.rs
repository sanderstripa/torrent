#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;

use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    let [action, key_file, status_file, port] = arguments.as_slice() else {
        return Err("usage: TorrentVpnHost --run KEY_FILE STATUS_FILE PORT".into());
    };
    if action != "--run" {
        return Err("usage: TorrentVpnHost --run KEY_FILE STATUS_FILE PORT".into());
    }
    let port = port
        .to_str()
        .and_then(|raw| raw.parse::<u16>().ok())
        .filter(|&port| port >= 1024)
        .ok_or("invalid local SOCKS port")?;
    let key_file = Path::new(key_file);
    let status = Path::new(status_file);
    let result = supervise(key_file, status, port);
    if let Err(error) = &result {
        let _ = write_status(status, &format!("error:{error}"));
    }
    result
}

fn supervise(key_file: &Path, status: &Path, port: u16) -> Result<()> {
    let key = fs::read_to_string(key_file);
    fs::remove_file(key_file)?;
    let (core, configuration) = config::parse(&key?, port)?;
    let package = std::env::current_exe()?;
    let package = package.parent().ok_or("invalid executable location")?;
    let binary = resolve_engine(package, core)?;
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    if TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok() {
        return Err("the local Torrent VPN port is already in use".into());
    }

    let directory = status
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or("invalid VPN status location")?;
    let mut file = tempfile::Builder::new()
        .prefix("connection-")
        .suffix(".json")
        .tempfile_in(directory)?;
    serde_json::to_writer(file.as_file_mut(), &configuration)?;
    file.as_file_mut().flush()?;
    let file = file.into_temp_path();

    let log = File::create(directory.join("engine.log"))?;
    let mut command = Command::new(&binary);
    command.current_dir(
        binary
            .parent()
            .ok_or("invalid VPN core executable location")?,
    );
    match core {
        config::Core::Xray => {
            command.args(["run", "-config"]);
        }
        config::Core::Sudoku => {
            command.arg("-c");
        }
    }
    command
        .arg(file.as_os_str())
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);

    #[cfg(windows)]
    let job = windows_job::Job::create()?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = CoreChild(command.spawn()?);
    #[cfg(windows)]
    job.assign(&child.0)?;

    let (closed, parent) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = io::copy(&mut io::stdin().lock(), &mut io::sink());
        let _ = closed.send(());
    });

    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if child.0.try_wait()?.is_some() {
            return Err("VPN core stopped before its SOCKS listener became ready".into());
        }
        if socks_ready(address) && child.0.try_wait()?.is_none() {
            break;
        }
        if Instant::now() >= deadline {
            return Err("VPN SOCKS listener did not start".into());
        }
        if parent.recv_timeout(Duration::from_millis(100)).is_ok() {
            return Ok(());
        }
    }
    write_status(status, "ready")?;
    loop {
        if parent.recv_timeout(Duration::from_millis(100)).is_ok() {
            return Ok(());
        }
        if child.0.try_wait()?.is_some() {
            return Err("VPN core stopped".into());
        }
    }
}

fn resolve_engine(package: &Path, core: config::Core) -> Result<PathBuf> {
    let name = core.name();
    let manifest: serde_json::Value =
        serde_json::from_reader(File::open(package.join("components.json"))?)?;
    let expected = manifest[name]["sha256"]
        .as_str()
        .filter(|hash| hash.len() == 64)
        .ok_or("invalid VPN component manifest")?;
    let path = package.join(name).join(format!("{name}.exe"));
    let mut binary = File::open(&path)?;
    let mut hash = Sha256::new();
    io::copy(&mut binary, &mut hash)?;
    if hex::encode(hash.finalize()) != expected {
        return Err("VPN component checksum mismatch".into());
    }
    Ok(path)
}

fn socks_ready(address: SocketAddr) -> bool {
    let result = (|| -> io::Result<_> {
        let timeout = Duration::from_millis(250);
        let mut stream = TcpStream::connect_timeout(&address, timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        stream.write_all(&[5, 1, 0])?;
        let mut response = [0; 2];
        stream.read_exact(&mut response)?;
        Ok(response)
    })();
    matches!(result, Ok([5, 0]))
}

fn write_status(path: &Path, status: &str) -> io::Result<()> {
    let directory = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid VPN status location")
        })?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(status.as_bytes())?;
    temporary.persist(path)?;
    Ok(())
}

struct CoreChild(Child);

impl Drop for CoreChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(windows)]
mod windows_job {
    use std::{io, mem::size_of, os::windows::io::AsRawHandle, process::Child, ptr::null};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job(HANDLE);

    impl Job {
        pub fn create() -> io::Result<Self> {
            let handle = unsafe { CreateJobObjectW(null(), null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        pub fn assign(&self, child: &Child) -> io::Result<()> {
            if unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle().cast()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
}
