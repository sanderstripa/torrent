use std::ffi::c_void;
use std::mem::zeroed;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use torrent_re_sort::{
    engine::Session, profile::Preferences, queue_policy::PREVIEW_POLICY, transfers::Transfers,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::COLOR_WINDOW;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OPENFILENAMEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const ACTION: u32 = WM_APP + 1;
const ADD_MAGNET: usize = 101;
const ADD_TORRENT: usize = 102;
const PAUSE: usize = 103;
const RESUME: usize = 104;
const STOP: usize = 105;
const REMOVE: usize = 106;
const UP: usize = 107;
const DOWN: usize = 108;

fn wide(text: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("Torrent").as_ptr(),
            MB_ICONERROR,
        )
    };
}

unsafe extern "system" fn dispatch(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_COMMAND if wparam >> 16 == 0 && (ADD_MAGNET..=DOWN).contains(&(wparam & 0xffff)) => {
            unsafe { PostMessageW(window, ACTION, wparam & 0xffff, 0) };
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

struct Controls {
    magnet: HWND,
    list: HWND,
}

impl Controls {
    fn create(window: HWND) -> Result<Self, String> {
        let child = |class, label, style, (x, y, width, height), id| {
            let handle = unsafe {
                CreateWindowExW(
                    0,
                    wide(class).as_ptr(),
                    wide(label).as_ptr(),
                    WS_CHILD | WS_VISIBLE | style,
                    x,
                    y,
                    width,
                    height,
                    window,
                    id as *mut c_void,
                    null_mut(),
                    null(),
                )
            };
            if handle.is_null() {
                Err(std::io::Error::last_os_error().to_string())
            } else {
                Ok(handle)
            }
        };
        let magnet = child(
            "EDIT",
            "",
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            (20, 20, 465, 32),
            0,
        )?;
        child(
            "BUTTON",
            "Magnet",
            WS_TABSTOP,
            (495, 20, 100, 32),
            ADD_MAGNET,
        )?;
        child(
            "BUTTON",
            ".torrent",
            WS_TABSTOP,
            (605, 20, 100, 32),
            ADD_TORRENT,
        )?;
        child("BUTTON", "Пауза", WS_TABSTOP, (20, 62, 100, 32), PAUSE)?;
        child(
            "BUTTON",
            "Продолжить",
            WS_TABSTOP,
            (130, 62, 120, 32),
            RESUME,
        )?;
        for (id, label, x) in [
            (STOP, "Стоп", 260),
            (REMOVE, "Удалить", 370),
            (UP, "Выше", 480),
            (DOWN, "Ниже", 590),
        ] {
            child("BUTTON", label, WS_TABSTOP, (x, 62, 100, 32), id)?;
        }
        let list = child(
            "LISTBOX",
            "",
            WS_BORDER | WS_VSCROLL | WS_TABSTOP,
            (20, 105, 870, 520),
            0,
        )?;
        Ok(Self { magnet, list })
    }

    fn selected(&self) -> Option<usize> {
        let row = unsafe { SendMessageW(self.list, LB_GETCURSEL, 0, 0) };
        usize::try_from(row).ok()
    }

    fn refresh(&self, transfers: &Transfers<'_>) {
        self.refresh_selection(transfers, self.selected());
    }

    fn refresh_selection(&self, transfers: &Transfers<'_>, selected: Option<usize>) {
        let top = unsafe { SendMessageW(self.list, LB_GETTOPINDEX, 0, 0) }.max(0);
        unsafe { SendMessageW(self.list, WM_SETREDRAW, 0, 0) };
        unsafe { SendMessageW(self.list, LB_RESETCONTENT, 0, 0) };
        for entry in transfers.ordered() {
            let label = match entry.handle.status() {
                Ok((name, status)) => {
                    let progress = status.progress_ppm as f64 / 10_000.0;
                    let state = if entry.selection_pending {
                        "Ожидание выбора файлов"
                    } else if entry.stopped {
                        "Остановлен"
                    } else if status.paused {
                        "Пауза"
                    } else if status.seeding {
                        "Раздача"
                    } else {
                        "Загрузка"
                    };
                    format!(
                        "{name}  •  {state}  •  {progress:.1}%  •  ↓ {} KiB/s",
                        status.download_rate / 1024
                    )
                }
                Err(error) => format!("{}: {error}", entry.key),
            };
            unsafe { SendMessageW(self.list, LB_ADDSTRING, 0, wide(label).as_ptr() as LPARAM) };
        }
        if let Some(row) = selected {
            unsafe { SendMessageW(self.list, LB_SETCURSEL, row, 0) };
        }
        unsafe {
            SendMessageW(self.list, LB_SETTOPINDEX, top as usize, 0);
            SendMessageW(self.list, WM_SETREDRAW, 1, 0);
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(self.list, null(), 1);
        }
    }

    fn magnet(&self) -> String {
        let length = unsafe { GetWindowTextLengthW(self.magnet) }.max(0);
        let mut text = vec![0; length as usize + 1];
        let count = unsafe { GetWindowTextW(self.magnet, text.as_mut_ptr(), length + 1) }.max(0);
        String::from_utf16_lossy(&text[..count as usize])
            .trim()
            .to_owned()
    }
}

fn torrent_file(owner: HWND) -> Option<String> {
    let mut filename = vec![0; 32_768];
    let filter = wide("Torrent (*.torrent)\0*.torrent\0");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_ptr(),
        lpstrFile: filename.as_mut_ptr(),
        nMaxFile: filename.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
        ..unsafe { zeroed() }
    };
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return None;
    }
    let end = filename
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(filename.len());
    Some(String::from_utf16_lossy(&filename[..end]))
}

