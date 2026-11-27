#[path = "installer_ui_geometry.rs"]
mod geometry;

use crate::windows_canvas::{dpi_scale, open_target_at_dpi, rect, rgb, window_dpi, Canvas, Fonts};
use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver};
use windows::core::Interface;
use windows::Win32::Graphics::Direct2D::{
    Common::D2D_RECT_F, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_DRAW_TEXT_OPTIONS_NO_SNAP,
};
use windows::Win32::Graphics::DirectWrite::*;
use windows_numerics::Vector2;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetStockObject, InvalidateRect, SetBkColor, SetDCBrushColor,
    SetTextColor, DC_BRUSH, HDC, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE, VK_RETURN, VK_SPACE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: windows_sys::core::PCWSTR = windows_sys::w!("TorrentRustEditionSetup");
const TITLE: windows_sys::core::PCWSTR = windows_sys::w!("Torrent Setup");
const PATH_ID: usize = 101;
const WIDTH: i32 = 840;
const HEIGHT: i32 = 540;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Welcome,
    Installing,
    Finish,
    Settings,
}

enum Worker {
    Progress(u8),
    Done(Result<(), String>),
}

struct Setup {
    page: Page,
    directory: PathBuf,
    english: bool,
    dark: bool,
    shortcuts: [bool; 2],
    launch: bool,
    progress: u8,
    worker: Option<Receiver<Worker>>,
    edit: HWND,
    fonts: Fonts,
    ribbon: Option<windows::Win32::Graphics::Imaging::IWICFormatConverter>,
}

impl Setup {
    fn text<'a>(&self, ru: &'a str, en: &'a str) -> &'a str {
        if self.english {
            en
        } else {
            ru
        }
    }
    fn primary(&self) -> D2D_RECT_F {
        match self.page {
            Page::Welcome => rect(44.0, 287.0, 272.0, 336.0),
            Page::Settings => geometry::settings(self.english).accept,
            _ => rect(306.0, 312.25, 534.0, 361.25),
        }
    }
    fn start(&mut self) {
        let directory = self.directory.clone();
        let shortcuts = self.shortcuts;
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let result =
                crate::installer_windows::install_with_options(&directory, shortcuts, |value| {
                    let _ = send.send(Worker::Progress(value));
                })
                .map_err(|error| error.to_string());
            let _ = send.send(Worker::Done(result));
        });
        self.worker = Some(receive);
        self.page = Page::Installing;
        self.progress = 0;
    }
    fn finish(&self) {
        if self.page == Page::Finish && self.launch {
            let _ = std::process::Command::new(self.directory.join("Torrent-RE.exe")).spawn();
        }
    }
}

pub fn show(directory: PathBuf) -> io::Result<()> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
        .ok()
        .map_err(|error| io::Error::other(error.to_string()))?;
    let result = show_window(directory);
    unsafe { CoUninitialize() };
    result
}

