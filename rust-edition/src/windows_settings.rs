use crate::windows_render::{open_target, rect, rgb, use_fonts, Canvas};
use torrent_re_sort::profile::{Preferences, Theme};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};

pub const NAV: usize = 140;
pub const CLOSE: usize = 111;
pub const SYSTEM: usize = 124;
pub const LIGHT: usize = 125;
pub const DARK: usize = 126;
pub const LANGUAGE: usize = 127;
pub const ADD: usize = 128;
pub const TRAY: usize = 129;
pub const FOLDER: usize = 121;
pub const UPDATE: usize = 122;
pub const CONNECT: usize = 123;
pub const XRAY: usize = 145;
pub const SUDOKU: usize = 146;
pub const KEY: usize = 147;
pub const TEST: usize = 148;
pub const SAVE: usize = 149;

pub struct SettingsView {
    pub preferences: Preferences,
    pub page: usize,
    pub width: f32,
    pub dark: bool,
    pub hover: Option<usize>,
    pub focus: Option<usize>,
    pub sudoku: bool,
    pub key_present: bool,
    pub connected: bool,
    pub save_failed: bool,
}

impl SettingsView {
    pub fn window_title(&self) -> &'static str {
        const RU: [&str; 5] = ["Настройки", "Общие", "Загрузки", "VPN", "О Torrent"];
        const EN: [&str; 5] = ["Settings", "General", "Downloads", "VPN", "About Torrent"];
        if self.preferences.english {
            EN[self.page]
        } else {
            RU[self.page]
        }
    }

    pub fn height(&self) -> f32 {
        if self.page == 0 {
            321.0
        } else {
            370.0
        }
    }

    pub fn ids(&self) -> Vec<usize> {
        let mut ids = vec![CLOSE];
        if self.page != 0 {
            ids.push(NAV);
        }
        ids.extend(NAV + 1..=NAV + 4);
        ids.extend_from_slice(match self.page {
            1 => &[SYSTEM, LIGHT, DARK, LANGUAGE, TRAY],
            2 => &[FOLDER, ADD],
            3 => &[XRAY, SUDOKU, KEY, TEST, SAVE],
            4 => &[UPDATE],
            _ => &[],
        });
        ids.push(CONNECT);
        ids
    }

    pub fn control(&self, id: usize) -> D2D_RECT_F {
        if (NAV..=NAV + 4).contains(&id) {
            let page = id - NAV;
            if self.page == 0 {
                let slot = page.saturating_sub(1);
                let width = (self.width - 68.0) / 2.0;
                let x = 28.0 + (slot % 2) as f32 * (width + 12.0);
                let y = 82.0 + (slot / 2) as f32 * 105.0;
                return rect(x, y, x + width, y + 92.0);
            }
            let y = 80.0 + page as f32 * 42.0;
            return rect(12.0, y, 163.0, y + 38.0);
        }
        let right = self.width - 28.0;
        match id {
            CLOSE => rect(right - 22.0, 16.0, right + 10.0, 48.0),
            SYSTEM..=DARK => {
                let x = right - 270.0 + (id - SYSTEM) as f32 * 90.0;
                rect(x, 100.0, x + 90.0, 134.0)
            }
            LANGUAGE | ADD => rect(right - 240.0, 163.0, right, 199.0),
            FOLDER => rect(right - 90.0, 99.0, right, 135.0),
            TRAY | UPDATE => rect(right - 112.0, 223.0, right, 259.0),
            XRAY => rect(right - 244.0, 100.0, right - 122.0, 134.0),
            SUDOKU => rect(right - 122.0, 100.0, right, 134.0),
            KEY => rect(right - 244.0, 163.0, right, 201.0),
            TEST => rect(right - 244.0, 223.0, right - 128.0, 259.0),
            SAVE => rect(right - 116.0, 223.0, right, 259.0),
            CONNECT => rect(
                self.width / 2.0 - 44.0,
                self.height() - 28.0,
                self.width / 2.0 + 44.0,
                self.height() - 4.0,
            ),
            _ => D2D_RECT_F::default(),
        }
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        self.ids().into_iter().find(|&id| {
            let r = self.control(id);
            x >= r.left && x < r.right && y >= r.top && y < r.bottom
        })
    }
}

