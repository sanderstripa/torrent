use std::ffi::c_void;
use std::mem::zeroed;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use torrent_re_sort::profile::{Rect, Theme, WindowPosition};
use torrent_re_sort::{
    engine::Session, profile::Preferences, queue_policy::PREVIEW_POLICY, transfers::Transfers,
};
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows_sys::Win32::Globalization::{CompareStringEx, NORM_IGNORECASE, SORT_DIGITSASNUMBERS};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::Graphics::Gdi::COLOR_WINDOW;
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromRect, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetActiveWindow, VK_ESCAPE};
use windows_sys::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW};
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
const FILES: usize = 109;
const SETTINGS: usize = 110;
const ACCEPT: usize = 111;
const CANCEL: usize = 112;
const BY_NAME: usize = 113;
const BY_SIZE: usize = 114;

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
        WM_COMMAND if wparam >> 16 == 0 && (ADD_MAGNET..=BY_SIZE).contains(&(wparam & 0xffff)) => {
            unsafe { PostMessageW(window, ACTION, wparam & 0xffff, 0) };
            0
        }
        WM_DESTROY => {
            if unsafe { GetWindow(window, GW_OWNER) }.is_null() {
                unsafe { PostQuitMessage(0) };
            }
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn child(
    window: HWND,
    class: &str,
    label: &str,
    style: u32,
    (x, y, width, height): (i32, i32, i32, i32),
    id: usize,
) -> Result<HWND, String> {
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
            GetModuleHandleW(null()),
            null(),
        )
    };
    if handle.is_null() {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(handle)
    }
}

fn edit_text(edit: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(edit) }.max(0);
    let mut text = vec![0; length as usize + 1];
    let count = unsafe { GetWindowTextW(edit, text.as_mut_ptr(), length + 1) }.max(0);
    String::from_utf16_lossy(&text[..count as usize])
}

struct Dialog {
    owner: HWND,
    window: HWND,
}

impl Dialog {
    fn create(owner: HWND, title: &str, width: i32, height: i32) -> Result<Self, String> {
        let window = unsafe {
            CreateWindowExW(
                WS_EX_DLGMODALFRAME | WS_EX_CONTROLPARENT,
                wide("TorrentRustDevelopment").as_ptr(),
                wide(title).as_ptr(),
                WS_POPUP | WS_CAPTION | WS_SYSMENU,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                owner,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            )
        };
        if window.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(Self { owner, window })
    }

    fn buttons(&self, y: i32) -> Result<(), String> {
        self.control("BUTTON", "ОК", WS_TABSTOP, (20, y, 100, 30), ACCEPT)?;
        self.control("BUTTON", "Отмена", WS_TABSTOP, (130, y, 100, 30), CANCEL)?;
        Ok(())
    }

    fn control(
        &self,
        class: &str,
        text: &str,
        style: u32,
        geometry: (i32, i32, i32, i32),
        id: usize,
    ) -> Result<HWND, String> {
        child(self.window, class, text, style, geometry, id)
    }