fn show_window(directory: PathBuf) -> io::Result<()> {
    let fonts = Fonts::load(include_bytes!("../../installer/assets/Montserrat.ttf"))
        .map_err(|error| io::Error::other(error.to_string()))?;
    let mut light = 1u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows_sys::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            windows_sys::w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut light as *mut u32).cast(),
            &mut size,
        );
    }
    let setup = RefCell::new(Setup {
        page: Page::Welcome,
        directory,
        english: false,
        dark: light == 0,
        shortcuts: [true, true],
        launch: true,
        progress: 0,
        worker: None,
        edit: null_mut(),
        fonts,
        ribbon: load_ribbon().ok(),
    });
    unsafe {
        let module = GetModuleHandleW(null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: module,
            lpszClassName: CLASS,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(io::Error::last_os_error());
        }
        let scale = dpi_scale(null_mut());
        let window = CreateWindowExW(
            0,
            CLASS,
            TITLE,
            WS_POPUP | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (WIDTH as f32 * scale).round() as i32,
            (HEIGHT as f32 * scale).round() as i32,
            null_mut(),
            null_mut(),
            module,
            (&setup as *const RefCell<Setup>).cast(),
        );
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }
        let path = wide(setup.borrow().directory.as_os_str());
        let edit = CreateWindowExW(
            0,
            windows_sys::w!("EDIT"),
            path.as_ptr(),
            WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            0,
            0,
            1,
            1,
            window,
            PATH_ID as *mut _,
            module,
            null(),
        );
        if edit.is_null() {
            DestroyWindow(window);
            return Err(io::Error::last_os_error());
        }
        setup.borrow_mut().edit = edit;
        let corners = 2u32;
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            window,
            33,
            (&corners as *const u32).cast(),
            4,
        );
        SetTimer(window, 1, 40, None);
        ShowWindow(window, SW_SHOW);
        let mut message = MSG::default();
        loop {
            match GetMessageW(&mut message, null_mut(), 0, 0) {
                -1 => {
                    DestroyWindow(window);
                    return Err(io::Error::last_os_error());
                }
                0 => return Ok(()),
                _ => {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    key: WPARAM,
    extra: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let data = unsafe { &*(extra as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, data.lpCreateParams as isize);
        }
    }
    let Some(setup) =
        (unsafe { (GetWindowLongPtrW(window, GWLP_USERDATA) as *const RefCell<Setup>).as_ref() })
    else {
        return unsafe { DefWindowProcW(window, message, key, extra) };
    };
    let Ok(mut state) = setup.try_borrow_mut() else {
        return unsafe { DefWindowProcW(window, message, key, extra) };
    };
    let scale = dpi_scale(window);
    let raw_x = extra as u16 as i16 as i32;
    let raw_y = (extra >> 16) as u16 as i16 as i32;
    let point = (raw_x as f32 / scale, raw_y as f32 / scale);
    let mut close = false;
    let mut browse = false;
    let mut error = None;
    match message {
        WM_ERASEBKGND | WM_NCACTIVATE => return 1,
        WM_NCCALCSIZE if key != 0 => return 0,
        WM_NCPAINT => return 0,
        WM_NCHITTEST => {
            use windows_sys::Win32::Foundation::POINT;
            let mut point = POINT { x: raw_x, y: raw_y };
            unsafe {
                windows_sys::Win32::Graphics::Gdi::ScreenToClient(window, &mut point);
            }
            return if (point.y as f32) < 55.0 * scale
                && (point.x as f32) < (WIDTH - 130) as f32 * scale
            {
                HTCAPTION as isize
            } else {
                HTCLIENT as isize
            };
        }
        WM_DPICHANGED => {
            let bounds = unsafe { &*(extra as *const RECT) };
            let (x, y, w, h) = (
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
            );
            drop(state);
            unsafe {
                SetWindowPos(
                    window,
                    null_mut(),
                    x,
                    y,
                    w,
                    h,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                PostMessageW(window, WM_APP + 1, 0, 0);
            }
            return 0;
        }
        message if message == WM_APP + 1 => {}
        WM_PAINT => {
            unsafe {
                paint(window, &state);
            }
            return 0;
        }
        WM_CTLCOLOREDIT => {
            let dc = key as HDC;
            let (text, field) = if state.dark {
                (0xe9e7e6, 0x2d2928)
            } else {
                (0x282624, 0xf5f4f3)
            };
            return unsafe {
                SetTextColor(dc, text);
                SetBkColor(dc, field);
                SetDCBrushColor(dc, field);
                GetStockObject(DC_BRUSH) as LRESULT
            };
        }
        WM_COMMAND if key & 0xffff == PATH_ID && key >> 16 == EN_CHANGE as usize => {
            let count = unsafe { GetWindowTextLengthW(state.edit) }.max(0);
            let mut value = vec![0u16; count as usize + 1];
            let read = unsafe { GetWindowTextW(state.edit, value.as_mut_ptr(), count + 1) }.max(0);
            state.directory = PathBuf::from(OsString::from_wide(&value[..read as usize]));
            return 0;
        }
        WM_LBUTTONUP => {
            let (x, y) = point;
            if y < 55.0 && x > WIDTH as f32 - 78.0 {
                close = state.page != Page::Installing;
            } else if y < 55.0 && x > WIDTH as f32 - 120.0 {
                drop(state);
                unsafe {
                    ShowWindow(window, SW_MINIMIZE);
                }
                return 0;
            } else if hit(state.primary(), x, y) {
                match state.page {
                    Page::Welcome | Page::Settings => state.start(),
                    Page::Finish => close = true,
                    _ => {}
                }
            } else if state.page == Page::Welcome && hit(rect(44.0, 350.0, 330.0, 382.0), x, y) {
                state.page = Page::Settings;
            } else if state.page == Page::Finish && (376.25..414.25).contains(&y) {
                state.launch = !state.launch;
            } else if state.page == Page::Settings {
                let g = geometry::settings(state.english);
                if hit(g.back, x, y) {
                    state.page = Page::Welcome;
                } else if hit(g.ru, x, y) {
                    state.english = false;
                } else if hit(g.en, x, y) {
                    state.english = true;
                } else if hit(g.light, x, y) {
                    state.dark = false;
                } else if hit(g.dark, x, y) {
                    state.dark = true;
                } else if hit(g.desktop, x, y) {
                    state.shortcuts[0] = !state.shortcuts[0];
                } else if hit(g.menu, x, y) {
                    state.shortcuts[1] = !state.shortcuts[1];
                } else if hit(g.browse, x, y) {
                    browse = true;
                }
            }
        }
        WM_KEYDOWN => match key as u16 {
            VK_ESCAPE => close = state.page != Page::Installing,
            VK_RETURN if matches!(state.page, Page::Welcome | Page::Settings) => state.start(),
            VK_RETURN if state.page == Page::Finish => close = true,
            VK_SPACE if state.page == Page::Finish => state.launch = !state.launch,
            _ => return 0,
        },
        WM_TIMER => {
            let update = state
                .worker
                .as_ref()
                .and_then(|worker| worker.try_recv().ok());
            match update {
                Some(Worker::Progress(progress)) => state.progress = progress,
                Some(Worker::Done(result)) => {
                    state.worker = None;
                    match result {
                        Ok(()) => {
                            state.page = Page::Finish;
                            state.progress = 100;
                            if let Err(detail) = persist_appearance(state.english, state.dark) {
                                error = Some(detail.to_string());
                            }
                        }
                        Err(detail) => {
                            state.page = Page::Welcome;
                            error = Some(detail);
                        }
                    }
                }
                None => return 0,
            }
        }
        WM_CLOSE => close = state.page != Page::Installing,
        WM_DESTROY => {
            unsafe {
                PostQuitMessage(0);
            }
            return 0;
        }
        _ => return unsafe { DefWindowProcW(window, message, key, extra) },
    }
    if close {
        state.finish();
        drop(state);
        unsafe {
            DestroyWindow(window);
        }
        return 0;
    }
    let edit = state.edit;
    let path = geometry::settings(state.english).path;
    let show_edit = state.page == Page::Settings;
    drop(state);

    if browse {
        if let Some(folder) = choose_folder(window) {
            unsafe {
                SetWindowTextW(edit, wide(folder.as_os_str()).as_ptr());
            }
        }
    }
    unsafe {
        if show_edit {
            MoveWindow(
                edit,
                ((path.left + 10.0) * scale).round() as i32,
                ((path.top + 9.0) * scale).round() as i32,
                ((path.right - path.left - 20.0) * scale).round() as i32,
                (19.0 * scale).round() as i32,
                1,
            );
            ShowWindow(edit, SW_SHOW);
        } else if IsWindowVisible(edit) != 0 {
            ShowWindow(edit, SW_HIDE);
            SetFocus(window);
        }
        InvalidateRect(window, null(), 0);
    }
    if let Some(error) = error {
        crate::installer_windows::message(&error, true);
    }
    0
}

fn persist_appearance(english: bool, dark: bool) -> io::Result<()> {
    use torrent_re_sort::profile::{Preferences, Theme};
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};
    let local = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| io::Error::other("LOCALAPPDATA is unavailable"))?;
    let profile = PathBuf::from(local).join("Torrent Rust Edition");
    let mut raw = null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, 0, null_mut(), &mut raw) };
    let default = if hr >= 0 && !raw.is_null() {
        let mut count = 0;
        unsafe {
            while *raw.add(count) != 0 {
                count += 1;
            }
            let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(raw, count)));
            CoTaskMemFree(raw.cast());
            path
        }
    } else {
        profile.join("Downloads")
    };
    let mut preferences = Preferences::load(&profile, default)?;
    preferences.english = english;
    preferences.theme = if dark { Theme::Dark } else { Theme::Light };
    std::fs::create_dir_all(&profile)?;
    preferences.save(&profile)
}