pub unsafe fn paint(window: HWND, view: &SettingsView) {
    let mut paint = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut paint) };
    let bounds = RECT {
        left: 0,
        top: 0,
        right: view.width as i32,
        bottom: view.height() as i32,
    };
    let _ = draw(dc, bounds, view);
    unsafe { EndPaint(window, &paint) };
}

fn draw(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    bounds: RECT,
    view: &SettingsView,
) -> windows::core::Result<()> {
    let (_, target) = open_target(dc, bounds)?;
    let surface = if view.dark { 0x202124 } else { 0xfcfcfd };
    let field = if view.dark { 0x28292d } else { 0xf3f4f5 };
    let line = if view.dark { 0x35363b } else { 0xe6e7e9 };
    let foreground = if view.dark { 0xeeeeef } else { 0x242628 };
    let muted = if view.dark { 0x96979d } else { 0x73767b };
    let selected = if view.dark { 0x202125 } else { 0xededef };
    let brush = unsafe { target.CreateSolidColorBrush(&rgb(surface), None)? };
    let canvas = Canvas {
        target: &target,
        brush,
    };
    unsafe {
        target.BeginDraw();
        target.Clear(Some(&rgb(surface)));
    }
    let result = use_fonts(|fonts| {
        let en = view.preferences.english;
        let text = |ru, en_text| if en { en_text } else { ru };
        let titles = [
            text("Все настройки", "All settings"),
            text("Общие", "General"),
            text("Загрузки", "Downloads"),
            "VPN",
            text("О Torrent", "About Torrent"),
        ];
        let hints = [
            "",
            text(
                "Тема, язык и системный трей",
                "Theme, language and notification area",
            ),
            text(
                "Папка и добавление торрентов",
                "Download folder and adding torrents",
            ),
            text("Подключения VLESS и Sudoku", "VLESS and Sudoku connections"),
            text("Версия и обновления", "Version and updates"),
        ];
        let right = view.width - 28.0;
        let label =
            |value, shape, size| canvas.label(fonts, value, shape, size, false, foreground, false);
        let button = |id, value| {
            let r = view.control(id);
            canvas.rounded(
                r,
                8.0,
                if view.hover == Some(id) {
                    selected
                } else {
                    surface
                },
                Some(line),
            );
            canvas.centered(fonts, value, r, 13, foreground)
        };
        let field_label = |id, value, size| {
            let r = view.control(id);
            canvas.rounded(r, 7.0, field, Some(line));
            canvas.label(
                fonts,
                value,
                rect(r.left + 12.0, r.top, r.right - 25.0, r.bottom),
                size,
                false,
                foreground,
                false,
            )
        };
        let row = |name, id| {
            let r = view.control(id);
            label(name, rect(192.0, r.top, r.left - 8.0, r.bottom), 13)
        };
        let separator = |y| canvas.line(192.0, y, right, y, line, 1.0);

        canvas.label(
            fonts,
            if view.page == 0 {
                text("Настройки", "Settings")
            } else {
                titles[view.page]
            },
            rect(28.0, 13.0, right - 40.0, 52.0),
            20,
            true,
            foreground,
            false,
        )?;
        let close = view.control(CLOSE);
        if view.hover == Some(CLOSE) {
            canvas.rounded(
                close,
                16.0,
                if view.dark { 0x54242f } else { 0xf6d1d9 },
                None,
            );
        }
        let close_color = if view.hover == Some(CLOSE) {
            foreground
        } else {
            muted
        };
        canvas.line(
            close.left + 11.0,
            27.0,
            close.left + 21.0,
            37.0,
            close_color,
            1.5,
        );
        canvas.line(
            close.left + 21.0,
            27.0,
            close.left + 11.0,
            37.0,
            close_color,
            1.5,
        );
        if view.page != 0 {
            canvas.line(175.0, 68.0, 175.0, 316.0, line, 1.0);
        }
        for page in usize::from(view.page == 0)..=4 {
            let id = NAV + page;
            let tile = view.control(id);
            let card = view.page == 0;
            if card || view.hover == Some(id) || view.page == page {
                canvas.rounded(
                    tile,
                    8.0,
                    if view.hover == Some(id) || view.page == page {
                        selected
                    } else {
                        surface
                    },
                    card.then_some(line),
                );
            }
            let y = (tile.top + tile.bottom) / 2.0;
            draw_icon(
                &canvas,
                page,
                tile.left + if card { 26.0 } else { 17.0 },
                y,
                foreground,
            );
            let x = tile.left + if card { 49.0 } else { 34.0 };
            label(
                titles[page],
                rect(
                    x,
                    if card { y - 26.0 } else { tile.top },
                    tile.right - 15.0,
                    if card { y } else { tile.bottom },
                ),
                13,
            )?;
            if card {
                canvas.label(
                    fonts,
                    hints[page],
                    rect(x, y, tile.right - 24.0, y + 24.0),
                    11,
                    false,
                    muted,
                    false,
                )?;
                canvas.line(tile.right - 16.0, y - 3.0, tile.right - 13.0, y, muted, 1.5);
                canvas.line(tile.right - 13.0, y, tile.right - 16.0, y + 3.0, muted, 1.5);
            }
        }
        match view.page {
            1 => {
                row(text("Тема", "Theme"), SYSTEM)?;
                canvas.rounded(
                    rect(right - 273.0, 97.0, right + 3.0, 137.0),
                    8.0,
                    field,
                    Some(line),
                );
                for (id, name, theme) in [
                    (SYSTEM, text("Системная", "System"), Theme::System),
                    (LIGHT, text("Светлая", "Light"), Theme::Light),
                    (DARK, text("Тёмная", "Dark"), Theme::Dark),
                ] {
                    let r = view.control(id);
                    if view.preferences.theme == theme {
                        canvas.rounded(
                            rect(r.left + 2.0, r.top + 1.0, r.right - 2.0, r.bottom - 1.0),
                            6.0,
                            surface,
                            Some(line),
                        );
                    } else if view.hover == Some(id) {
                        canvas.rounded(r, 6.0, selected, None);
                    }
                    canvas.centered(fonts, name, r, 12, foreground)?;
                }
                separator(149.0);
                row(text("Язык интерфейса", "Interface language"), LANGUAGE)?;
                field_label(LANGUAGE, text("Русский", "English"), 13)?;
                chevron(&canvas, view.control(LANGUAGE), muted);
                separator(213.0);
                row(
                    text("Скрывать в трей при закрытии", "Hide to tray on close"),
                    TRAY,
                )?;
                button(
                    TRAY,
                    if view.preferences.close_to_tray {
                        text("Включено", "On")
                    } else {
                        text("Выключено", "Off")
                    },
                )?;
            }
            2 => {
                label(
                    text("Папка загрузок", "Download folder"),
                    rect(192.0, 99.0, right - 240.0, 135.0),
                    13,
                )?;
                let choose = view.control(FOLDER);
                canvas.rounded(
                    rect(right - 232.0, 99.0, choose.left - 10.0, 135.0),
                    7.0,
                    field,
                    Some(line),
                );
                canvas.label(
                    fonts,
                    &view.preferences.folder.to_string_lossy(),
                    rect(right - 222.0, 99.0, choose.left - 18.0, 135.0),
                    12,
                    false,
                    muted,
                    false,
                )?;
                button(FOLDER, text("Выбрать", "Choose"))?;
                separator(149.0);
                row(text("При добавлении торрента", "On adding a torrent"), ADD)?;
                field_label(
                    ADD,
                    if view.preferences.select_files {
                        text("Выбрать файлы", "Choose files")
                    } else {
                        text("Начать загрузку", "Start download")
                    },
                    12,
                )?;
                chevron(&canvas, view.control(ADD), muted);
            }
            3 => {
                row(text("Протокол", "Protocol"), XRAY)?;
                canvas.rounded(
                    rect(right - 247.0, 97.0, right + 3.0, 137.0),
                    8.0,
                    field,
                    Some(line),
                );
                for (id, name, active) in [
                    (XRAY, "Xray · VLESS", !view.sudoku),
                    (SUDOKU, "Sudoku", view.sudoku),
                ] {
                    let r = view.control(id);
                    if active {
                        canvas.rounded(
                            rect(r.left + 2.0, r.top + 1.0, r.right - 2.0, r.bottom - 1.0),
                            6.0,
                            surface,
                            Some(line),
                        );
                    } else if view.hover == Some(id) {
                        canvas.rounded(r, 6.0, selected, None);
                    }
                    canvas.centered(fonts, name, r, 12, if active { foreground } else { muted })?;
                }
                separator(149.0);
                row(text("Ключ подключения", "Connection key"), KEY)?;
                let value = if !view.key_present {
                    text("Нажмите, чтобы вставить ключ", "Click to paste a key")
                } else if view.sudoku {
                    "sudoku://••••••••••••"
                } else {
                    "vless://••••••••••••"
                };
                field_label(KEY, value, 12)?;
                let r = view.control(KEY);
                let status = if view.connected {
                    if view.dark {
                        0x57d6a3
                    } else {
                        0x087f5b
                    }
                } else {
                    0xc95050
                };
                canvas.circle(r.right - 16.0, (r.top + r.bottom) / 2.0, 4.5, status, true);
                button(TEST, text("Проверить", "Test"))?;
                button(SAVE, text("Сохранить", "Save"))?;
            }
            4 => {
                label(
                    text("Версия Torrent", "Torrent version"),
                    rect(192.0, 99.0, right - 150.0, 135.0),
                    13,
                )?;
                canvas.label(
                    fonts,
                    "1.4.4 RE",
                    rect(right - 150.0, 99.0, right, 135.0),
                    13,
                    false,
                    foreground,
                    true,
                )?;
                separator(149.0);
                label(
                    text("Разработчик", "Developer"),
                    rect(192.0, 163.0, right - 180.0, 199.0),
                    13,
                )?;
                canvas.label(
                    fonts,
                    "Sander Stripa",
                    rect(right - 180.0, 163.0, right, 199.0),
                    13,
                    false,
                    muted,
                    true,
                )?;
                button(UPDATE, text("Обновить", "Update"))?;
            }
            _ => {}
        }
        if view.page != 0 && view.save_failed {
            canvas.label(
                fonts,
                text("Не удалось сохранить настройки", "Could not save settings"),
                rect(192.0, 283.0, right, 305.0),
                12,
                false,
                0xc95050,
                false,
            )?;
        }
        canvas.centered(fonts, "by connect.", view.control(CONNECT), 11, muted)?;
        if let Some(focus) = view.focus {
            let r = view.control(focus);
            unsafe {
                canvas.brush.SetColor(&rgb(muted));
                canvas.target.DrawRoundedRectangle(
                    &windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                        rect: r,
                        radiusX: 8.0,
                        radiusY: 8.0,
                    },
                    &canvas.brush,
                    2.0,
                    None,
                );
            }
        }
        Ok(())
    });
    let end = unsafe { target.EndDraw(None, None) };
    result.and(end)
}

