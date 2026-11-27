use std::ffi::OsStr;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use torrent_re_sort::install_payload::install_new;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegGetValueW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_YESNO,
};

const NAME: &str = "Torrent Rust Edition";
const VERSION: &str = "1.4.4 RE";
const APP: &str = "Torrent-RE.exe";
const SETUP: &str = "Uninstall.exe";
const OWNER: &str = ".torrent-rust-edition";
const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\TorrentRustEdition";
const REGISTERED: &str = r"Software\RegisteredApplications";
const CAPABILITIES: &str = r"Software\TorrentRustEdition\Capabilities";
const FILE_CLASS: &str = r"Software\Classes\TorrentRustEdition.File";
const MAGNET_CLASS: &str = r"Software\Classes\TorrentRustEdition.Magnet";
const TORRENT_OPEN_WITH: &str = r"Software\Classes\.torrent\OpenWithProgids";
const REPARSE_POINT: u32 = 0x400;

include!(concat!(env!("OUT_DIR"), "/installer_payload.rs"));

pub fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [action, directory] if action == "--install" => install(Path::new(directory)),
        [action, directory] if action == "--uninstall" => launch_uninstaller(Path::new(directory)),
        [action, directory, parent] if action == "--cleanup" => {
            cleanup(Path::new(directory), parent)
        }
        [] if std::env::current_exe()?.file_name() == Some(OsStr::new(SETUP)) => {
            let executable = std::env::current_exe()?;
            let directory = executable.parent().ok_or_else(invalid_path)?;
            if confirm("Remove Torrent Rust Edition?") {
                launch_uninstaller(directory)?;
            }
            Ok(())
        }
        [] => {
            let local = std::env::var_os("LOCALAPPDATA").ok_or_else(invalid_path)?;
            let directory = registered_directory()?
                .unwrap_or_else(|| PathBuf::from(local).join("Programs").join(NAME));
            crate::installer_ui::show(directory)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid setup arguments",
        )),
    }
}

fn install(directory: &Path) -> io::Result<()> {
    install_with_options(directory, [true, true], |_| {})
}

pub(crate) fn install_with_options(
    directory: &Path,
    shortcuts: [bool; 2],
    mut report: impl FnMut(u8),
) -> io::Result<()> {
    validate_path(directory)?;
    if let Some(registered) = registered_directory()? {
        if registered.join(APP).is_file() && !same_path(directory, &registered) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Use the registered installation folder for an update",
            ));
        }
    }
    if !FILES.iter().any(|(name, _)| *name == APP) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Installation payload is missing",
        ));
    }
    let parent = directory.parent().ok_or_else(invalid_path)?;
    fs::create_dir_all(parent)?;
    validate_path(directory)?;

    let previous = directory.exists();
    if previous && fs::read_dir(directory)?.next().is_some() {
        validate_owned(directory)?;
    }
    report(15);
    let work = tempfile::Builder::new()
        .prefix("TorrentSetup-")
        .tempdir_in(parent)?;
    let staged = work.path().join("payload");
    let backup = work.path().join("backup");
    let exe = fs::read(std::env::current_exe()?)?;
    let mut payload: Vec<_> = FILES
        .iter()
        .map(|(name, bytes)| (Path::new(name), *bytes))
        .collect();
    payload.push((Path::new(SETUP), &exe));
    payload.push((Path::new(OWNER), NAME.as_bytes()));
    install_new(&staged, &payload)?;
    report(65);
    if previous {
        fs::rename(directory, &backup)?;
    }
    if let Err(error) = fs::rename(&staged, directory) {
        if previous {
            restore_backup(work, &backup, directory)?;
        }
        return Err(error);
    }
    if let Err(error) = register(directory, shortcuts) {
        if fs::rename(directory, &staged).is_err() {
            // Keep the old install available even if a new file became locked.
            let _ = work.keep();
            return Err(error);
        }
        if previous {
            restore_backup(work, &backup, directory)?;
        } else {
            let _ = unregister();
        }
        return Err(error);
    }
    report(100);
    Ok(())
}

fn restore_backup(work: tempfile::TempDir, backup: &Path, directory: &Path) -> io::Result<()> {
    if let Err(error) = fs::rename(backup, directory) {
        let location = work.keep();
        return Err(io::Error::new(
            error.kind(),
            format!("{error}. Backup: {}", location.display()),
        ));
    }
    Ok(())
}

fn launch_uninstaller(directory: &Path) -> io::Result<()> {
    validate_owned(directory)?;
    let temporary = tempfile::Builder::new()
        .prefix("torrent-re-cleanup-")
        .suffix(".exe")
        .tempfile()?;
    let copy = temporary.into_temp_path();
    fs::copy(std::env::current_exe()?, &copy)?;
    let parent = std::process::id();
    Command::new(&*copy)
        .args(["--cleanup"])
        .arg(directory)
        .arg(parent.to_string())
        .spawn()?;
    copy.keep()?;
    Ok(())
}