    fn run(&self, mut command: impl FnMut(usize) -> bool) {
        unsafe {
            EnableWindow(self.owner, 0);
            ShowWindow(self.window, SW_SHOW);
            let mut message = MSG::default();
            while IsWindow(self.window) != 0 {
                match GetMessageW(&mut message, null_mut(), 0, 0) {
                    -1 => break,
                    0 => {
                        PostQuitMessage(message.wParam as i32);
                        break;
                    }
                    _ if message.hwnd == self.window && message.message == ACTION => {
                        if command(message.wParam) {
                            break;
                        }
                    }
                    _ if message.message == WM_KEYDOWN && message.wParam == VK_ESCAPE as usize => {
                        break
                    }
                    _ if IsDialogMessageW(self.window, &message) != 0 => {}
                    _ => {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
        }
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        unsafe {
            EnableWindow(self.owner, 1);
            if IsWindow(self.window) != 0 {
                DestroyWindow(self.window);
            }
            SetActiveWindow(self.owner);
        }
    }
}

fn settings(owner: HWND, prefs: &Preferences) -> Result<Option<Preferences>, String> {
    use torrent_re_sort::profile::Theme;
    let dialog = Dialog::create(owner, "Настройки", 590, 250)?;
    let folder = dialog.control(
        "EDIT",
        &prefs.folder.to_string_lossy(),
        WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
        (20, 20, 530, 28),
        0,
    )?;
    let files = dialog.control(
        "BUTTON",
        "Выбирать файлы перед загрузкой",
        WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        (20, 58, 270, 24),
        0,
    )?;
    let english = dialog.control(
        "BUTTON",
        "English",
        WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        (20, 90, 150, 24),
        0,
    )?;
    let theme = dialog.control(
        "COMBOBOX",
        "",
        WS_TABSTOP | CBS_DROPDOWNLIST as u32,
        (20, 122, 190, 125),
        0,
    )?;
    unsafe {
        SendMessageW(files, BM_SETCHECK, prefs.select_files as usize, 0);
        SendMessageW(english, BM_SETCHECK, prefs.english as usize, 0);
        for item in ["Светлая", "Тёмная", "Системная"] {
            SendMessageW(theme, CB_ADDSTRING, 0, wide(item).as_ptr() as LPARAM);
        }
        SendMessageW(theme, CB_SETCURSEL, prefs.theme as usize, 0);
    }
    dialog.buttons(160)?;
    let mut updated = None;
    dialog.run(|id| match id {
        CANCEL => true,
        ACCEPT => {
            let folder = PathBuf::from(edit_text(folder));
            if !folder.is_absolute() {
                show_error("Укажите абсолютный путь к папке загрузок.");
                return false;
            }
            let theme = match unsafe { SendMessageW(theme, CB_GETCURSEL, 0, 0) } {
                1 => Theme::Dark,
                2 => Theme::System,
                _ => Theme::Light,
            };
            updated = Some(Preferences {
                folder,
                theme,
                select_files: unsafe { SendMessageW(files, BM_GETCHECK, 0, 0) } == 1,
                english: unsafe { SendMessageW(english, BM_GETCHECK, 0, 0) } == 1,
            });
            true
        }
        _ => false,
    });
    Ok(updated)
}

fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (wide(a), wide(b));
    let order = unsafe {
        CompareStringEx(
            null(),
            NORM_IGNORECASE | SORT_DIGITSASNUMBERS,
            a.as_ptr(),
            -1,
            b.as_ptr(),
            -1,
            null(),
            null_mut(),
            0,
        )
    };
    match order {
        1 => std::cmp::Ordering::Less,
        3 => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

fn select_torrent_files(
    owner: HWND,
    transfer: &torrent_re_sort::transfers::Transfer<'_>,
) -> Result<Option<Vec<u8>>, String> {
    let files = transfer.handle.files()?;
    if files.is_empty() {
        return Err("Файлы станут доступны после получения метаданных.".into());
    }
    let mut priorities = transfer.handle.file_priorities()?;
    priorities.resize(files.len(), 4);
    if transfer.selection_pending {
        for priority in &mut priorities {
            if *priority == 0 {
                *priority = 4;
            }
        }
    }
    let dialog = Dialog::create(owner, "Файлы торрента", 730, 510)?;
    let list = dialog.control(
        "LISTBOX",
        "",
        WS_BORDER | WS_VSCROLL | WS_TABSTOP | LBS_EXTENDEDSEL as u32,
        (20, 52, 670, 352),
        0,
    )?;
    dialog.control("BUTTON", "По имени", WS_TABSTOP, (20, 14, 110, 28), BY_NAME)?;
    dialog.control(
        "BUTTON",
        "По размеру",
        WS_TABSTOP,
        (140, 14, 110, 28),
        BY_SIZE,
    )?;
    dialog.buttons(418)?;
    let mut indices: Vec<_> = (0..files.len()).collect();
    let mut accepted = false;
    let mut sorting = (0, false);
    let read_selection = |indices: &[usize], priorities: &mut [u8]| {
        for (row, &index) in indices.iter().enumerate() {
            let selected = unsafe { SendMessageW(list, LB_GETSEL, row, 0) } > 0;
            priorities[index] = if selected {
                priorities[index].max(1)
            } else {
                0
            };
        }
    };
    let redraw = |indices: &[usize], priorities: &[u8]| unsafe {
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        for (row, &index) in indices.iter().enumerate() {
            let file = &files[index];
            let label = format!("{}  •  {} B", file.path, file.size);
            SendMessageW(list, LB_ADDSTRING, 0, wide(label).as_ptr() as LPARAM);
            SendMessageW(
                list,
                LB_SETSEL,
                (priorities[index] != 0) as usize,
                row as LPARAM,
            );
        }
    };
    redraw(&indices, &priorities);
    dialog.run(|id| match id {
        ACCEPT => {
            read_selection(&indices, &mut priorities);
            accepted = true;
            true
        }
        CANCEL => true,
        BY_NAME | BY_SIZE => {
            read_selection(&indices, &mut priorities);
            sorting = (id, sorting.0 == id && !sorting.1);
            indices.sort_by(|&a, &b| {
                let order = if id == BY_SIZE {
                    files[a].size.cmp(&files[b].size)
                } else {
                    compare_names(&files[a].path, &files[b].path)
                };
                (if sorting.1 { order.reverse() } else { order }).then(a.cmp(&b))
            });
            redraw(&indices, &priorities);
            false
        }
        _ => false,
    });
    Ok(accepted.then_some(priorities))
}

struct Controls {
    magnet: HWND,
    list: HWND,
}

impl Controls {
    fn create(window: HWND) -> Result<Self, String> {
        let child =
            |class, label, style, geometry, id| child(window, class, label, style, geometry, id);
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
        child(
            "BUTTON",
            "Настройки",
            WS_TABSTOP,
            (715, 20, 120, 32),
            SETTINGS,
        )?;
        child("BUTTON", "Файлы", WS_TABSTOP, (700, 62, 100, 32), FILES)?;
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
        edit_text(self.magnet).trim().to_owned()
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

fn drop_files(drop: windows_sys::Win32::UI::Shell::HDROP) -> Vec<String> {
    unsafe {
        let count = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
        let paths = (0..count)
            .filter_map(|index| {
                let length = DragQueryFileW(drop, index, null_mut(), 0);
                if length == 0 {
                    return None;
                }
                let mut buffer = vec![0; length as usize + 1];
                let copied = DragQueryFileW(drop, index, buffer.as_mut_ptr(), length + 1);
                (copied != 0).then(|| String::from_utf16_lossy(&buffer[..copied as usize]))
            })
            .collect();
        DragFinish(drop);
        paths
    }
}

fn prompt_for_files(
    owner: HWND,
    queue: &mut Transfers<'_>,
    prompted: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    let next = queue.ordered().iter().position(|entry| {
        entry.selection_pending
            && !entry.stopped
            && !prompted.contains(&entry.key)
            && entry
                .handle
                .status()
                .is_ok_and(|(_, status)| status.has_metadata)
    });
    if let Some(index) = next {
        let entry = &queue.ordered()[index];
        prompted.insert(entry.key.clone());
        if let Some(priorities) = select_torrent_files(owner, entry)? {
            queue.select_files(index, &priorities)?;
        }
    }
    Ok(())
}

fn apply_title_theme(window: HWND, theme: Theme) {
    let dark = match theme {
        Theme::Light => 0,
        Theme::Dark => 1,
        Theme::System => {
            let mut light: u32 = 1;
            let mut size = std::mem::size_of::<u32>() as u32;
            let result = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize").as_ptr(),
                    wide("AppsUseLightTheme").as_ptr(),
                    RRF_RT_REG_DWORD,
                    null_mut(),
                    (&mut light as *mut u32).cast(),
                    &mut size,
                )
            };
            i32::from(result == 0 && light == 0)
        }
    };
    unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&dark as *const i32).cast(),
            std::mem::size_of_val(&dark) as u32,
        );
    }
}

fn monitor_work(monitor: windows_sys::Win32::Graphics::Gdi::HMONITOR) -> Option<Rect> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    let RECT {
        left,
        top,
        right,
        bottom,
    } = info.rcWork;
    (right > left && bottom > top).then_some(Rect {
        left,
        top,
        right,
        bottom,
    })
}

