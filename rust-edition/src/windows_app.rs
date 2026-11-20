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
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::System::SystemServices::SS_CENTERIMAGE;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, SetActiveWindow, VK_DELETE, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, FOLDERID_Downloads, SHBrowseForFolderW,
    SHGetKnownFolderPath, SHGetPathFromIDListW, ShellExecuteW, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
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
const FILES: usize = 109;
const SETTINGS: usize = 110;
const ACCEPT: usize = 111;
const CANCEL: usize = 112;
const BY_NAME: usize = 113;
const BY_SIZE: usize = 114;
const PRIORITY_LOW: usize = 115;
const PRIORITY_NORMAL: usize = 116;
const PRIORITY_HIGH: usize = 117;
const OPEN_ADD: usize = 118;
const MINIMIZE: usize = 119;
const CLOSE: usize = 120;
const CHOOSE_FOLDER: usize = 121;
const ABOUT_UPDATE: usize = 122;
const ABOUT_CONNECT: usize = 123;
const THEME_SYSTEM: usize = 124;
const THEME_LIGHT: usize = 125;
const THEME_DARK: usize = 126;
const CHANGE_LANGUAGE: usize = 127;
const CHANGE_ADD: usize = 128;
const CONTEXT: u32 = WM_APP + 2;
const LAYOUT: u32 = WM_APP + 3;
const REQUEST_CLOSE: u32 = WM_APP + 4;
const APPLY_THEME: u32 = WM_APP + 5;
const SHELL_SOURCES: u32 = WM_APP + 6;
const WINDOW_MOVED: u32 = WM_APP + 7;
const CLASS: windows_sys::core::PCWSTR = windows_sys::w!("TorrentRustDevelopment");
thread_local! {
    static SHELL_INBOX: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn tr<'a>(english: bool, ru: &'a str, en: &'a str) -> &'a str {
    if english {
        en
    } else {
        ru
    }
}

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
        WM_COPYDATA if unsafe { GetWindow(window, GW_OWNER) }.is_null() => {
            let Some(packet) = (unsafe { (lparam as *const COPYDATASTRUCT).as_ref() }) else {
                return 0;
            };
            let length = packet.cbData as usize;
            if packet.dwData != 1
                || !(2..=65_536).contains(&length)
                || length % 2 != 0
                || packet.lpData.is_null()
            {
                return 0;
            }
            let buffer =
                unsafe { std::slice::from_raw_parts(packet.lpData.cast::<u16>(), length / 2) };
            if buffer.last() != Some(&0) || buffer[..buffer.len() - 1].contains(&0) {
                return 0;
            }
            let source = String::from_utf16_lossy(&buffer[..buffer.len() - 1]);
            SHELL_INBOX.with(|inbox| inbox.borrow_mut().push(source));
            unsafe { PostMessageW(window, SHELL_SOURCES, 0, 0) };
            1
        }
        WM_COMMAND if wparam & 0xffff == 200 && wparam >> 16 == LBN_DBLCLK as usize => {
            unsafe { PostMessageW(window, ACTION, FILES, 0) };
            0
        }
        WM_LBUTTONUP
            if unsafe {
                !GetPropW(window, windows_sys::w!("TorrentSettingsBackdrop")).is_null()
            } =>
        {
            let sheet = unsafe { GetPropW(window, windows_sys::w!("TorrentSettingsBackdrop")) };
            unsafe { PostMessageW(sheet, ACTION, CANCEL, 0) };
            0
        }
        WM_ERASEBKGND
            if unsafe {
                !GetPropW(window, windows_sys::w!("TorrentSettingsBackdrop")).is_null()
            } =>
        {
            let mut area = RECT::default();
            unsafe {
                GetClientRect(window, &mut area);
                windows_sys::Win32::Graphics::Gdi::FillRect(
                    wparam as _,
                    &area,
                    windows_sys::Win32::Graphics::Gdi::GetStockObject(
                        windows_sys::Win32::Graphics::Gdi::BLACK_BRUSH,
                    ),
                );
            }
            1
        }
        WM_ERASEBKGND if unsafe { GetWindow(window, GW_OWNER) }.is_null() => {
            let mut area = RECT::default();
            unsafe { GetClientRect(window, &mut area) };
            unsafe { crate::windows_paint::background(wparam as _, area, window_dark(window)) };
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORLISTBOX | WM_CTLCOLORDLG => unsafe {
            crate::windows_paint::control_background(wparam as _, window_dark(window)) as LRESULT
        },
        WM_DRAWITEM if [OPEN_ADD, SETTINGS, MINIMIZE, CLOSE].contains(&wparam) => {
            let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            let label = match wparam {
                OPEN_ADD => "+",
                SETTINGS => "⚙",
                MINIMIZE => "−",
                _ => "×",
            };
            unsafe { crate::windows_paint::caption_button(draw, label, window_dark(window)) };
            1
        }
        WM_MOVE | WM_SIZE | WM_CLOSE | WM_SETTINGCHANGE
            if unsafe { GetWindow(window, GW_OWNER) }.is_null() =>
        {
            let action = match message {
                WM_SIZE => LAYOUT,
                WM_MOVE => WINDOW_MOVED,
                WM_CLOSE => REQUEST_CLOSE,
                _ => APPLY_THEME,
            };
            unsafe { PostMessageW(window, action, 0, 0) };
            0
        }
        WM_CONTEXTMENU => {
            unsafe { PostMessageW(window, CONTEXT, wparam, lparam) };
            0
        }
        WM_NCHITTEST if unsafe { GetWindow(window, GW_OWNER) }.is_null() => {
            let hit = unsafe { DefWindowProcW(window, message, wparam, lparam) };
            if hit == HTCLIENT as isize {
                let mut point = windows_sys::Win32::Foundation::POINT {
                    x: lparam as i16 as i32,
                    y: (lparam >> 16) as i16 as i32,
                };
                unsafe { windows_sys::Win32::Graphics::Gdi::ScreenToClient(window, &mut point) };
                if (0..62).contains(&point.y) {
                    let child = unsafe { ChildWindowFromPoint(window, point) };
                    if child == window || child.is_null() || unsafe { GetDlgCtrlID(child) } == 190 {
                        return HTCAPTION as isize;
                    }
                }
            }
            hit
        }
        WM_NCLBUTTONDBLCLK if wparam == HTCAPTION as usize => 0,
        WM_SYSCOMMAND if wparam & 0xfff0 == SC_MAXIMIZE as usize => 0,
        WM_MEASUREITEM if wparam == 200 => {
            unsafe { (*(lparam as *mut MEASUREITEMSTRUCT)).itemHeight = 136 };
            1
        }
        WM_DRAWITEM if wparam == 200 => {
            unsafe { crate::windows_paint::transfer(&*(lparam as *const DRAWITEMSTRUCT)) };
            1
        }
        WM_COMMAND
            if (wparam >> 16 == 0 && (ADD_MAGNET..=THEME_DARK).contains(&(wparam & 0xffff)))
                || (wparam >> 16 == CBN_SELCHANGE as usize
                    && [CHANGE_LANGUAGE, CHANGE_ADD].contains(&(wparam & 0xffff))) =>
        {
            unsafe { PostMessageW(window, ACTION, wparam & 0xffff, 0) };
            0
        }
        WM_DESTROY => {
            unsafe { RemovePropW(window, windows_sys::w!("TorrentDarkMode")) };
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
    sheet: bool,
    backdrop: HWND,
}

impl Dialog {
    fn create(owner: HWND, title: &str, width: i32, height: i32) -> Result<Self, String> {
        Self::open(owner, title, width, height, false)
    }

    fn open(
        owner: HWND,
        title: &str,
        width: i32,
        height: i32,
        sheet: bool,
    ) -> Result<Self, String> {
        let mut owner_rect = RECT::default();
        unsafe { GetWindowRect(owner, &mut owner_rect) };
        let x = owner_rect.left + (owner_rect.right - owner_rect.left - width) / 2;
        let y = if sheet {
            owner_rect.top
        } else {
            owner_rect.top + (owner_rect.bottom - owner_rect.top - height) / 2
        };
        let window = unsafe {
            CreateWindowExW(
                (if sheet { 0 } else { WS_EX_DLGMODALFRAME }) | WS_EX_CONTROLPARENT,
                wide("TorrentRustDevelopment").as_ptr(),
                wide(title).as_ptr(),
                WS_POPUP
                    | if sheet {
                        WS_CLIPCHILDREN
                    } else {
                        WS_CAPTION | WS_SYSMENU
                    },
                x,
                y,
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
        apply_title_theme(
            window,
            if window_dark(owner) {
                Theme::Dark
            } else {
                Theme::Light
            },
        );
        let backdrop = if sheet {
            unsafe {
                let backdrop = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_NOACTIVATE,
                    wide("TorrentRustDevelopment").as_ptr(),
                    wide("").as_ptr(),
                    WS_POPUP,
                    owner_rect.left,
                    owner_rect.top,
                    owner_rect.right - owner_rect.left,
                    owner_rect.bottom - owner_rect.top,
                    owner,
                    null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                );
                if !backdrop.is_null() {
                    SetPropW(backdrop, windows_sys::w!("TorrentSettingsBackdrop"), window);
                    SetLayeredWindowAttributes(backdrop, 0, 42, LWA_ALPHA);
                }
                backdrop
            }
        } else {
            null_mut()
        };
        Ok(Self {
            owner,
            window,
            sheet,
            backdrop,
        })
    }

    fn buttons(&self, y: i32, english: bool) -> Result<(), String> {
        self.control(
            "BUTTON",
            tr(english, "ОК", "OK"),
            WS_TABSTOP,
            (20, y, 100, 30),
            ACCEPT,
        )?;
        self.control(
            "BUTTON",
            tr(english, "Отмена", "Cancel"),
            WS_TABSTOP,
            (130, y, 100, 30),
            CANCEL,
        )?;
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
            if !self.backdrop.is_null() {
                ShowWindow(self.backdrop, SW_SHOWNOACTIVATE);
            }
            let mut client_animation = 0_i32;
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                (&mut client_animation as *mut i32).cast(),
                0,
            );
            if !self.sheet
                || client_animation == 0
                || AnimateWindow(self.window, 220, AW_SLIDE | AW_VER_POSITIVE | AW_ACTIVATE) == 0
            {
                ShowWindow(self.window, SW_SHOW);
            }
            let mut message = MSG::default();
            let mut deferred = Vec::new();
            while IsWindow(self.window) != 0 {
                match GetMessageW(&mut message, null_mut(), 0, 0) {
                    -1 => break,
                    0 => {
                        PostQuitMessage(message.wParam as i32);
                        break;
                    }
                    _ if message.hwnd == self.owner
                        && [REQUEST_CLOSE, SHELL_SOURCES, WM_DROPFILES]
                            .contains(&message.message) =>
                    {
                        deferred.push((message.message, message.wParam, message.lParam));
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
            for (message, wparam, lparam) in deferred {
                PostMessageW(self.owner, message, wparam, lparam);
            }
        }
    }
}

impl Drop for Dialog {
    fn drop(&mut self) {
        unsafe {
            EnableWindow(self.owner, 1);
            if IsWindow(self.window) != 0 {
                if self.sheet && IsWindowVisible(self.window) != 0 {
                    let mut motion = 0_i32;
                    SystemParametersInfoW(
                        SPI_GETCLIENTAREAANIMATION,
                        0,
                        (&mut motion as *mut i32).cast(),
                        0,
                    );
                    if motion != 0 {
                        AnimateWindow(self.window, 160, AW_SLIDE | AW_VER_NEGATIVE | AW_HIDE);
                    }
                }
                DestroyWindow(self.window);
            }
            if !self.backdrop.is_null() {
                RemovePropW(self.backdrop, windows_sys::w!("TorrentSettingsBackdrop"));
                DestroyWindow(self.backdrop);
            }
            SetActiveWindow(self.owner);
        }
    }
}

fn downloads_folder() -> Option<PathBuf> {
    let mut allocation = null_mut();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, 0, null_mut(), &mut allocation) };
    if result < 0 || allocation.is_null() {
        return None;
    }
    let mut count = 0;
    unsafe {
        while *allocation.add(count) != 0 {
            count += 1;
        }
        let units = std::slice::from_raw_parts(allocation, count);
        let path = PathBuf::from(
            <std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(units),
        );
        CoTaskMemFree(allocation.cast());
        Some(path)
    }
}

fn choose_folder(owner: HWND, en: bool) -> Option<PathBuf> {
    let title = wide(tr(en, "Папка загрузок", "Download folder"));
    let mut display_name = [0_u16; 260];
    let dialog = BROWSEINFOW {
        hwndOwner: owner,
        pszDisplayName: display_name.as_mut_ptr(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS,
        ..unsafe { zeroed() }
    };
    let result = unsafe { SHBrowseForFolderW(&dialog) };
    if result.is_null() {
        return None;
    }
    let mut buffer = [0_u16; 260];
    let status = unsafe { SHGetPathFromIDListW(result, buffer.as_mut_ptr()) };
    unsafe { CoTaskMemFree(result.cast()) };
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    (status != 0).then(|| {
        PathBuf::from(
            <std::ffi::OsString as std::os::windows::ffi::OsStringExt>::from_wide(&buffer[..end]),
        )
    })
}

fn open_url(owner: HWND, url: &str) -> bool {
    unsafe {
        ShellExecuteW(
            owner,
            windows_sys::w!("open"),
            wide(url).as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        ) as usize
            > 32
    }
}

fn settings(
    owner: HWND,
    prefs: &mut Preferences,
    root: &std::path::Path,
    controls: &Controls,
) -> Result<(), String> {
    let dialog = Dialog::open(
        owner,
        tr(prefs.english, "Настройки", "Settings"),
        720,
        560,
        true,
    )?;
    let mut fields = Vec::new();
    let label = |text, y, width| {
        dialog.control("STATIC", text, SS_CENTERIMAGE as u32, (28, y, width, 32), 0)
    };
    fields.push(label("", 12, 620)?);
    dialog.control("BUTTON", "×", WS_TABSTOP, (655, 12, 34, 32), ACCEPT)?;
    fields.push(label("", 62, 370)?);
    let system = dialog.control("BUTTON", "", WS_TABSTOP, (422, 62, 90, 32), THEME_SYSTEM)?;
    let light = dialog.control("BUTTON", "", WS_TABSTOP, (512, 62, 90, 32), THEME_LIGHT)?;
    let dark = dialog.control("BUTTON", "", WS_TABSTOP, (602, 62, 90, 32), THEME_DARK)?;
    fields.push(label("", 117, 370)?);
    let language = dialog.control(
        "COMBOBOX",
        "",
        WS_TABSTOP | CBS_DROPDOWNLIST as u32,
        (422, 117, 270, 120),
        CHANGE_LANGUAGE,
    )?;
    fields.push(label("", 172, 385)?);
    let folder = dialog.control(
        "EDIT",
        &prefs.folder.to_string_lossy(),
        WS_BORDER | ES_READONLY as u32 | ES_AUTOHSCROLL as u32,
        (337, 172, 245, 30),
        0,
    )?;
    let choose = dialog.control("BUTTON", "", WS_TABSTOP, (592, 172, 100, 32), CHOOSE_FOLDER)?;
    fields.push(label("", 227, 370)?);
    let add = dialog.control(
        "COMBOBOX",
        "",
        WS_TABSTOP | CBS_DROPDOWNLIST as u32,
        (422, 227, 270, 120),
        CHANGE_ADD,
    )?;
    fields.push(label("", 302, 620)?);
    label("Torrent 1.4.0 Rust Edition", 348, 620)?;
    label("Sander Stripa", 382, 620)?;
    let updates = dialog.control("BUTTON", "", WS_TABSTOP, (28, 433, 145, 32), ABOUT_UPDATE)?;
    dialog.control(
        "BUTTON",
        "by connect.",
        WS_TABSTOP,
        (302, 500, 116, 32),
        ABOUT_CONNECT,
    )?;
    unsafe {
        for field in fields.iter().copied() {
            SendMessageW(field, WM_SETFONT, controls.fonts.heading() as usize, 1);
        }
        for value in ["Русский", "English"] {
            SendMessageW(language, CB_ADDSTRING, 0, wide(value).as_ptr() as LPARAM);
        }
    }
    let display = |prefs: &Preferences| unsafe {
        let en = prefs.english;
        let headings = [
            tr(en, "Настройки", "Settings"),
            tr(en, "Тема", "Theme"),
            tr(en, "Язык интерфейса", "Interface language"),
            tr(en, "Папка загрузок", "Download folder"),
            tr(en, "При добавлении торрента", "On adding a torrent"),
            tr(en, "О Torrent", "About Torrent"),
        ];
        SetWindowTextW(dialog.window, wide(headings[0]).as_ptr());
        for (field, title) in fields.iter().zip(headings) {
            SetWindowTextW(*field, wide(title).as_ptr());
        }
        for (button, theme, title) in [
            (system, Theme::System, tr(en, "Системная", "System")),
            (light, Theme::Light, tr(en, "Светлая", "Light")),
            (dark, Theme::Dark, tr(en, "Тёмная", "Dark")),
        ] {
            let title = if prefs.theme == theme {
                format!("✓ {title}")
            } else {
                title.into()
            };
            SetWindowTextW(button, wide(title).as_ptr());
        }
        SetWindowTextW(choose, wide(tr(en, "Выбрать…", "Browse…")).as_ptr());
        SetWindowTextW(updates, wide(tr(en, "Обновление", "Updates")).as_ptr());
        SetWindowTextW(folder, wide(prefs.folder.as_os_str()).as_ptr());
        SendMessageW(language, CB_SETCURSEL, en as usize, 0);
        SendMessageW(add, CB_RESETCONTENT, 0, 0);
        for item in [
            tr(en, "Начать загрузку", "Start download"),
            tr(en, "Выбрать файлы", "Choose files"),
        ] {
            SendMessageW(add, CB_ADDSTRING, 0, wide(item).as_ptr() as LPARAM);
        }
        SendMessageW(add, CB_SETCURSEL, prefs.select_files as usize, 0);
    };
    display(prefs);
    dialog.run(|id| {
        let mut updated = prefs.clone();
        match id {
            ACCEPT | CANCEL => return true,
            THEME_SYSTEM => updated.theme = Theme::System,
            THEME_LIGHT => updated.theme = Theme::Light,
            THEME_DARK => updated.theme = Theme::Dark,
            CHANGE_LANGUAGE => {
                updated.english = unsafe { SendMessageW(language, CB_GETCURSEL, 0, 0) } == 1
            }
            CHANGE_ADD => {
                updated.select_files = unsafe { SendMessageW(add, CB_GETCURSEL, 0, 0) } == 1
            }
            CHOOSE_FOLDER => {
                let Some(path) = choose_folder(dialog.window, prefs.english) else {
                    return false;
                };
                updated.folder = path;
            }
            ABOUT_UPDATE | ABOUT_CONNECT => {
                let url = if id == ABOUT_UPDATE {
                    "https://github.com/sanderstripa/torrent/releases/latest"
                } else {
                    "https://connect.sanderstripa.com/?utm_source=chatgpt.com"
                };
                if !open_url(dialog.window, url) {
                    show_error(tr(
                        prefs.english,
                        "Не удалось открыть ссылку.",
                        "Could not open the link.",
                    ));
                }
                return false;
            }
            _ => return false,
        }
        if let Err(error) = std::fs::create_dir_all(root).and_then(|()| updated.save(root)) {
            show_error(&error.to_string());
        } else {
            *prefs = updated;
            controls.set_language(prefs.english);
            controls.dark.set(apply_title_theme(owner, prefs.theme));
            apply_title_theme(dialog.window, prefs.theme);
        }
        display(prefs);
        false
    });
    Ok(())
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
    en: bool,
) -> Result<Option<Vec<u8>>, String> {
    let files = transfer.handle.files()?;
    if files.is_empty() {
        return Err(tr(
            en,
            "Файлы станут доступны после получения метаданных.",
            "Files will become available when metadata arrives.",
        )
        .into());
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
    let dialog = Dialog::create(owner, tr(en, "Файлы торрента", "Torrent files"), 730, 510)?;
    let list = dialog.control(
        "LISTBOX",
        "",
        WS_BORDER | WS_VSCROLL | WS_HSCROLL | WS_TABSTOP | LBS_EXTENDEDSEL as u32,
        (20, 52, 670, 352),
        0,
    )?;
    dialog.control("BUTTON", "По имени", WS_TABSTOP, (20, 14, 110, 28), BY_NAME)?;
    dialog.control(
        "BUTTON",
        tr(en, "По размеру", "By size"),
        WS_TABSTOP,
        (140, 14, 110, 28),
        BY_SIZE,
    )?;
    for (id, label, x) in [
        (PRIORITY_LOW, tr(en, "Низкий", "Low"), 270),
        (PRIORITY_NORMAL, tr(en, "Обычный", "Normal"), 380),
        (PRIORITY_HIGH, tr(en, "Высокий", "High"), 490),
    ] {
        dialog.control("BUTTON", label, WS_TABSTOP, (x, 14, 100, 28), id)?;
    }
    dialog.buttons(418, en)?;
    let mut indices: Vec<_> = (0..files.len()).collect();
    let mut accepted = false;
    let mut remembered: Vec<u8> = priorities
        .iter()
        .map(|&p| if p == 0 { 4 } else { p })
        .collect();
    let mut sorting = (0, false);
    let read_selection = |indices: &[usize], priorities: &mut [u8], remembered: &[u8]| {
        for (row, &index) in indices.iter().enumerate() {
            let selected = unsafe { SendMessageW(list, LB_GETSEL, row, 0) } > 0;
            priorities[index] = if selected { remembered[index] } else { 0 };
        }
    };
    let redraw = |indices: &[usize], priorities: &[u8]| unsafe {
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        let mut extent = 0;
        for (row, &index) in indices.iter().enumerate() {
            let file = &files[index];
            let level = match priorities[index] {
                0 => tr(en, "Пропустить", "Skip"),
                1 => tr(en, "Низкий", "Low"),
                7 => tr(en, "Высокий", "High"),
                _ => tr(en, "Обычный", "Normal"),
            };
            let label = format!("{}  •  {} B  •  {level}", file.path, file.size);
            let units = wide(label);
            extent = extent.max(units.len().saturating_mul(12));
            SendMessageW(list, LB_ADDSTRING, 0, units.as_ptr() as LPARAM);
            SendMessageW(
                list,
                LB_SETSEL,
                (priorities[index] != 0) as usize,
                row as LPARAM,
            );
        }
        SendMessageW(list, LB_SETHORIZONTALEXTENT, extent, 0);
    };
    redraw(&indices, &priorities);
    dialog.run(|id| match id {
        ACCEPT => {
            read_selection(&indices, &mut priorities, &remembered);
            accepted = true;
            true
        }
        CANCEL => true,
        PRIORITY_LOW | PRIORITY_NORMAL | PRIORITY_HIGH => {
            read_selection(&indices, &mut priorities, &remembered);
            let row = unsafe { SendMessageW(list, LB_GETCARETINDEX, 0, 0) };
            if let Some(&index) = usize::try_from(row).ok().and_then(|row| indices.get(row)) {
                let priority = match id {
                    PRIORITY_LOW => 1,
                    PRIORITY_HIGH => 7,
                    _ => 4,
                };
                priorities[index] = priority;
                remembered[index] = priority;
                redraw(&indices, &priorities);
                unsafe { SendMessageW(list, LB_SETCARETINDEX, row as usize, 0) };
            }
            false
        }
        BY_NAME | BY_SIZE => {
            read_selection(&indices, &mut priorities, &remembered);
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
    window: HWND,
    english: std::cell::Cell<bool>,
    dark: std::cell::Cell<bool>,
    fonts: Box<crate::windows_paint::Fonts>,
    cards: std::cell::RefCell<Vec<crate::windows_paint::TransferCard>>,
    list: HWND,
}

impl Controls {
    fn create(window: HWND) -> Result<Self, String> {
        let child =
            |class, label, style, geometry, id| child(window, class, label, style, geometry, id);
        child(
            "STATIC",
            "TORRENT.",
            SS_CENTERIMAGE as u32,
            (24, 0, 170, 62),
            190,
        )?;
        for (id, label) in [
            (OPEN_ADD, "+"),
            (SETTINGS, "⚙"),
            (MINIMIZE, "−"),
            (CLOSE, "×"),
        ] {
            child(
                "BUTTON",
                label,
                WS_TABSTOP | BS_OWNERDRAW as u32,
                (0, 0, 32, 32),
                id,
            )?;
        }
        let list = child(
            "LISTBOX",
            "",
            WS_VSCROLL
                | WS_TABSTOP
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32
                | LBS_NOTIFY as u32,
            (16, 84, 870, 520),
            200,
        )?;
        let fonts = Box::new(crate::windows_paint::Fonts::new());
        unsafe {
            SendMessageW(
                GetDlgItem(window, 190),
                WM_SETFONT,
                fonts.heading() as usize,
                1,
            )
        };
        Ok(Self {
            window,
            english: std::cell::Cell::new(false),
            dark: std::cell::Cell::new(false),
            fonts,
            cards: std::cell::RefCell::new(Vec::new()),
            list,
        })
    }

    fn set_language(&self, english: bool) {
        self.english.set(english);
    }

    fn layout(&self) {
        let mut available = RECT::default();
        if unsafe { GetClientRect(self.window, &mut available) } == 0 {
            return;
        }
        for (index, id) in [CLOSE, MINIMIZE, SETTINGS, OPEN_ADD]
            .into_iter()
            .enumerate()
        {
            let button = unsafe { GetDlgItem(self.window, id as i32) };
            let x = available.right - 46 - index as i32 * 40;
            unsafe { MoveWindow(button, x, 16, 32, 32, 1) };
        }
        unsafe {
            MoveWindow(
                self.list,
                16,
                84,
                (available.right - 32).max(1),
                (available.bottom - 100).max(1),
                1,
            )
        };
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
        let en = self.english.get();
        let mut cards = self.cards.borrow_mut();
        cards.clear();
        cards.reserve(transfers.ordered().len());
        for entry in transfers.ordered() {
            let mut card = crate::windows_paint::TransferCard::new();
            card.attach_fonts(&self.fonts);
            card.dark = self.dark.get();
            match entry.handle.status() {
                Ok((name, status)) => {
                    card.title = name;
                    card.paused = status.paused || entry.stopped;
                    let state = if entry.selection_pending {
                        tr(en, "Выбор файлов", "Choose files")
                    } else if entry.stopped {
                        tr(en, "Остановлено", "Stopped")
                    } else if status.paused {
                        tr(en, "Пауза", "Paused")
                    } else if !status.has_metadata {
                        tr(en, "Получение списка файлов", "Fetching metadata")
                    } else if status.seeding {
                        tr(en, "Завершено · Раздача", "Complete · Seeding")
                    } else {
                        tr(en, "Загрузка", "Downloading")
                    };
                    let size = status.wanted as f64 / (1024.0 * 1024.0);
                    let size = format!("{size:.1}");
                    let size = if en { size } else { size.replace('.', ",") };
                    card.details = format!("{size} {} · {state}", tr(en, "МБ", "MB"));
                    if !entry.selection_pending {
                        card.progress = status.progress_ppm;
                        card.progress_label = if status.seeding {
                            "✓".into()
                        } else {
                            format!("{}%", status.progress_ppm / 10_000)
                        };
                        if status.seeding {
                            card.footer = "100%".into();
                        } else if card.paused {
                            card.footer = state.into();
                        } else if status.download_rate > 0 {
                            let seconds = (status.wanted - status.wanted_done).max(0)
                                / i64::from(status.download_rate);
                            card.footer = format!(
                                "{} {} {} {} {}",
                                tr(en, "Осталось", "Remaining"),
                                seconds / 60,
                                tr(en, "мин", "min"),
                                seconds % 60,
                                tr(en, "сек", "sec")
                            );
                        }
                    }
                }
                Err(error) => {
                    card.title = entry.key.clone();
                    card.details = error;
                }
            };
            cards.push(card);
        }
        for card in cards.iter() {
            unsafe {
                let row = SendMessageW(
                    self.list,
                    LB_ADDSTRING,
                    0,
                    wide(&card.title).as_ptr() as LPARAM,
                );
                if row >= 0 {
                    SendMessageW(
                        self.list,
                        LB_SETITEMDATA,
                        row as usize,
                        card as *const _ as LPARAM,
                    );
                }
            }
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
}

fn magnet_input(owner: HWND, en: bool) -> Result<Option<String>, String> {
    let dialog = Dialog::create(owner, "Magnet", 580, 150)?;
    let edit = dialog.control(
        "EDIT",
        "",
        WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
        (20, 18, 520, 28),
        0,
    )?;
    dialog.buttons(62, en)?;
    let mut source = None;
    dialog.run(|id| match id {
        CANCEL => true,
        ACCEPT => {
            let value = edit_text(edit).trim().to_owned();
            if !value.starts_with("magnet:") {
                show_error(tr(en, "Введите magnet-ссылку", "Enter a magnet link"));
                return false;
            }
            source = Some(value);
            true
        }
        _ => false,
    });
    Ok(source)
}

fn menu(owner: HWND, x: i32, y: i32, entries: &[(usize, &str)]) -> Option<usize> {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return None;
        }
        for &(id, label) in entries {
            AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
        }
        let action = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            x,
            y,
            0,
            owner,
            null(),
        );
        DestroyMenu(menu);
        (action != 0).then_some(action as usize)
    }
}

fn add_menu_entries(en: bool) -> [(usize, &'static str); 2] {
    [
        (ADD_TORRENT, tr(en, "Открыть .torrent", "Open .torrent")),
        (ADD_MAGNET, tr(en, "Добавить magnet", "Add magnet")),
    ]
}

fn context_menu(window: HWND, controls: &Controls, location: LPARAM) {
    let en = controls.english.get();
    let (x, y) = if location == -1 {
        let mut area = RECT::default();
        unsafe { GetWindowRect(controls.list, &mut area) };
        (area.left + 12, area.top + 12)
    } else {
        (location as i16 as i32, (location >> 16) as i16 as i32)
    };
    let selected = if location == -1 {
        controls.selected()
    } else {
        let mut point = windows_sys::Win32::Foundation::POINT { x, y };
        unsafe { windows_sys::Win32::Graphics::Gdi::ScreenToClient(controls.list, &mut point) };
        let mut area = RECT::default();
        unsafe { GetClientRect(controls.list, &mut area) };
        if point.x < 0 || point.y < 0 || point.x >= area.right || point.y >= area.bottom {
            None
        } else {
            let packed = (i64::from(point.y) << 16) | i64::from(point.x & 0xffff);
            let result =
                unsafe { SendMessageW(controls.list, LB_ITEMFROMPOINT, 0, packed as LPARAM) };
            if result >> 16 == 0 {
                let row = (result & 0xffff) as usize;
                let count = unsafe { SendMessageW(controls.list, LB_GETCOUNT, 0, 0) };
                (row < count.max(0) as usize).then_some(row)
            } else {
                None
            }
        }
    };
    let action = if let Some(row) = selected {
        unsafe { SendMessageW(controls.list, LB_SETCURSEL, row, 0) };
        menu(
            window,
            x,
            y,
            &[
                (PAUSE, tr(en, "Пауза", "Pause")),
                (RESUME, tr(en, "Возобновить", "Resume")),
                (STOP, tr(en, "Остановить", "Stop")),
                (REMOVE, tr(en, "Удалить из списка", "Remove from list")),
                (UP, tr(en, "Приоритет выше", "Higher priority")),
                (DOWN, tr(en, "Приоритет ниже", "Lower priority")),
                (FILES, tr(en, "Файлы", "Files")),
            ],
        )
    } else {
        let [open, magnet] = add_menu_entries(en);
        menu(
            window,
            x,
            y,
            &[open, magnet, (SETTINGS, tr(en, "Настройки", "Settings"))],
        )
    };
    if let Some(action) = action {
        unsafe { PostMessageW(window, ACTION, action, 0) };
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
    en: bool,
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
        if let Some(priorities) = select_torrent_files(owner, entry, en)? {
            queue.select_files(index, &priorities)?;
        }
    }
    Ok(())
}

fn window_dark(window: HWND) -> bool {
    unsafe { !GetPropW(window, windows_sys::w!("TorrentDarkMode")).is_null() }
}

fn apply_title_theme(window: HWND, theme: Theme) -> bool {
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
        let property = windows_sys::w!("TorrentDarkMode");
        if dark != 0 {
            SetPropW(window, property, 1usize as HANDLE);
        } else {
            RemovePropW(window, property);
        }
        windows_sys::Win32::Graphics::Gdi::InvalidateRect(window, null(), 1);
        DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&dark as *const i32).cast(),
            std::mem::size_of_val(&dark) as u32,
        );
    }
    dark != 0
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

fn forward_sources() {
    let existing = unsafe { FindWindowW(CLASS, null()) };
    if existing.is_null() {
        return;
    }
    for source in std::env::args_os().skip(1) {
        let mut data = wide(source);
        let bytes = data.len().saturating_mul(2);
        if bytes > 65_536 {
            continue;
        }
        let packet = COPYDATASTRUCT {
            dwData: 1,
            cbData: bytes as u32,
            lpData: data.as_mut_ptr().cast(),
        };
        unsafe {
            SendMessageTimeoutW(
                existing,
                WM_COPYDATA,
                0,
                (&packet as *const COPYDATASTRUCT) as LPARAM,
                SMTO_ABORTIFHUNG,
                2000,
                null_mut(),
            );
        }
    }
    unsafe {
        ShowWindow(existing, SW_RESTORE);
        SetForegroundWindow(existing);
    }
}

pub fn run() -> Result<(), String> {
    let smoke = std::env::args_os().any(|argument| argument == "--smoke-test");
    let interaction = std::env::args_os().any(|argument| argument == "--interaction-test");
    let _lock = if smoke || interaction {
        None
    } else {
        let name = wide("Local\\Torrent.RustEdition.Development");
        let mutex = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        if mutex.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let lock = ProcessLock(mutex);
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            forward_sources();
            return Ok(());
        }
        Some(lock)
    };
    // The development shell uses a separate profile until migration is validated.
    let profile = if smoke {
        std::env::temp_dir().join(format!("torrent-re-smoke-{}", std::process::id()))
    } else if interaction {
        std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join("re-ui-test-data")
    } else {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?)
            .join("Torrent RE Development")
    };
    if !smoke && !interaction && !profile.exists() {
        if let Some(local) = profile.parent() {
            let stable = local.join("Torrent");
            let settings = stable.join("settings.txt");
            let position = stable.join("window-position.txt");
            if settings.is_file() || position.is_file() {
                std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
                for item in [settings, position] {
                    if item.is_file() {
                        let name = item.file_name().ok_or("Invalid legacy profile path")?;
                        std::fs::copy(&item, profile.join(name))
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
    }
    let default_folder = downloads_folder().unwrap_or_else(|| profile.join("Downloads"));
    let mut prefs =
        Preferences::load(&profile, default_folder).map_err(|error| error.to_string())?;
    let session = Session::open(PREVIEW_POLICY, smoke || interaction)?;
    let (mut transfers, warnings) = Transfers::open(&session, &profile.join("session"))?;
    if !warnings.is_empty() && !smoke {
        show_error(&warnings.join("\n"));
    }
    unsafe {
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let class = CLASS;
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(dispatch),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW as usize + 1) as *mut c_void,
            lpszClassName: class,
            ..zeroed()
        };
        if RegisterClassW(&window_class) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let title = wide("Torrent Rust Edition");
        let window = CreateWindowExW(
            0,
            class,
            title.as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
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
        let dark = apply_title_theme(window, prefs.theme);
        if !smoke {
            restore_position(window, &profile);
        }
        let controls = Controls::create(window)?;
        controls.set_language(prefs.english);
        controls.dark.set(dark);
        controls.layout();
        let mut prompted = std::collections::HashSet::new();
        let mut last_save = std::time::Instant::now();
        if !smoke {
            for source in std::env::args_os()
                .skip(1)
                .filter(|arg| arg != "--interaction-test")
            {
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
                _ if message.hwnd == window && message.message == LAYOUT => controls.layout(),
                _ if message.hwnd == window && message.message == WINDOW_MOVED && !smoke => {
                    SetTimer(window, 2, 250, None);
                }
                _ if message.hwnd == window
                    && message.message == WM_TIMER
                    && message.wParam == 2 =>
                {
                    KillTimer(window, 2);
                    let _ = save_position(window, &profile);
                }
                _ if message.hwnd == window && message.message == CONTEXT => {
                    context_menu(window, &controls, message.lParam)
                }
                _ if message.hwnd == window && message.message == REQUEST_CLOSE => {
                    if !smoke {
                        if let Err(error) = save_position(window, &profile) {
                            show_error(&error.to_string());
                        }
                    }
                    DestroyWindow(window);
                }
                _ if message.hwnd == window && message.message == APPLY_THEME => {
                    controls.dark.set(apply_title_theme(window, prefs.theme));
                    controls.refresh(&transfers);
                    DispatchMessageW(&message);
                }
                _ if message.hwnd == window && message.message == WM_TIMER => {
                    if let Err(error) =
                        prompt_for_files(window, &mut transfers, &mut prompted, prefs.english)
                    {
                        show_error(&error);
                    }
                    if last_save.elapsed().as_secs() >= 30 && !transfers.ordered().is_empty() {
                        last_save = std::time::Instant::now();
                        if let Err(error) = transfers.save() {
                            eprintln!("{error}");
                        }
                    }
                    controls.refresh(&transfers)
                }
                _ if message.hwnd == window && message.message == SHELL_SOURCES => {
                    let sources =
                        SHELL_INBOX.with(|inbox| std::mem::take(&mut *inbox.borrow_mut()));
                    for source in sources {
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
                        OPEN_ADD => {
                            let mut anchor = RECT::default();
                            GetWindowRect(GetDlgItem(window, OPEN_ADD as i32), &mut anchor);
                            if let Some(action) = menu(
                                window,
                                anchor.left,
                                anchor.bottom,
                                &add_menu_entries(prefs.english),
                            ) {
                                PostMessageW(window, ACTION, action, 0);
                            }
                            Ok(())
                        }
                        MINIMIZE => {
                            ShowWindow(window, SW_MINIMIZE);
                            Ok(())
                        }
                        CLOSE => {
                            PostMessageW(window, WM_CLOSE, 0, 0);
                            Ok(())
                        }
                        ADD_MAGNET => magnet_input(window, prefs.english).and_then(|source| {
                            source.map_or(Ok(()), |source| {
                                transfers
                                    .add(&source, &destination, prefs.select_files)
                                    .map(|_| ())
                            })
                        }),
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
                            if let Some(priorities) = select_torrent_files(
                                window,
                                &transfers.ordered()[row],
                                prefs.english,
                            )? {
                                transfers.select_files(row, &priorities)?;
                            }
                            Ok(())
                        }),
                        SETTINGS => settings(window, &mut prefs, &profile, &controls),
                        _ => Ok(()),
                    };
                    match result {
                        Err(error) => show_error(&error),
                        Ok(())
                            if [
                                ADD_MAGNET,
                                ADD_TORRENT,
                                PAUSE,
                                RESUME,
                                STOP,
                                REMOVE,
                                UP,
                                DOWN,
                                FILES,
                            ]
                            .contains(&message.wParam) =>
                        {
                            if let Err(error) = transfers.save() {
                                show_error(&error);
                            }
                            last_save = std::time::Instant::now();
                        }
                        _ => {}
                    }
                    controls.refresh_selection(&transfers, selected);
                }
                _ if message.hwnd == controls.list
                    && message.message == WM_KEYDOWN
                    && [VK_DELETE as usize, VK_RETURN as usize].contains(&message.wParam) =>
                {
                    PostMessageW(
                        window,
                        ACTION,
                        if message.wParam == VK_DELETE as usize {
                            REMOVE
                        } else {
                            FILES
                        },
                        0,
                    );
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