fn cleanup(directory: &Path, parent: &OsStr) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_FAILED};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };
    let pid = parent
        .to_str()
        .and_then(|v| v.parse::<u32>().ok())
        .ok_or_else(invalid_path)?;
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if !handle.is_null() {
        let result = unsafe { WaitForSingleObject(handle, 10_000) };
        unsafe { CloseHandle(handle) };
        if result != 0 {
            return Err(if result == WAIT_FAILED {
                io::Error::last_os_error()
            } else {
                io::Error::new(io::ErrorKind::TimedOut, "Setup is still running")
            });
        }
    }

    validate_owned(directory)?;
    if let Some(registered) = registered_directory()? {
        if !same_path(directory, &registered) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Installation folder does not match registration",
            ));
        }
    }
    let parent = directory.parent().ok_or_else(invalid_path)?;
    let staging = tempfile::Builder::new()
        .prefix("torrent-re-remove-")
        .tempdir_in(parent)?;
    let backup = staging.path().join("installed");
    fs::rename(directory, &backup)?;
    if let Err(error) = unregister() {
        restore_backup(staging, &backup, directory)?;
        let _ = register(directory, [true, true]);
        return Err(error);
    }
    staging.close()?;
    Ok(())
}

fn validate_owned(directory: &Path) -> io::Result<()> {
    validate_path(directory)?;
    if fs::read(directory.join(OWNER))? != NAME.as_bytes() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "This folder belongs to another installation",
        ));
    }
    check_owned_entries(directory, directory)
}

fn check_owned_entries(root: &Path, folder: &Path) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        let ty = fs::symlink_metadata(&path)?;
        if ty.file_attributes() & REPARSE_POINT != 0 {
            return Err(invalid_path());
        }
        let relative = path.strip_prefix(root).map_err(|_| invalid_path())?;
        let known = if ty.is_dir() {
            FILES
                .iter()
                .any(|(name, _)| Path::new(name).starts_with(relative))
        } else {
            relative == Path::new(OWNER)
                || relative == Path::new(SETUP)
                || FILES.iter().any(|(name, _)| relative == Path::new(name))
        };
        if !known {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Unrecognized file in installation: {}", relative.display()),
            ));
        }
        if ty.is_dir() {
            check_owned_entries(root, &path)?;
        }
    }
    Ok(())
}

fn validate_path(path: &Path) -> io::Result<()> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
    {
        return Err(invalid_path());
    }
    for component in path.ancestors() {
        match fs::symlink_metadata(component) {
            Ok(metadata)
                if !metadata.is_dir() || metadata.file_attributes() & REPARSE_POINT != 0 =>
            {
                return Err(invalid_path())
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn invalid_path() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "Invalid installation folder")
}

fn same_path(first: &Path, second: &Path) -> bool {
    let first = fs::canonicalize(first).unwrap_or_else(|_| first.to_owned());
    let second = fs::canonicalize(second).unwrap_or_else(|_| second.to_owned());
    use windows_sys::Win32::Globalization::{CompareStringOrdinal, CSTR_EQUAL};
    unsafe {
        CompareStringOrdinal(wide(first).as_ptr(), -1, wide(second).as_ptr(), -1, 1) == CSTR_EQUAL
    }
}

fn registered_directory() -> io::Result<Option<PathBuf>> {
    let key = wide(UNINSTALL);
    let name = wide("InstallLocation");
    let mut count = 0;
    let first = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut count,
        )
    };
    if first == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check_registry(first)?;
    let mut buffer = vec![0u16; (count as usize).div_ceil(2)];
    let second = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if second == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    check_registry(second)?;
    let end = buffer
        .iter()
        .position(|&ch| ch == 0)
        .unwrap_or(buffer.len());
    if end == 0 {
        return Ok(None);
    }
    use std::os::windows::ffi::OsStringExt;
    Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..end],
    ))))
}