fn restore_position(window: HWND, root: &std::path::Path) {
    let Some(saved) = std::fs::read_to_string(root.join("window-position.txt"))
        .ok()
        .and_then(|text| WindowPosition::decode(&text))
    else {
        return;
    };
    let mut original = RECT::default();
    if unsafe { GetWindowRect(window, &mut original) } == 0 {
        return;
    }
    let (width, height) = (
        original.right - original.left,
        original.bottom - original.top,
    );
    let area = RECT {
        left: saved.x,
        top: saved.y,
        right: saved.x + width,
        bottom: saved.y + height,
    };
    if let Some(work) = monitor_work(unsafe { MonitorFromRect(&area, MONITOR_DEFAULTTONEAREST) }) {
        let (x, y) = saved.restore(width, height, work);
        unsafe {
            SetWindowPos(
                window,
                null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }
}

fn save_position(window: HWND, root: &std::path::Path) -> std::io::Result<()> {
    let mut area = RECT::default();
    if unsafe { IsIconic(window) | IsZoomed(window) } != 0
        || unsafe { GetWindowRect(window, &mut area) } == 0
    {
        return Ok(());
    }
    if let Some(work) = monitor_work(unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) })
    {
        std::fs::create_dir_all(root)?;
        WindowPosition {
            x: area.left,
            y: area.top,
            work,
        }
        .save(root)?;
    }
    Ok(())
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
    let mut prefs =
        Preferences::load(&profile, default_folder).map_err(|error| error.to_string())?;
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
        apply_title_theme(window, prefs.theme);
        if !smoke {
            restore_position(window, &profile);
        }
        let controls = Controls::create(window)?;
        let mut prompted = std::collections::HashSet::new();
        if !smoke {
            for source in std::env::args_os().skip(1) {
                if let Err(error) = transfers.add(
                    &source.to_string_lossy(),
                    &prefs.folder.to_string_lossy(),
                    prefs.select_files,
                ) {
                    show_error(&error);
                }
            }
        }
        controls.refresh(&transfers);
        if smoke {
            DestroyWindow(window);
        } else {
            DragAcceptFiles(window, 1);
            SetTimer(window, 1, 1000, None);
            ShowWindow(window, SW_SHOW);
        }
        let mut message = MSG::default();
        loop {
            match GetMessageW(&mut message, null_mut(), 0, 0) {
                -1 => return Err(std::io::Error::last_os_error().to_string()),
                0 => break,
                _ if message.hwnd == window && message.message == WM_CLOSE => {
                    if !smoke {
                        if let Err(error) = save_position(window, &profile) {
                            show_error(&error.to_string());
                        }
                    }
                    DispatchMessageW(&message);
                }
                _ if message.hwnd == window && message.message == WM_SETTINGCHANGE => {
                    apply_title_theme(window, prefs.theme);
                    DispatchMessageW(&message);
                }
                _ if message.hwnd == window && message.message == WM_TIMER => {
                    if let Err(error) = prompt_for_files(window, &mut transfers, &mut prompted) {
                        show_error(&error);
                    }
                    controls.refresh(&transfers)
                }
                _ if message.hwnd == window && message.message == WM_DROPFILES => {
                    for source in drop_files(message.wParam as _) {
                        if let Err(error) = transfers.add(
                            &source,
                            &prefs.folder.to_string_lossy(),
                            prefs.select_files,
                        ) {
                            show_error(&error);
                        }
                    }
                    controls.refresh(&transfers);
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
                        FILES => selected.map_or(Ok(()), |row| {
                            if let Some(priorities) =
                                select_torrent_files(window, &transfers.ordered()[row])?
                            {
                                transfers.select_files(row, &priorities)?;
                            }
                            Ok(())
                        }),
                        SETTINGS => settings(window, &prefs).and_then(|change| {
                            if let Some(updated) = change {
                                std::fs::create_dir_all(&profile)
                                    .map_err(|error| error.to_string())?;
                                updated.save(&profile).map_err(|error| error.to_string())?;
                                prefs = updated;
                                apply_title_theme(window, prefs.theme);
                            }
                            Ok(())
                        }),
                        _ => Ok(()),
                    };
                    if let Err(error) = result {
                        show_error(&error);
                    }
                    controls.refresh_selection(&transfers, selected);
                }
                _ if IsDialogMessageW(window, &message) != 0 => {}
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