fn choose_folder(owner: HWND) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        SHBrowseForFolderW, SHGetPathFromIDListW, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
    };
    let mut info = BROWSEINFOW {
        hwndOwner: owner,
        ulFlags: BIF_RETURNONLYFSDIRS,
        ..Default::default()
    };
    let item = unsafe { SHBrowseForFolderW(&mut info) };
    if item.is_null() {
        return None;
    }
    let mut raw = [0u16; 260];
    let ok = unsafe { SHGetPathFromIDListW(item, raw.as_mut_ptr()) };
    unsafe {
        CoTaskMemFree(item.cast());
    }
    let end = raw
        .iter()
        .position(|&value| value == 0)
        .unwrap_or(raw.len());
    (ok != 0).then(|| PathBuf::from(OsString::from_wide(&raw[..end])))
}

fn hit(r: D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn load_ribbon() -> windows::core::Result<windows::Win32::Graphics::Imaging::IWICFormatConverter> {
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    // Raster asset shipped by the CONNECT installer pinned by 1.4.4 (589669b4).
    static RIBBON: &[u8] = include_bytes!("../installer/assets/connect-mobius.png");
    unsafe {
        let imaging: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = imaging.CreateStream()?;
        stream.InitializeFromMemory(RIBBON)?;
        let decoder =
            imaging.CreateDecoderFromStream(&stream, null(), WICDecodeMetadataCacheOnLoad)?;
        let frame = decoder.GetFrame(0)?;
        let converter = imaging.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        Ok(converter)
    }
}

fn ribbon(
    canvas: &Canvas<'_>,
    source: &windows::Win32::Graphics::Imaging::IWICFormatConverter,
    region: D2D_RECT_F,
) -> windows::core::Result<()> {
    unsafe {
        let bitmap = canvas.target.CreateBitmapFromWicBitmap(source, None)?;
        let size = bitmap.GetSize();
        let scale = ((region.right - region.left) / size.width)
            .min((region.bottom - region.top) / size.height);
        let (width, height) = (size.width * scale, size.height * scale);
        let left = (region.left + region.right - width) / 2.0;
        let top = (region.top + region.bottom - height) / 2.0;
        canvas.target.DrawBitmap(
            &bitmap,
            Some(&rect(left, top, left + width, top + height)),
            1.0,
            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
            None,
        );
    }
    Ok(())
}

fn label(
    canvas: &Canvas<'_>,
    fonts: &Fonts,
    value: &str,
    area: D2D_RECT_F,
    size: f32,
    color: u32,
    align: DWRITE_TEXT_ALIGNMENT,
    weight: DWRITE_FONT_WEIGHT,
    tracking: f32,
) -> windows::core::Result<()> {
    let text = value.encode_utf16().collect::<Vec<_>>();
    let format = fonts.fresh_format(size, weight)?;
    let axis = [DWRITE_FONT_AXIS_VALUE {
        axisTag: DWRITE_FONT_AXIS_TAG_WEIGHT,
        value: weight.0 as f32,
    }];
    let range = DWRITE_TEXT_RANGE {
        startPosition: 0,
        length: text.len() as u32,
    };
    unsafe {
        let with_axis: IDWriteTextFormat3 = format.cast()?;
        with_axis.SetFontAxisValues(&axis)?;
        let ascent = (size * 0.968).round();
        let descent = (size * 0.251).round();
        format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, ascent + descent, ascent)?;
        format.SetTextAlignment(align)?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let layout = fonts.factory.CreateTextLayout(
            &text,
            &format,
            area.right - area.left,
            area.bottom - area.top,
        )?;
        let variable: IDWriteTextLayout4 = layout.cast()?;
        variable.SetFontAxisValues(&axis, range)?;
        if tracking != 0.0 {
            let spaced: IDWriteTextLayout1 = layout.cast()?;
            spaced.SetCharacterSpacing(tracking / 2.0, tracking / 2.0, 0.0, range)?;
        }
        canvas.brush.SetColor(&rgb(color));
        canvas.target.DrawTextLayout(
            Vector2 {
                X: area.left,
                Y: area.top,
            },
            &layout,
            &canvas.brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP | D2D1_DRAW_TEXT_OPTIONS_NO_SNAP,
        );
    }
    Ok(())
}

