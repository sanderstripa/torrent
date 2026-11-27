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
use windows_sys::Win32::System::SystemServices::{SS_CENTER, SS_CENTERIMAGE};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, IsWindowEnabled, SetActiveWindow, VK_DELETE, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, FOLDERID_Downloads, SHBrowseForFolderW,
    SHGetKnownFolderPath, SHGetPathFromIDListW, ShellExecuteW, Shell_NotifyIconW,
    BIF_RETURNONLYFSDIRS, BROWSEINFOW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub(crate) const ACTION: u32 = WM_APP + 1;
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
const TOGGLE_TRAY: usize = 129;
const VPN_COMMAND: usize = 130;
const CONTEXT: u32 = WM_APP + 2;
const LAYOUT: u32 = WM_APP + 3;
const REQUEST_CLOSE: u32 = WM_APP + 4;
const APPLY_THEME: u32 = WM_APP + 5;
const SHELL_SOURCES: u32 = WM_APP + 6;
const WINDOW_MOVED: u32 = WM_APP + 7;
const TRAY_EVENT: u32 = WM_APP + 10;
const REOPEN: u32 = WM_APP + 11;
const EXIT: u32 = WM_APP + 12;
const CLASS: windows_sys::core::PCWSTR = windows_sys::w!("TorrentRustDevelopment");
thread_local! {
    static CARD_ROWS: std::cell::RefCell<Vec<crate::windows_paint::TransferCard>> = const { std::cell::RefCell::new(Vec::new()) };
    static SHELL_INBOX: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn tr<'a>(english: bool, ru: &'a str, en: &'a str) -> &'a str {
    if english {
        en
    } else {
        ru
    }
}

static TRACE_UI: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn trace_ui(message: &str) {
    if TRACE_UI.load(std::sync::atomic::Ordering::Relaxed) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("re-ui-trace.txt")
        {
            let _ = writeln!(file, "{message}");
        }
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
    #[cfg(all(feature = "rqbit", feature = "vpn"))]
    if let Some(result) =
        unsafe { crate::windows_settings::dispatch(window, message, wparam, lparam) }
    {
        return result;
    }
    #[cfg(feature = "rqbit")]
    if let Some(result) = unsafe { crate::windows_files::dispatch(window, message, wparam, lparam) }
    {
        return result;
    }
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
                let image = GetPropW(window, windows_sys::w!("TorrentSettingsSnapshot"));
                if let Some(snapshot) = image
                    .cast::<crate::windows_paint::SettingsBackdrop>()
                    .as_ref()
                {
                    snapshot.paint(wparam as _, area);
                } else {
                    windows_sys::Win32::Graphics::Gdi::FillRect(
                        wparam as _,
                        &area,
                        windows_sys::Win32::Graphics::Gdi::GetStockObject(
                            windows_sys::Win32::Graphics::Gdi::BLACK_BRUSH,
                        ),
                    );
                }
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
        #[cfg(feature = "rqbit")]
        WM_DRAWITEM if wparam == 190 => {
            let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            let _ = crate::windows_render::title(draw, window_dark(window));
            1
        }
        #[cfg(feature = "rqbit")]
        WM_DRAWITEM if [OPEN_ADD, MINIMIZE, CLOSE, VPN_COMMAND].contains(&wparam) => {
            use crate::windows_render::Icon;
            let draw = unsafe { &*(lparam as *const DRAWITEMSTRUCT) };
            let icon = match wparam {
                OPEN_ADD => Icon::Add,
                MINIMIZE => Icon::Minimize,
                VPN_COMMAND => Icon::Vpn(unsafe {
                    GetPropW(window, windows_sys::w!("TorrentVpnVisual")) as u8
                }),
                _ => Icon::Close,
            };
            let active = draw.itemState & (0x0001 | 0x0040) != 0;
            if crate::windows_render::draw(draw.hDC, draw.rcItem, window_dark(window), active, icon)
                .is_err()
            {
                unsafe {
                    crate::windows_paint::control_background(draw.hDC, window_dark(window));
                }
            }
            1
        }
        #[cfg(not(feature = "rqbit"))]
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
            trace_ui("queue draw begin");
            let Some(draw) = (unsafe { (lparam as *const DRAWITEMSTRUCT).as_ref() }) else {
                return 0;
            };
            CARD_ROWS.with(|cards| {
                if let Ok(cards) = cards.try_borrow() {
                    if let Some(row) = cards.get(draw.itemID as usize) {
                        #[cfg(feature = "rqbit")]
                        if crate::windows_render::card(draw, row).is_ok() {
                            return;
                        }
                        unsafe { crate::windows_paint::transfer(draw, row) };
                    }
                }
            });
            1
        }
        WM_COMMAND
            if (wparam >> 16 == 0
                && ((ADD_MAGNET..=THEME_DARK).contains(&(wparam & 0xffff))
                    || (wparam & 0xffff) == TOGGLE_TRAY
                    || cfg!(all(feature = "rqbit", feature = "vpn"))
                        && (wparam & 0xffff) == VPN_COMMAND))
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
    _snapshot: Option<Box<crate::windows_paint::SettingsBackdrop>>,
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
        let snapshot = if sheet {
            crate::windows_paint::SettingsBackdrop::capture(
                owner,
                owner_rect.right - owner_rect.left,
                owner_rect.bottom - owner_rect.top,
            )
            .map(Box::new)
        } else {
            None
        };
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
                    if let Some(image) = snapshot.as_deref() {
                        SetPropW(
                            backdrop,
                            windows_sys::w!("TorrentSettingsSnapshot"),
                            (image as *const crate::windows_paint::SettingsBackdrop)
                                .cast_mut()
                                .cast(),
                        );
                    }
                    SetLayeredWindowAttributes(
                        backdrop,
                        0,
                        if snapshot.is_some() { 255 } else { 42 },
                        LWA_ALPHA,
                    );
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
            _snapshot: snapshot,
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
            if self.sheet {
                trace_ui("settings animation completed");
            }
            #[cfg(all(feature = "rqbit", feature = "vpn"))]
            if !GetPropW(self.window, crate::windows_settings::PROPERTY).is_null() {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.window);
            }
            #[cfg(feature = "rqbit")]
            if !GetPropW(self.window, crate::windows_files::PROPERTY).is_null() {
                windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus(self.window);
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
                        && [REQUEST_CLOSE, REOPEN, EXIT, SHELL_SOURCES, WM_DROPFILES]
                            .contains(&message.message) =>
                    {
                        deferred.push((message.message, message.wParam, message.lParam));
                    }
                    #[cfg(all(feature = "rqbit", feature = "vpn"))]
                    _ if message.hwnd == self.owner
                        && message.message == WM_TIMER
                        && message.wParam == 1
                        && !GetPropW(self.window, crate::windows_settings::PROPERTY).is_null() =>
                    {
                        command(crate::windows_settings::TICK);
                    }
                    _ if message.hwnd == self.window && message.message == ACTION => {
                        if command(message.wParam) {
                            break;
                        }
                    }
                    _ if message.message == WM_KEYDOWN && message.wParam == VK_ESCAPE as usize => {
                        break
                    }
                    #[cfg(all(feature = "rqbit", feature = "vpn"))]
                    _ if message.hwnd == self.window
                        && message.message == WM_KEYDOWN
                        && !GetPropW(self.window, crate::windows_settings::PROPERTY).is_null() =>
                    {
                        DispatchMessageW(&message);
                    }
                    #[cfg(feature = "rqbit")]
                    _ if message.hwnd == self.window
                        && message.message == WM_KEYDOWN
                        && !GetPropW(self.window, crate::windows_files::PROPERTY).is_null() =>
                    {
                        DispatchMessageW(&message);
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
                RemovePropW(self.backdrop, windows_sys::w!("TorrentSettingsSnapshot"));
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

#[cfg(not(all(feature = "rqbit", feature = "vpn")))]
fn settings(
    owner: HWND,
    prefs: &mut Preferences,
    root: &std::path::Path,
    controls: &Controls,
) -> Result<(), String> {
    trace_ui("settings opening");
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
    let tray_label = label("", 277, 370)?;
    let tray = dialog.control("BUTTON", "", WS_TABSTOP, (580, 277, 112, 32), TOGGLE_TRAY)?;
    fields.push(label("", 318, 620)?);

    label("Torrent 1.4.0 Rust Edition Preview 2", 348, 620)?;
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
        SetWindowTextW(
            tray_label,
            wide(tr(
                en,
                "Скрывать в трей при закрытии",
                "Hide to tray on close",
            ))
            .as_ptr(),
        );
        let tray_state = if prefs.close_to_tray {
            tr(en, "Включено", "On")
        } else {
            tr(en, "Выключено", "Off")
        };
        SetWindowTextW(tray, wide(tray_state).as_ptr());
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
    trace_ui("settings ready");
    dialog.run(|id| {
        let mut updated = prefs.clone();
        match id {
            ACCEPT | CANCEL => return true,
            THEME_SYSTEM => updated.theme = Theme::System,
            THEME_LIGHT => updated.theme = Theme::Light,
            THEME_DARK => updated.theme = Theme::Dark,
            TOGGLE_TRAY => updated.close_to_tray = !prefs.close_to_tray,
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

pub(crate) fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
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

#[cfg(not(feature = "rqbit"))]
fn select_torrent_files(
    owner: HWND,
    transfer: &torrent_re_sort::transfers::Transfer<'_>,
    en: bool,
) -> Result<Option<Vec<u8>>, String> {
    trace_ui("file dialog constructing");
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
    trace_ui("file dialog ready");
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

#[cfg(feature = "rqbit")]
fn select_torrent_files(
    owner: HWND,
    transfer: &torrent_re_sort::transfers::Transfer<'_>,
    english: bool,
) -> Result<Option<Vec<u8>>, String> {
    use crate::windows_files as files_ui;
    let files = transfer.handle.files()?;
    if files.is_empty() {
        return Err(tr(
            english,
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
    let (title, _) = transfer.handle.status()?;
    let view = std::cell::RefCell::new(files_ui::FileView::new(
        files,
        priorities,
        title,
        english,
        window_dark(owner),
    ));
    let (width, height) = view.borrow().size();
    let dialog = Dialog::create(
        owner,
        tr(english, "Файлы торрента", "Torrent files"),
        width,
        height,
    )?;
    unsafe {
        SetWindowLongPtrW(dialog.window, GWL_STYLE, (WS_POPUP | WS_SYSMENU) as isize);
        if SetPropW(
            dialog.window,
            files_ui::PROPERTY,
            (&view as *const _ as *mut std::ffi::c_void).cast(),
        ) == 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        SetWindowPos(
            dialog.window,
            null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
        );
    }
    let mut accepted = false;
    dialog.run(|id| match id {
        files_ui::APPLY => {
            accepted = true;
            true
        }
        files_ui::CANCEL => true,
        id if id >= files_ui::PRIORITY => {
            let index = id - files_ui::PRIORITY;
            let (position, selected) = {
                let view = view.borrow();
                let Some(&current) = view.priorities.get(index) else {
                    return false;
                };
                (view.menu_point(index), current)
            };
            let mut point = windows_sys::Win32::Foundation::POINT {
                x: position.0,
                y: position.1,
            };
            unsafe {
                windows_sys::Win32::Graphics::Gdi::ClientToScreen(dialog.window, &mut point);
            }
            let options = [0u8, 1, 4, 7].map(|priority| crate::windows_popup::Item {
                id: usize::from(priority) + 1,
                title: files_ui::priority_label(priority, english),
                checked: files_ui::priority_label(priority, english)
                    == files_ui::priority_label(selected, english),
                enabled: true,
            });
            if let Some(action) = crate::windows_popup::show(
                dialog.window,
                point.x,
                point.y,
                &options,
                window_dark(owner),
            ) {
                view.borrow_mut().set_priority(index, (action - 1) as u8);
                unsafe {
                    windows_sys::Win32::Graphics::Gdi::InvalidateRect(dialog.window, null(), 0);
                }
            }
            false
        }
        _ => false,
    });
    unsafe {
        RemovePropW(dialog.window, files_ui::PROPERTY);
    }
    Ok(accepted.then(|| view.into_inner().priorities))
}

struct Controls {
    window: HWND,
    english: std::cell::Cell<bool>,
    dark: std::cell::Cell<bool>,
    fonts: Box<crate::windows_paint::Fonts>,
    empty: HWND,
    empty_hint: HWND,
    list: HWND,
}

impl Controls {
    fn create(window: HWND) -> Result<Self, String> {
        let child =
            |class, label, style, geometry, id| child(window, class, label, style, geometry, id);
        child(
            "STATIC",
            "TORRENT.",
            if cfg!(feature = "rqbit") {
                0x000d
            } else {
                SS_CENTERIMAGE as u32
            },
            (24, 0, 170, 62),
            190,
        )?;
        for (id, label) in [(OPEN_ADD, "+"), (MINIMIZE, "−"), (CLOSE, "×")] {
            child(
                "BUTTON",
                label,
                WS_TABSTOP | BS_OWNERDRAW as u32,
                (0, 0, 32, 32),
                id,
            )?;
        }
        #[cfg(not(feature = "rqbit"))]
        child(
            "BUTTON",
            "⚙",
            WS_TABSTOP | BS_OWNERDRAW as u32,
            (0, 0, 32, 32),
            SETTINGS,
        )?;
        #[cfg(all(feature = "rqbit", feature = "vpn"))]
        child(
            "BUTTON",
            "VPN",
            WS_TABSTOP | BS_OWNERDRAW as u32,
            (0, 0, 32, 32),
            VPN_COMMAND,
        )?;
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
        let empty = child(
            "STATIC",
            "",
            SS_CENTERIMAGE as u32 | SS_CENTER as u32,
            (20, 245, 850, 30),
            0,
        )?;
        let empty_hint = child(
            "STATIC",
            "",
            SS_CENTERIMAGE as u32 | SS_CENTER as u32,
            (20, 280, 850, 30),
            0,
        )?;
        let fonts = Box::new(crate::windows_paint::Fonts::new());
        unsafe {
            SendMessageW(empty, WM_SETFONT, fonts.heading() as usize, 1);
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
            empty,
            empty_hint,
            list,
        })
    }

    fn set_language(&self, english: bool) {
        self.english.set(english);
        unsafe {
            SetWindowTextW(
                self.empty,
                wide(tr(
                    english,
                    "Добавьте первый торрент",
                    "Add your first torrent",
                ))
                .as_ptr(),
            );
            SetWindowTextW(
                self.empty_hint,
                wide(tr(
                    english,
                    "Нажмите + или перетащите сюда .torrent",
                    "Click + or drop a .torrent file here",
                ))
                .as_ptr(),
            );
        }
    }

    fn layout(&self) {
        let mut available = RECT::default();
        if unsafe { GetClientRect(self.window, &mut available) } == 0 {
            return;
        }
        #[cfg(not(feature = "rqbit"))]
        let button_order = [CLOSE, MINIMIZE, SETTINGS, OPEN_ADD];
        #[cfg(feature = "rqbit")]
        let button_order = [CLOSE, MINIMIZE, OPEN_ADD, VPN_COMMAND];
        for (index, id) in button_order.into_iter().enumerate() {
            let button = unsafe { GetDlgItem(self.window, id as i32) };
            if !button.is_null() {
                let x = available.right - 46 - index as i32 * 40;
                unsafe { MoveWindow(button, x, 16, 32, 32, 1) };
            }
        }
        unsafe {
            MoveWindow(
                self.empty,
                16,
                available.bottom / 2 - 32,
                (available.right - 32).max(1),
                30,
                1,
            );
            MoveWindow(
                self.empty_hint,
                16,
                available.bottom / 2,
                (available.right - 32).max(1),
                30,
                1,
            );
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
        let is_empty = transfers.ordered().is_empty();
        unsafe {
            ShowWindow(self.list, if is_empty { SW_HIDE } else { SW_SHOWNA });
            ShowWindow(self.empty, if is_empty { SW_SHOWNA } else { SW_HIDE });
            ShowWindow(self.empty_hint, if is_empty { SW_SHOWNA } else { SW_HIDE });
        }
        let top = unsafe { SendMessageW(self.list, LB_GETTOPINDEX, 0, 0) }.max(0);
        unsafe { SendMessageW(self.list, WM_SETREDRAW, 0, 0) };
        unsafe { SendMessageW(self.list, LB_RESETCONTENT, 0, 0) };
        let en = self.english.get();
        let mut cards = Vec::with_capacity(transfers.ordered().len());
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
                    card.details = format!("{} · {state}", formatted_size(status.wanted, en));
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
        CARD_ROWS.with(|rows| *rows.borrow_mut() = cards);
        for _ in transfers.ordered() {
            unsafe {
                SendMessageW(
                    self.list,
                    LB_ADDSTRING,
                    0,
                    windows_sys::w!("").cast::<u16>() as LPARAM,
                );
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

pub(crate) fn formatted_size(bytes: i64, en: bool) -> String {
    let (divisor, unit) = if bytes >= 1024 * 1024 * 1024 {
        (1024 * 1024 * 1024, tr(en, "ГБ", "GB"))
    } else if bytes >= 1024 * 1024 {
        (1024 * 1024, tr(en, "МБ", "MB"))
    } else {
        (1024, tr(en, "КБ", "KB"))
    };
    let value = format!("{:.1}", bytes as f64 / f64::from(divisor));
    let value = if en { value } else { value.replace('.', ",") };
    format!("{value} {unit}")
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

#[cfg(all(feature = "rqbit", feature = "vpn"))]
fn paint_vpn_title(window: HWND, vpn: &crate::windows_vpn::Vpn, enabled: bool) {
    unsafe {
        let property = windows_sys::w!("TorrentVpnVisual");
        let state = vpn.state(enabled);
        if GetPropW(window, property) as u8 != state {
            if state == 0 {
                RemovePropW(window, property);
            } else {
                SetPropW(window, property, state as HANDLE);
            }
            let button = GetDlgItem(window, VPN_COMMAND as i32);
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(button, null(), 0);
        }
    }
}

#[cfg(all(feature = "rqbit", feature = "vpn"))]
fn vpn_key_input(owner: HWND, current: &str, en: bool) -> Result<Option<String>, String> {
    let dialog = Dialog::create(
        owner,
        tr(en, "Ключ подключения", "Connection key"),
        570,
        150,
    )?;
    let edit = dialog.control(
        "EDIT",
        current,
        WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
        (20, 18, 520, 28),
        0,
    )?;
    dialog.buttons(62, en)?;
    let mut selected = None;
    dialog.run(|id| match id {
        ACCEPT => {
            selected = Some(edit_text(edit).trim().to_owned());
            true
        }
        CANCEL => true,
        _ => false,
    });
    Ok(selected)
}

#[cfg(all(feature = "rqbit", feature = "vpn"))]
fn settings_144(
    owner: HWND,
    prefs: &mut Preferences,
    profile: &std::path::Path,
    controls: &Controls,
    vpn: &mut crate::windows_vpn::Vpn,
    queue: &Transfers<'_>,
    initial_page: usize,
) -> Result<(), String> {
    use crate::windows_settings as page;
    let mut available = RECT::default();
    unsafe {
        GetClientRect(owner, &mut available);
    }
    let width = (available.right - 96).min(720).max(520);
    let dialog = Dialog::open(
        owner,
        tr(prefs.english, "Настройки", "Settings"),
        width,
        if initial_page == 0 { 321 } else { 370 },
        true,
    )?;
    let mut key = vpn.key()?;
    let view = std::cell::RefCell::new(page::SettingsView {
        preferences: prefs.clone(),
        page: initial_page,
        width: width as f32,
        dark: window_dark(owner),
        hover: None,
        focus: None,
        sudoku: key.starts_with("sudoku://"),
        key_present: !key.is_empty(),
        connected: vpn.state(prefs.vpn_enabled) == 2,
        save_failed: false,
    });
    if unsafe {
        SetPropW(
            dialog.window,
            page::PROPERTY,
            (&view as *const _ as *mut std::ffi::c_void).cast(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().to_string());
    }

    dialog.run(|id| {
        let previous = prefs.clone();
        let en = prefs.english;
        let mut result = Ok(());
        let mut persist = true;
        match id {
            page::CLOSE | CANCEL => return true,
            id if (page::NAV..=page::NAV + 4).contains(&id) => {
                let mut state = view.borrow_mut();
                state.page = id - page::NAV;
                state.hover = None;
                if state.focus.is_some() {
                    state.focus = Some(if state.page == 0 { page::CLOSE } else { id });
                }
                unsafe {
                    SetWindowPos(
                        dialog.window,
                        null_mut(),
                        0,
                        0,
                        width,
                        state.height() as i32,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                persist = false;
            }
            page::SYSTEM => prefs.theme = Theme::System,
            page::LIGHT => prefs.theme = Theme::Light,
            page::DARK => prefs.theme = Theme::Dark,
            page::TRAY => prefs.close_to_tray = !prefs.close_to_tray,
            page::FOLDER => {
                if let Some(folder) = choose_folder(dialog.window, en) {
                    prefs.folder = folder;
                }
            }
            page::LANGUAGE | page::ADD => {
                let r = view.borrow().control(id);
                let mut point = windows_sys::Win32::Foundation::POINT {
                    x: r.left as i32,
                    y: r.bottom as i32,
                };
                unsafe {
                    windows_sys::Win32::Graphics::Gdi::ClientToScreen(dialog.window, &mut point)
                };
                let items = if id == page::LANGUAGE {
                    [(1, "Русский"), (2, "English")]
                } else {
                    [
                        (1, tr(en, "Начать загрузку", "Start download")),
                        (2, tr(en, "Выбрать файлы", "Choose files")),
                    ]
                };
                let checked = if id == page::LANGUAGE {
                    prefs.english
                } else {
                    prefs.select_files
                };
                let entries = items.map(|(id, title)| crate::windows_popup::Item {
                    id,
                    title,
                    checked: id == if checked { 2 } else { 1 },
                    enabled: true,
                });
                if let Some(choice) = crate::windows_popup::show(
                    dialog.window,
                    point.x,
                    point.y,
                    &entries,
                    window_dark(owner),
                ) {
                    if id == page::LANGUAGE {
                        prefs.english = choice == 2;
                    } else {
                        prefs.select_files = choice == 2;
                    }
                }
            }
            page::XRAY | page::SUDOKU => {
                view.borrow_mut().sudoku = id == page::SUDOKU;
                persist = false;
            }
            page::KEY => {
                match vpn_key_input(dialog.window, &key, en) {
                    Ok(Some(value)) => {
                        key = value;
                        let mut state = view.borrow_mut();
                        state.key_present = !key.is_empty();
                        if key.starts_with("sudoku://") {
                            state.sudoku = true;
                        } else if key.starts_with("vless://") {
                            state.sudoku = false;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => result = Err(error),
                }
                persist = false;
            }
            page::TEST | page::SAVE => {
                let sudoku = view.borrow().sudoku;
                let scheme = if sudoku { "sudoku://" } else { "vless://" };
                if !key.starts_with(scheme) {
                    result = Err(if sudoku {
                        tr(
                            en,
                            "Для Sudoku вставьте ключ sudoku://",
                            "Paste a sudoku:// key for Sudoku",
                        )
                    } else {
                        tr(
                            en,
                            "Для Xray вставьте ключ vless://",
                            "Paste a vless:// key for Xray",
                        )
                    }
                    .to_owned());
                } else {
                    result = vpn.save_key(&key);
                    if result.is_ok() && (id == page::TEST || prefs.vpn_enabled) {
                        prefs.vpn_enabled = true;
                        result = vpn.connect(queue, &key);
                    }
                }
            }
            page::UPDATE | page::CONNECT => {
                let link = if id == page::UPDATE {
                    "https://github.com/sanderstripa/torrent/releases/latest"
                } else {
                    "https://connect.sanderstripa.com/?utm_source=chatgpt.com"
                };
                open_url(dialog.window, link);
                persist = false;
            }
            page::TICK => {
                result = vpn.tick(queue);
                persist = false;
            }
            _ => persist = false,
        }

        let save_failed = if persist {
            std::fs::create_dir_all(profile)
                .and_then(|()| prefs.save(profile))
                .is_err()
        } else {
            false
        };
        if save_failed {
            // A failed preference save must not reset an in-memory VPN isolation request.
            let vpn_enabled = prefs.vpn_enabled;
            *prefs = previous;
            prefs.vpn_enabled = vpn_enabled;
        }
        if let Err(error) = result {
            show_error(&error);
        }
        let mut state = view.borrow_mut();
        state.preferences = prefs.clone();
        state.dark = apply_title_theme(owner, prefs.theme);
        state.connected = vpn.state(prefs.vpn_enabled) == 2;
        if persist {
            state.save_failed = save_failed;
        }
        controls.dark.set(state.dark);
        controls.set_language(prefs.english);
        paint_vpn_title(owner, vpn, prefs.vpn_enabled);
        unsafe {
            SetWindowTextW(dialog.window, wide(state.window_title()).as_ptr());
            windows_sys::Win32::Graphics::Gdi::InvalidateRect(dialog.window, null(), 0);
        }
        false
    });
    unsafe { RemovePropW(dialog.window, page::PROPERTY) };
    Ok(())
}

#[cfg(all(feature = "rqbit", feature = "vpn"))]
fn toggle_vpn(
    owner: HWND,
    prefs: &mut Preferences,
    profile: &std::path::Path,
    controls: &Controls,
    vpn: &mut crate::windows_vpn::Vpn,
    queue: &Transfers<'_>,
) -> Result<(), String> {
    if prefs.vpn_enabled {
        prefs.vpn_enabled = false;
        if let Err(error) = prefs.save(profile) {
            prefs.vpn_enabled = true;
            return Err(error.to_string());
        }
        return vpn.disconnect(queue);
    }
    let key = vpn.key()?;
    if key.is_empty() {
        return settings_144(owner, prefs, profile, controls, vpn, queue, 3);
    }
    prefs.vpn_enabled = true;
    let result = vpn.connect(queue, &key);
    std::fs::create_dir_all(profile)
        .and_then(|()| prefs.save(profile))
        .map_err(|error| error.to_string())?;
    result
}

fn menu(owner: HWND, x: i32, y: i32, entries: &[(usize, &str)]) -> Option<usize> {
    #[cfg(feature = "rqbit")]
    {
        let items: Vec<_> = entries
            .iter()
            .map(|&(id, title)| crate::windows_popup::Item {
                id,
                title,
                checked: false,
                enabled: true,
            })
            .collect();
        return crate::windows_popup::show(owner, x, y, &items, window_dark(owner));
    }
    #[cfg(not(feature = "rqbit"))]
    native_menu(owner, x, y, entries)
}

fn native_menu(owner: HWND, x: i32, y: i32, entries: &[(usize, &str)]) -> Option<usize> {
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

struct AppIcons(Vec<HICON>);

impl AppIcons {
    fn attach(window: HWND) -> Self {
        let data = include_bytes!("../../assets/app.ico");
        let count = usize::from(u16::from_le_bytes([data[4], data[5]]));
        let table = data.get(6..6 + count * 16).unwrap_or_default();
        let mut loaded = Vec::new();
        for (kind, width) in [(ICON_SMALL, 32), (ICON_BIG, 256)] {
            let best = table.chunks_exact(16).min_by_key(|format| {
                let dimension = if format[0] == 0 {
                    256
                } else {
                    i32::from(format[0])
                };
                (dimension - width).abs()
            });
            let Some(format) = best else { continue };
            let size = u32::from_le_bytes(format[8..12].try_into().unwrap()) as usize;
            let start = u32::from_le_bytes(format[12..16].try_into().unwrap()) as usize;
            let Some(payload) = data.get(start..start.saturating_add(size)) else {
                continue;
            };
            let icon = unsafe {
                CreateIconFromResourceEx(
                    payload.as_ptr(),
                    payload.len() as u32,
                    1,
                    0x0003_0000,
                    width.min(48),
                    width.min(48),
                    LR_DEFAULTCOLOR,
                )
            };
            if !icon.is_null() {
                unsafe { SendMessageW(window, WM_SETICON, kind as usize, icon as LPARAM) };
                loaded.push(icon);
            }
        }
        Self(loaded)
    }
}

impl Drop for AppIcons {
    fn drop(&mut self) {
        for &icon in &self.0 {
            unsafe { DestroyIcon(icon) };
        }
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
    unsafe { PostMessageW(existing, REOPEN, 0, 0) };
}

fn tray_data(window: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = window;
    data.uID = 1;
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = TRAY_EVENT;
    data.hIcon = unsafe { SendMessageW(window, WM_GETICON, ICON_SMALL as usize, 0) as HICON };
    let tooltip = wide("Torrent");
    data.szTip[..tooltip.len()].copy_from_slice(&tooltip);
    data
}

fn reveal(window: HWND, hidden: &mut bool) {
    unsafe {
        ShowWindow(
            window,
            if IsIconic(window) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        let popup = GetLastActivePopup(window);
        let foreground = if IsWindowEnabled(window) == 0
            && IsWindowVisible(popup) != 0
            && IsWindowEnabled(popup) != 0
        {
            popup
        } else {
            window
        };
        if SetForegroundWindow(foreground) == 0 {
            SetWindowPos(
                foreground,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            let flash = FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd: window,
                dwFlags: FLASHW_TRAY,
                uCount: 3,
                dwTimeout: 0,
            };
            FlashWindowEx(&flash);
        }
        if *hidden {
            Shell_NotifyIconW(NIM_DELETE, &tray_data(window));
            *hidden = false;
        }
    }
}

#[cfg(feature = "rqbit")]
fn migrate_preview_queue(profile: &std::path::Path) -> Result<(), String> {
    use torrent_re_sort::profile::{load_resume_entries, save_queue_order};

    let target = profile.join("session");
    if target.exists() {
        return Ok(());
    }
    let previous = profile
        .parent()
        .ok_or("Invalid profile location")?
        .join("Torrent RE Development")
        .join("session");
    let entries = load_resume_entries(&previous).map_err(|error| error.to_string())?;
    if entries.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(profile).map_err(|error| error.to_string())?;
    let staging = tempfile::tempdir_in(profile).map_err(|error| error.to_string())?;
    for entry in &entries {
        entry
            .save(staging.path())
            .map_err(|error| error.to_string())?;
    }
    let keys = entries
        .iter()
        .map(|entry| entry.key.as_str())
        .collect::<Vec<_>>();
    save_queue_order(staging.path(), &keys).map_err(|error| error.to_string())?;
    std::fs::rename(staging.path(), target).map_err(|error| error.to_string())
}

pub fn run() -> Result<(), String> {
    let smoke = std::env::args_os().any(|argument| argument == "--smoke-test");
    let interaction = std::env::args_os().any(|argument| argument == "--interaction-test");
    TRACE_UI.store(interaction, std::sync::atomic::Ordering::Relaxed);
    trace_ui("starting");
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
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable")?).join(
            if cfg!(feature = "rqbit") {
                "Torrent Rust Edition"
            } else {
                "Torrent RE Development"
            },
        )
    };
    if !smoke && !interaction && !profile.exists() {
        if let Some(local) = profile.parent() {
            let stable = local.join("Torrent");
            let settings = stable.join("settings.txt");
            let position = stable.join("window-position.txt");
            #[cfg(all(feature = "rqbit", feature = "vpn"))]
            {
                let key = stable.join("VPN").join("connection-key.dat");
                if key.is_file() {
                    std::fs::create_dir_all(profile.join("VPN"))
                        .map_err(|error| error.to_string())?;
                    std::fs::copy(key, profile.join("VPN").join("connection-key.dat"))
                        .map_err(|error| error.to_string())?;
                }
            }
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
    #[cfg(feature = "rqbit")]
    if !smoke && !interaction {
        migrate_preview_queue(&profile)?;
    }
    let default_folder = downloads_folder().unwrap_or_else(|| profile.join("Downloads"));
    let mut prefs =
        Preferences::load(&profile, default_folder).map_err(|error| error.to_string())?;
    let session = Session::open(
        PREVIEW_POLICY,
        smoke || interaction || cfg!(feature = "rqbit") && prefs.vpn_enabled,
    )?;
    let (mut transfers, warnings) = Transfers::open(&session, &profile.join("session"))?;
    if !warnings.is_empty() && !smoke {
        show_error(&warnings.join("\n"));
    }
    #[cfg(all(feature = "rqbit", feature = "vpn"))]
    let mut vpn = crate::windows_vpn::Vpn::new(&profile);
    #[cfg(all(feature = "rqbit", feature = "vpn"))]
    if !smoke && !interaction && prefs.vpn_enabled {
        match vpn.key().and_then(|key| vpn.connect(&transfers, &key)) {
            Ok(()) => {}
            Err(error) => show_error(&error),
        }
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
        let taskbar_created = RegisterWindowMessageW(windows_sys::w!("TaskbarCreated"));
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
        trace_ui("created window");
        let _icons = AppIcons::attach(window);
        trace_ui("attached icon");
        let dark = apply_title_theme(window, prefs.theme);
        if !smoke {
            restore_position(window, &profile);
        }
        let controls = Controls::create(window)?;
        trace_ui("created controls");
        controls.set_language(prefs.english);
        controls.dark.set(dark);
        controls.layout();
        #[cfg(all(feature = "rqbit", feature = "vpn"))]
        paint_vpn_title(window, &vpn, prefs.vpn_enabled);
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
        trace_ui("initial list refresh begin");
        controls.refresh(&transfers);
        trace_ui("initial list refresh end");
        if smoke {
            DestroyWindow(window);
        } else {
            DragAcceptFiles(window, 1);
            SetTimer(window, 1, 1000, None);
            ShowWindow(window, SW_SHOW);
            trace_ui("main window shown");
        }
        let mut message = MSG::default();
        let mut hidden_to_tray = false;
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
                _ if message.hwnd == window && message.message == REOPEN => {
                    reveal(window, &mut hidden_to_tray)
                }
                _ if message.hwnd == window && message.message == TRAY_EVENT => {
                    match message.lParam as u32 {
                        WM_LBUTTONDBLCLK => reveal(window, &mut hidden_to_tray),
                        WM_RBUTTONUP => {
                            let mut point = std::mem::zeroed();
                            if GetCursorPos(&mut point) != 0 {
                                SetForegroundWindow(window);
                                match native_menu(
                                    window,
                                    point.x,
                                    point.y,
                                    &[
                                        (1, tr(prefs.english, "Открыть Torrent", "Open Torrent")),
                                        (2, tr(prefs.english, "Выход", "Exit")),
                                    ],
                                ) {
                                    Some(1) => reveal(window, &mut hidden_to_tray),
                                    Some(2) => {
                                        PostMessageW(window, EXIT, 0, 0);
                                    }
                                    _ => {}
                                }
                                PostMessageW(window, WM_NULL, 0, 0);
                            }
                        }
                        _ => {}
                    }
                }
                _ if message.hwnd == window
                    && taskbar_created != 0
                    && message.message == taskbar_created
                    && hidden_to_tray =>
                {
                    if Shell_NotifyIconW(NIM_ADD, &tray_data(window)) == 0 {
                        reveal(window, &mut hidden_to_tray);
                    }
                }
                _ if message.hwnd == window
                    && message.message == REQUEST_CLOSE
                    && prefs.close_to_tray
                    && !smoke =>
                {
                    if !hidden_to_tray {
                        if Shell_NotifyIconW(NIM_ADD, &tray_data(window)) != 0 {
                            let _ = save_position(window, &profile);
                            hidden_to_tray = true;
                            ShowWindow(window, SW_HIDE);
                        } else {
                            show_error(tr(
                                prefs.english,
                                "Не удалось создать значок в трее.",
                                "Could not create the notification area icon.",
                            ));
                        }
                    }
                }
                _ if message.hwnd == window && [REQUEST_CLOSE, EXIT].contains(&message.message) => {
                    if hidden_to_tray {
                        Shell_NotifyIconW(NIM_DELETE, &tray_data(window));
                    }
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
                    #[cfg(all(feature = "rqbit", feature = "vpn"))]
                    if let Err(error) = vpn.tick(&transfers) {
                        if !hidden_to_tray {
                            show_error(&error);
                        }
                    }
                    #[cfg(all(feature = "rqbit", feature = "vpn"))]
                    paint_vpn_title(window, &vpn, prefs.vpn_enabled);
                    if !hidden_to_tray {
                        if let Err(error) =
                            prompt_for_files(window, &mut transfers, &mut prompted, prefs.english)
                        {
                            show_error(&error);
                        }
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
                    reveal(window, &mut hidden_to_tray);
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
                            let [open, magnet] = add_menu_entries(prefs.english);
                            #[cfg(feature = "rqbit")]
                            let items = [
                                open,
                                magnet,
                                (SETTINGS, tr(prefs.english, "Настройки", "Settings")),
                            ];
                            #[cfg(not(feature = "rqbit"))]
                            let items = [open, magnet];
                            if let Some(action) = menu(window, anchor.left, anchor.bottom, &items) {
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
                        #[cfg(not(all(feature = "rqbit", feature = "vpn")))]
                        SETTINGS => settings(window, &mut prefs, &profile, &controls),
                        #[cfg(all(feature = "rqbit", feature = "vpn"))]
                        SETTINGS => settings_144(
                            window, &mut prefs, &profile, &controls, &mut vpn, &transfers, 0,
                        ),
                        #[cfg(all(feature = "rqbit", feature = "vpn"))]
                        VPN_COMMAND => toggle_vpn(
                            window, &mut prefs, &profile, &controls, &mut vpn, &transfers,
                        ),
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