fn register(directory: &Path, shortcuts: [bool; 2]) -> io::Result<()> {
    let path = directory.to_str().ok_or_else(invalid_path)?;
    let application = directory.join(APP);
    let uninstall = directory.join(SETUP);
    let launch = format!("\"{}\" \"%1\"", application.display());
    let remove = format!("\"{}\" --uninstall \"{path}\"", uninstall.display());
    for (key, name, value) in [
        (UNINSTALL, "DisplayName", NAME),
        (UNINSTALL, "DisplayVersion", VERSION),
        (UNINSTALL, "InstallLocation", path),
        (UNINSTALL, "UninstallString", &remove),
        (FILE_CLASS, "", "Torrent File (Rust Edition)"),
        (MAGNET_CLASS, "", "Magnet link (Rust Edition)"),
        (MAGNET_CLASS, "URL Protocol", ""),
        (TORRENT_OPEN_WITH, "TorrentRustEdition.File", ""),
        (CAPABILITIES, "ApplicationName", NAME),
        (REGISTERED, NAME, CAPABILITIES),
    ] {
        set_string(key, name, value)?;
    }
    set_string(&format!(r"{FILE_CLASS}\shell\open\command"), "", &launch)?;
    set_string(&format!(r"{MAGNET_CLASS}\shell\open\command"), "", &launch)?;
    set_string(
        &format!(r"{CAPABILITIES}\FileAssociations"),
        ".torrent",
        "TorrentRustEdition.File",
    )?;
    set_string(
        &format!(r"{CAPABILITIES}\URLAssociations"),
        "magnet",
        "TorrentRustEdition.Magnet",
    )?;
    for (shortcut, enabled) in shortcut_paths()?.into_iter().zip(shortcuts) {
        if enabled {
            create_shortcut(&shortcut, &application, directory)?;
        } else {
            remove_shortcut(&shortcut)?;
        }
    }
    Ok(())
}

fn unregister() -> io::Result<()> {
    for shortcut in shortcut_paths()? {
        remove_shortcut(&shortcut)?;
    }
    for key in [
        UNINSTALL,
        FILE_CLASS,
        MAGNET_CLASS,
        r"Software\TorrentRustEdition",
    ] {
        check_registry(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(key).as_ptr()) })?;
    }
    for (key, name) in [
        (REGISTERED, NAME),
        (TORRENT_OPEN_WITH, "TorrentRustEdition.File"),
    ] {
        with_key(key, |handle| {
            check_registry(unsafe { RegDeleteValueW(handle, wide(name).as_ptr()) })
        })?;
    }
    Ok(())
}

fn remove_shortcut(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn shortcut_paths() -> io::Result<[PathBuf; 2]> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Programs, SHGetKnownFolderPath,
    };

    fn path(id: &windows_sys::core::GUID) -> io::Result<PathBuf> {
        let mut raw = null_mut();
        let result = unsafe { SHGetKnownFolderPath(id, 0, null_mut(), &mut raw) };
        if result < 0 {
            return Err(io::Error::other(format!(
                "Could not locate shortcut folder ({result:#x})"
            )));
        }
        let mut length = 0;
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
            let folder = std::ffi::OsString::from_wide(std::slice::from_raw_parts(raw, length));
            CoTaskMemFree(raw.cast());
            Ok(PathBuf::from(folder).join("Torrent Rust Edition.lnk"))
        }
    }

    Ok([path(&FOLDERID_Desktop)?, path(&FOLDERID_Programs)?])
}

fn create_shortcut(path: &Path, executable: &Path, directory: &Path) -> io::Result<()> {
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    // The installer invokes this on its own Win32 thread.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|error| io::Error::other(error.to_string()))?;
    let result = unsafe {
        (|| -> windows::core::Result<()> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(PCWSTR(wide(executable).as_ptr()))?;
            link.SetWorkingDirectory(PCWSTR(wide(directory).as_ptr()))?;
            let file: IPersistFile = link.cast()?;
            file.Save(PCWSTR(wide(path).as_ptr()), true)
        })()
    };
    unsafe { CoUninitialize() };
    result.map_err(|error| io::Error::other(error.to_string()))
}

fn set_string(key: &str, name: &str, value: &str) -> io::Result<()> {
    with_key(key, |handle| {
        let value = wide(value);
        let code = unsafe {
            RegSetValueExW(
                handle,
                wide(name).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        };
        if code == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(code as i32))
        }
    })
}

fn with_key(key: &str, action: impl FnOnce(HKEY) -> io::Result<()>) -> io::Result<()> {
    let mut handle = null_mut();
    let code = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(key).as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut handle,
            null_mut(),
        )
    };
    if code != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let result = action(handle);
    unsafe { RegCloseKey(handle) };
    result
}

fn check_registry(code: u32) -> io::Result<()> {
    match code {
        ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
        code => Err(io::Error::from_raw_os_error(code as i32)),
    }
}

fn confirm(text: &str) -> bool {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(text).as_ptr(),
            wide(NAME).as_ptr(),
            MB_YESNO | MB_ICONINFORMATION,
        ) == IDYES
    }
}

pub fn message(text: &str, error: bool) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(text).as_ptr(),
            wide(NAME).as_ptr(),
            MB_OK
                | if error {
                    MB_ICONERROR
                } else {
                    MB_ICONINFORMATION
                },
        )
    };
}

fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}
