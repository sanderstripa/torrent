use crate::windows_canvas::{
    dpi_scale, open_target_at_dpi, rect, rgb, use_fonts, window_dpi, Canvas,
};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, ScreenToClient,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, ReleaseCapture, SetCapture, SetFocus, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME,
    VK_RETURN, VK_SPACE, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: windows_sys::core::PCWSTR = windows_sys::w!("TorrentRustStyledPopup");

#[derive(Clone, Copy)]
pub struct Item<'a> {
    pub id: usize,
    pub title: &'a str,
    pub checked: bool,
    pub enabled: bool,
}

struct Popup<'a> {
    items: &'a [Item<'a>],
    dark: bool,
    hover: Option<usize>,
    keyboard: bool,
    last_mouse: POINT,
    chosen: Option<usize>,
    done: bool,
}

impl Popup<'_> {
    fn active(&self, slot: usize) -> bool {
        self.items
            .get(slot)
            .is_some_and(|item| item.enabled && item.id != 0)
    }
    fn choose(&mut self) {
        if let Some(slot) = self.hover.filter(|&index| self.active(index)) {
            self.chosen = Some(self.items[slot].id);
        }
    }
    fn row(&self, x: i32, y: i32) -> Option<usize> {
        if !(0..260).contains(&x) || y < 6 {
            return None;
        }
        let row = ((y - 6) / 34) as usize;
        (row < self.items.len()).then_some(row)
    }
}

unsafe extern "system" fn dispatch(
    window: HWND,
    message: u32,
    key: WPARAM,
    extra: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(extra as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
    }
    let Some(cell) = (unsafe {
        (GetWindowLongPtrW(window, GWLP_USERDATA) as *const std::cell::RefCell<Popup<'_>>).as_ref()
    }) else {
        return unsafe { DefWindowProcW(window, message, key, extra) };
    };
    let Ok(mut popup) = cell.try_borrow_mut() else {
        return unsafe { DefWindowProcW(window, message, key, extra) };
    };
    let scale = dpi_scale(window);
    let left = ((extra as u16 as i16) as f32 / scale).floor() as i32;
    let top = (((extra >> 16) as u16 as i16) as f32 / scale).floor() as i32;
    match message {
        WM_ERASEBKGND | WM_NCACTIVATE => return 1,
        WM_NCCALCSIZE if key != 0 => return 0,
        WM_NCPAINT => return 0,
        WM_PAINT => {
            unsafe {
                paint(window, &popup);
            }
            return 0;
        }
        WM_MOUSEMOVE => {
            if popup.keyboard && popup.last_mouse.x == left && popup.last_mouse.y == top {
                return 0;
            }
            popup.keyboard = false;
            popup.last_mouse = POINT { x: left, y: top };
            popup.hover = popup.row(left, top);
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            popup.hover = popup.row(left, top);
            if popup.hover.is_none() || popup.hover.is_some_and(|slot| popup.active(slot)) {
                popup.choose();
                drop(popup);
                unsafe {
                    DestroyWindow(window);
                }
                return 0;
            }
        }
        WM_KEYDOWN => {
            popup.keyboard = true;
            match key as u16 {
                VK_ESCAPE => {
                    drop(popup);
                    unsafe {
                        DestroyWindow(window);
                    }
                    return 0;
                }
                VK_RETURN | VK_SPACE => {
                    popup.choose();
                    if popup.chosen.is_some() {
                        drop(popup);
                        unsafe {
                            DestroyWindow(window);
                        }
                    }
                    return 0;
                }
                VK_HOME => popup.hover = (0..popup.items.len()).find(|&index| popup.active(index)),
                VK_END => {
                    popup.hover = (0..popup.items.len())
                        .rev()
                        .find(|&index| popup.active(index))
                }
                VK_UP | VK_DOWN => {
                    let count = popup.items.len();
                    let up = key as u16 == VK_UP;
                    let start = popup.hover.unwrap_or(if up { 0 } else { count - 1 });
                    popup.hover = (1..=count)
                        .map(|offset| {
                            if up {
                                (start + count - offset) % count
                            } else {
                                (start + offset) % count
                            }
                        })
                        .find(|&index| popup.active(index));
                }
                _ => return unsafe { DefWindowProcW(window, message, key, extra) },
            }
        }
        WM_KILLFOCUS | WM_CANCELMODE | WM_CLOSE => {
            drop(popup);
            unsafe {
                DestroyWindow(window);
            }
            return 0;
        }
        WM_DESTROY => {
            popup.done = true;
            drop(popup);
            if unsafe { GetCapture() } == window {
                unsafe {
                    ReleaseCapture();
                }
            }
            unsafe {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            }
            return 0;
        }
        _ => return unsafe { DefWindowProcW(window, message, key, extra) },
    }
    unsafe {
        InvalidateRect(window, null(), 0);
    }
    0
}

unsafe fn paint(window: HWND, popup: &Popup<'_>) {
    let mut data = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut data) };
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(window, &mut bounds);
    }
    let dpi = window_dpi(window);
    if let Ok((_, target)) = open_target_at_dpi(dc, bounds, dpi) {
        let (bg, fg, muted, card, line, selected) = if popup.dark {
            (0x08090b, 0xeeeeef, 0x96979d, 0x111215, 0x292a2e, 0x202125)
        } else {
            (0xffffff, 0x242628, 0x73767b, 0xfcfcfd, 0xe6e7e9, 0xededef)
        };
        if let Ok(brush) = unsafe { target.CreateSolidColorBrush(&rgb(card), None) } {
            let canvas = Canvas {
                target: &target,
                brush,
            };
            unsafe {
                target.BeginDraw();
                target.Clear(Some(&rgb(bg)));
            }
            let width = bounds.right as f32 * 96.0 / dpi;
            canvas.rounded(
                rect(
                    0.5,
                    0.5,
                    width - 0.5,
                    bounds.bottom as f32 * 96.0 / dpi - 0.5,
                ),
                9.0,
                card,
                Some(line),
            );
            let _ = use_fonts(|fonts| {
                for (index, item) in popup.items.iter().enumerate() {
                    let y = 6.0 + index as f32 * 34.0;
                    if item.id == 0 {
                        canvas.line(12.0, y + 17.0, width - 12.0, y + 17.0, line, 1.0);
                        continue;
                    }
                    if popup.hover == Some(index) && item.enabled {
                        canvas.rounded(rect(5.0, y, width - 5.0, y + 34.0), 6.0, selected, None);
                    }
                    if item.checked {
                        canvas.line(13.0, y + 17.0, 17.0, y + 21.0, fg, 1.7);
                        canvas.line(17.0, y + 21.0, 24.0, y + 13.0, fg, 1.7);
                    }
                    canvas.label(
                        fonts,
                        item.title,
                        rect(34.0, y, width - 12.0, y + 34.0),
                        13,
                        false,
                        if item.enabled { fg } else { muted },
                        false,
                    )?;
                }
                Ok(())
            });
            let _ = unsafe { target.EndDraw(None, None) };
        }
    }
    unsafe {
        EndPaint(window, &data);
    }
}