struct ProcessLock(HANDLE);

impl Drop for ProcessLock {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn run() -> Result<(), String> {
    let smoke = std::env::args_os().any(|argument| argument == "--smoke-test");
    let _lock = if smoke {
        None
    } else {
        let name = wide("Local\\Torrent.RustEdition.Development");
        let mutex = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        if mutex.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let lock = ProcessLock(mutex);
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Ok(());
        }
        Some(lock)
    };
    // The development shell uses a separate profile until migration is validated.
    let profile = if smoke {
        std::env::temp_dir().join(format!("torrent-re-smoke-{}", std::process::id()))
    } else {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?)
            .join("Torrent RE Development")
    };
    let default_folder = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| profile.clone())
        .join("Downloads");
    let prefs = Preferences::load(&profile, default_folder).map_err(|error| error.to_string())?;
    let session = Session::open(PREVIEW_POLICY, smoke)?;
    let (mut transfers, warnings) = Transfers::open(&session, &profile.join("session"))?;
    if !warnings.is_empty() && !smoke {
        show_error(&warnings.join("\n"));
    }
    unsafe {
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let class = wide("TorrentRustDevelopment");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(dispatch),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW as usize + 1) as *mut c_void,
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&window_class) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let title = wide("Torrent Rust Edition");
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            940,
            720,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if window.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let controls = Controls::create(window)?;
        controls.refresh(&transfers);
        if smoke {
            DestroyWindow(window);
        } else {
            SetTimer(window, 1, 1000, None);
            ShowWindow(window, SW_SHOW);
        }
        let mut message = MSG::default();
        loop {
            match GetMessageW(&mut message, null_mut(), 0, 0) {
                -1 => return Err(std::io::Error::last_os_error().to_string()),
                0 => break,
                _ if message.hwnd == window && message.message == WM_TIMER => {
                    controls.refresh(&transfers)
                }
                _ if message.hwnd == window && message.message == ACTION => {
                    let destination = prefs.folder.to_string_lossy();
                    let mut selected = controls.selected();
                    let result = match message.wParam {
                        ADD_MAGNET => {
                            let source = controls.magnet();
                            if source.starts_with("magnet:") {
                                transfers
                                    .add(&source, &destination, prefs.select_files)
                                    .map(|_| ())
                            } else {
                                Err("Введите magnet-ссылку".into())
                            }
                        }
                        ADD_TORRENT => torrent_file(window).map_or(Ok(()), |source| {
                            transfers
                                .add(&source, &destination, prefs.select_files)
                                .map(|_| ())
                        }),
                        PAUSE => controls
                            .selected()
                            .map_or(Ok(()), |row| transfers.pause(row, false)),
                        RESUME => controls
                            .selected()
                            .map_or(Ok(()), |row| transfers.resume(row)),
                        STOP => selected.map_or(Ok(()), |row| transfers.pause(row, true)),
                        REMOVE => selected.map_or(Ok(()), |row| transfers.remove(row)),
                        UP | DOWN => selected.map_or(Ok(()), |row| {
                            transfers
                                .move_row(row, message.wParam == UP)
                                .map(|row| selected = Some(row))
                        }),
                        _ => Ok(()),
                    };
                    if let Err(error) = result {
                        show_error(&error);
                    }
                    controls.refresh_selection(&transfers, selected);
                }
                _ => {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    }
    if !smoke {
        transfers.save()?;
    }
    Ok(())
}