fn chevron(canvas: &Canvas<'_>, r: D2D_RECT_F, color: u32) {
    let x = r.right - 22.0;
    let y = r.top + 15.0;
    canvas.line(x, y, x + 4.0, y + 4.0, color, 1.5);
    canvas.line(x + 4.0, y + 4.0, x + 8.0, y, color, 1.5);
}

fn draw_icon(c: &Canvas<'_>, page: usize, x: f32, y: f32, ink: u32) {
    let line = |a, b, d, e| c.line(x + a, y + b, x + d, y + e, ink, 1.35);
    let circle = |dx, dy, r| c.circle(x + dx, y + dy, r, ink, false);
    let outline =
        |l, t, r, b, radius| c.outline(rect(x + l, y + t, x + r, y + b), radius, ink, 1.35);
    match page {
        1 => {
            line(-8.0, -5.0, 8.0, -5.0);
            line(-8.0, 5.0, 8.0, 5.0);
            circle(-3.0, -5.0, 2.5);
            circle(3.0, 5.0, 2.5);
        }
        2 => {
            for (a, b, d, e) in [
                (0.0, -8.0, 0.0, 3.0),
                (-4.0, -1.0, 0.0, 3.0),
                (0.0, 3.0, 4.0, -1.0),
                (-8.0, 3.0, -8.0, 8.0),
                (-8.0, 8.0, 8.0, 8.0),
                (8.0, 8.0, 8.0, 3.0),
            ] {
                line(a, b, d, e);
            }
        }
        3 => {
            outline(-7.0, -2.0, 7.0, 9.0, 2.0);
            line(-4.0, -2.0, -4.0, -6.0);
            line(4.0, -2.0, 4.0, -6.0);
            for segment in 0..8 {
                let a = std::f32::consts::PI * segment as f32 / 8.0;
                let b = std::f32::consts::PI * (segment + 1) as f32 / 8.0;
                line(
                    4.0 * a.cos(),
                    -6.0 - 4.0 * a.sin(),
                    4.0 * b.cos(),
                    -6.0 - 4.0 * b.sin(),
                );
            }
            line(0.0, 2.0, 0.0, 5.0);
        }
        4 => {
            circle(0.0, 0.0, 9.0);
            line(0.0, -1.0, 0.0, 5.0);
            circle(0.0, -5.0, 0.25);
        }
        _ => {
            for dx in [-7.0, 2.0] {
                for dy in [-7.0, 2.0] {
                    outline(dx, dy, dx + 5.0, dy + 5.0, 0.0);
                }
            }
        }
    }
}

