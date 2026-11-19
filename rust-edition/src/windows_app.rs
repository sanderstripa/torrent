use std::ffi::c_void;
use std::mem::zeroed;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use torrent_re_sort::{engine::Session, queue_policy::PREVIEW_POLICY, transfers::Transfers};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::COLOR_WINDOW;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

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
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
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
    let session = Session::open(PREVIEW_POLICY, smoke)?;
    let (transfers, warnings) = Transfers::open(&session, &profile.join("session"))?;
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
        if smoke {
            DestroyWindow(window);
        } else {
            ShowWindow(window, SW_SHOW);
        }
        let mut message = MSG::default();
        loop {
            match GetMessageW(&mut message, null_mut(), 0, 0) {
                -1 => return Err(std::io::Error::last_os_error().to_string()),
                0 => break,
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