pub fn show(owner: HWND, x: i32, y: i32, items: &[Item<'_>], dark: bool) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let popup = std::cell::RefCell::new(Popup {
        items,
        dark,
        hover: items.iter().position(|item| item.checked),
        keyboard: false,
        last_mouse: POINT::default(),
        chosen: None,
        done: false,
    });
    let scale = dpi_scale(owner);
    let width = (260.0 * scale).round() as i32;
    let height = ((12 + 34 * items.len()) as f32 * scale).round() as i32;
    let point = POINT { x, y };
    unsafe {
        let module = GetModuleHandleW(null());
        RegisterClassW(&WNDCLASSW {
            hInstance: module,
            lpfnWndProc: Some(dispatch),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..Default::default()
        });
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let has_work = GetMonitorInfoW(
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST),
            &mut monitor,
        ) != 0;
        let (x, y) = if has_work {
            let work = monitor.rcWork;
            (
                x.clamp(work.left, (work.right - width).max(work.left)),
                y.clamp(work.top, (work.bottom - height).max(work.top)),
            )
        } else {
            (x, y)
        };
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            CLASS,
            windows_sys::w!(""),
            WS_POPUP,
            x,
            y,
            width,
            height,
            owner,
            null_mut(),
            module,
            (&popup as *const std::cell::RefCell<Popup<'_>>)
                .cast_mut()
                .cast::<c_void>(),
        );
        if window.is_null() {
            return None;
        }
        let corners = 2u32;
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            window,
            33,
            (&corners as *const u32).cast(),
            4,
        );
        let mut mouse = POINT::default();
        GetCursorPos(&mut mouse);
        ScreenToClient(window, &mut mouse);
        popup.borrow_mut().last_mouse = POINT {
            x: (mouse.x as f32 / scale).floor() as i32,
            y: (mouse.y as f32 / scale).floor() as i32,
        };
        ShowWindow(window, SW_SHOW);
        SetFocus(window);
        SetCapture(window);
        let mut message = MSG::default();
        while !popup.borrow().done {
            let response = GetMessageW(&mut message, null_mut(), 0, 0);
            if response <= 0 {
                if response == 0 {
                    PostQuitMessage(message.wParam as i32);
                }
                break;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if !popup.borrow().done {
            DestroyWindow(window);
        }
        SetFocus(owner);
    }
    popup.into_inner().chosen
}