unsafe fn paint(window: HWND, setup: &Setup) {
    let mut data = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut data) };
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(window, &mut bounds);
    }
    let _ = draw(dc, bounds, window_dpi(window), setup);
    unsafe {
        EndPaint(window, &data);
    }
}

fn draw(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    bounds: RECT,
    dpi: f32,
    setup: &Setup,
) -> windows::core::Result<()> {
    let (_, target) = open_target_at_dpi(dc, bounds, dpi)?;
    let (bg, fg, muted, border, button, inverse, field) = if setup.dark {
        (
            0x08090b, 0xe6e7e9, 0x9a9da3, 0x35363b, 0xe3e4df, 0x08090b, 0x28292d,
        )
    } else {
        (
            0xfcfcfd, 0x242628, 0x73767b, 0xe6e7e9, 0x0671ff, 0xffffff, 0xf3f4f5,
        )
    };
    let brush = unsafe { target.CreateSolidColorBrush(&rgb(fg), None)? };
    let canvas = Canvas {
        target: &target,
        brush,
    };
    let w = WIDTH as f32;
    let font = &setup.fonts;
    let tr = |ru, en| setup.text(ru, en);
    unsafe {
        target.BeginDraw();
        target.Clear(Some(&rgb(bg)));
        target.SetTextAntialiasMode(
            windows::Win32::Graphics::Direct2D::D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE,
        );
    }
    let result = (|| {
        let text = |value, r, size, ink| {
            label(
                &canvas,
                font,
                value,
                r,
                size,
                ink,
                DWRITE_TEXT_ALIGNMENT_LEADING,
                DWRITE_FONT_WEIGHT_NORMAL,
                0.0,
            )
        };
        let centered = |value, r, size, ink| {
            label(
                &canvas,
                font,
                value,
                r,
                size,
                ink,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_FONT_WEIGHT_NORMAL,
                0.0,
            )
        };
        let primary = |value, r| {
            canvas.rounded(r, 10.0, button, None);
            label(
                &canvas,
                font,
                value,
                r,
                14.0,
                inverse,
                DWRITE_TEXT_ALIGNMENT_CENTER,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                0.2,
            )
        };
        let switch = |x: f32, y: f32, on| {
            canvas.rounded(
                rect(x, y, x + 38.0, y + 22.0),
                11.0,
                if on { button } else { bg },
                (!on).then_some(muted),
            );
            canvas.circle(
                x + if on { 27.0 } else { 11.0 },
                y + 11.0,
                7.0,
                if on { inverse } else { muted },
                true,
            );
        };
        let choice = |value, r, chosen| {
            if chosen {
                canvas.rounded(r, 5.0, button, Some(button));
            }
            centered(value, r, 11.0, if chosen { inverse } else { fg })
        };
        let caption_line = |x, y, a, b| {
            canvas.line(x, y, a, b, fg, 1.25);
            canvas.circle(x, y, 0.625, fg, true);
            canvas.circle(a, b, 0.625, fg, true);
        };
        caption_line(w - 105.0, 36.0, w - 95.0, 36.0);
        caption_line(w - 63.0, 32.0, w - 55.0, 40.0);
        caption_line(w - 55.0, 32.0, w - 63.0, 40.0);
        canvas.outline(
            rect(0.5, 0.5, w - 0.5, HEIGHT as f32 - 0.5),
            0.0,
            border,
            1.0,
        );
        canvas.line(44.0, 490.0, w - 44.0, 490.0, border, 1.0);
        label(
            &canvas,
            font,
            "by CONNECT.",
            rect(44.0, 500.0, 270.0, 532.0),
            10.0,
            muted,
            DWRITE_TEXT_ALIGNMENT_LEADING,
            DWRITE_FONT_WEIGHT_NORMAL,
            1.0,
        )?;
        label(
            &canvas,
            font,
            tr("Версия 1.4.4 RE", "Version 1.4.4 RE"),
            rect(w - 220.0, 500.0, w - 44.0, 532.0),
            10.0,
            muted,
            DWRITE_TEXT_ALIGNMENT_TRAILING,
            DWRITE_FONT_WEIGHT_NORMAL,
            1.0,
        )?;
        match setup.page {
            Page::Welcome => {
                if let Some(source) = &setup.ribbon {
                    ribbon(&canvas, source, rect(377.0, 61.0, 817.0, 426.0))?;
                }
                label(
                    &canvas,
                    font,
                    tr("УСТАНОВКА", "INSTALLATION"),
                    rect(44.0, 165.0, 380.0, 193.0),
                    12.0,
                    fg,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    DWRITE_FONT_WEIGHT_SEMI_BOLD,
                    4.0,
                )?;
                label(
                    &canvas,
                    font,
                    "Torrent",
                    rect(44.0, 202.0, 410.0, 258.0),
                    42.0,
                    fg,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    -1.8,
                )?;
                primary(tr("Установить", "Install"), setup.primary())?;
                text(
                    tr("Настройки установки", "Installation settings"),
                    rect(44.0, 350.0, 330.0, 382.0),
                    13.0,
                    muted,
                )?;
            }
            Page::Installing => {
                if let Some(source) = &setup.ribbon {
                    ribbon(&canvas, source, rect(185.0, 69.0, 655.0, 374.0))?;
                }
                canvas.rounded(rect(44.0, 435.0, w - 44.0, 439.0), 2.0, border, None);
                if setup.progress > 0 {
                    let end = 44.0 + (w - 88.0) * f32::from(setup.progress) / 100.0;
                    canvas.rounded(rect(44.0, 435.0, end, 439.0), 2.0, button, None);
                }
                centered(
                    tr("Установка…", "Installing…"),
                    rect(44.0, 443.0, w - 44.0, 475.0),
                    11.0,
                    muted,
                )?;
            }
            Page::Finish => {
                label(
                    &canvas,
                    font,
                    "Torrent",
                    rect(44.0, 131.75, w - 44.0, 183.75),
                    42.0,
                    fg,
                    DWRITE_TEXT_ALIGNMENT_CENTER,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    -1.8,
                )?;
                centered(
                    tr("Установка завершена", "Installation complete"),
                    rect(44.0, 195.75, w - 44.0, 213.75),
                    14.0,
                    fg,
                )?;
                primary(tr("Готово", "Done"), setup.primary())?;
                let start = if setup.english { 289.0 } else { 279.0 };
                switch(start, 384.25, setup.launch);
                text(
                    tr(
                        "Запустить Torrent после установки",
                        "Launch Torrent after installation",
                    ),
                    rect(start + 51.0, 388.25, w - 120.0, 402.25),
                    11.0,
                    fg,
                )?;
            }
            Page::Settings => {
                let g = geometry::settings(setup.english);
                label(
                    &canvas,
                    font,
                    tr("Настройки установки", "Installation settings"),
                    g.title,
                    23.0,
                    fg,
                    DWRITE_TEXT_ALIGNMENT_LEADING,
                    DWRITE_FONT_WEIGHT_MEDIUM,
                    0.0,
                )?;
                text(tr("Назад", "Back"), g.back, 13.0, muted)?;
                text(tr("Язык", "Language"), g.language, 12.0, fg)?;
                choice("RU", g.ru, !setup.english)?;
                choice("EN", g.en, setup.english)?;
                text(tr("Тема", "Theme"), g.theme, 12.0, fg)?;
                choice(tr("Светлая", "Light"), g.light, !setup.dark)?;
                choice(tr("Тёмная", "Dark"), g.dark, setup.dark)?;
                text(
                    tr("Папка установки", "Installation folder"),
                    g.path_label,
                    12.0,
                    fg,
                )?;
                canvas.rounded(g.path, 6.0, field, Some(border));
                canvas.rounded(g.browse, 6.0, bg, Some(border));
                centered(tr("Выбрать…", "Browse…"), g.browse, 12.0, fg)?;
                switch(g.desktop.left, g.desktop.top + 8.0, setup.shortcuts[0]);
                text(
                    tr("Ярлык на рабочем столе", "Desktop shortcut"),
                    g.desktop_label,
                    13.0,
                    fg,
                )?;
                switch(g.menu.left, g.menu.top + 8.0, setup.shortcuts[1]);
                text(
                    tr("Ярлык в меню «Пуск»", "Start menu shortcut"),
                    g.menu_label,
                    13.0,
                    fg,
                )?;
                primary(tr("Принять и установить", "Accept and install"), g.accept)?;
            }
        }
        Ok(())
    })();
    let end = unsafe { target.EndDraw(None, None) };
    result.and(end)
}