pub const TICK: usize = 150;
pub const PROPERTY: windows_sys::core::PCWSTR = windows_sys::w!("TorrentSettings144");

pub unsafe fn dispatch(window: HWND, message: u32, wparam: usize, lparam: isize) -> Option<isize> {
    use std::ptr::null;
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
    use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{VK_RETURN, VK_SHIFT, VK_SPACE, VK_TAB};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let cell = unsafe {
        GetPropW(window, PROPERTY)
            .cast::<std::cell::RefCell<SettingsView>>()
            .as_ref()
    }?;
    let mut view = cell.try_borrow_mut().ok()?;
    let x = (lparam as u16 as i16) as f32;
    let y = ((lparam >> 16) as u16 as i16) as f32;
    match message {
        WM_PAINT => unsafe {
            paint(window, &view);
            return Some(0);
        },
        WM_ERASEBKGND => return Some(1),
        WM_MOUSEMOVE => {
            let hover = view.hit(x, y);
            if view.hover == hover {
                return Some(0);
            }
            view.hover = hover;
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: window,
                dwHoverTime: 0,
            };
            unsafe { TrackMouseEvent(&mut tracking) };
        }
        WM_MOUSELEAVE => view.hover = None,
        WM_LBUTTONUP => {
            if let Some(id) = view.hit(x, y) {
                view.focus = None;
                unsafe { PostMessageW(window, crate::windows_app::ACTION, id, 0) };
            }
        }
        WM_COMMAND if wparam >> 16 == 0 => {
            let id = wparam & 0xffff;
            if view.ids().contains(&id) || id == CLOSE {
                unsafe { PostMessageW(window, crate::windows_app::ACTION, id, 0) };
            }
            return Some(0);
        }
        WM_KEYDOWN if wparam == VK_TAB as usize => {
            let ids = view.ids();
            let backwards = unsafe { GetKeyState(VK_SHIFT as i32) < 0 };
            let next = view
                .focus
                .and_then(|id| ids.iter().position(|item| *item == id))
                .map(|index| (index + if backwards { ids.len() - 1 } else { 1 }) % ids.len())
                .unwrap_or_else(|| if backwards { ids.len() - 1 } else { 0 });
            view.focus = Some(ids[next]);
        }
        WM_KEYDOWN if matches!(wparam as u16, VK_RETURN | VK_SPACE) => {
            if let Some(id) = view.focus {
                unsafe { PostMessageW(window, crate::windows_app::ACTION, id, 0) };
            }
        }
        _ => return None,
    }
    unsafe { InvalidateRect(window, null(), 0) };
    Some(0)
}
