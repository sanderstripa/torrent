use crate::windows_app::ACTION;
use crate::windows_canvas::{
    dpi_scale, open_target_at_dpi, rect, rgb, use_fonts, window_dpi, Canvas,
};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const PROPERTY: windows_sys::core::PCWSTR = windows_sys::w!("TorrentPrompt144");
pub const WIDTH: i32 = 490;
pub const HEIGHT: i32 = 215;
pub const EDIT: usize = 10;
pub const APPLY: usize = 1;
pub const CANCEL: usize = 2;

pub struct Prompt<'a> {
    title: &'a str,
    help: &'a str,
    english: bool,
    dark: bool,
    error: bool,
    brush: HBRUSH,
}

impl<'a> Prompt<'a> {
    pub fn new(title: &'a str, help: &'a str, english: bool, dark: bool, error: bool) -> Self {
        let shade = if dark { 16 } else { 250 };
        Self {
            title,
            help,
            english,
            dark,
            error,
            brush: unsafe { CreateSolidBrush(shade * 0x010101) },
        }
    }
}

impl Drop for Prompt<'_> {
    fn drop(&mut self) {
        unsafe { DeleteObject(self.brush) };
    }
}

fn button(id: usize) -> D2D_RECT_F {
    let x = WIDTH as f32 - if id == APPLY { 128.0 } else { 240.0 };
    rect(x, 161.0, x + 104.0, 197.0)
}

pub unsafe fn dispatch(window: HWND, message: u32, key: WPARAM, data: LPARAM) -> Option<LRESULT> {
    let state = unsafe { GetPropW(window, PROPERTY).cast::<Prompt<'_>>().as_ref()? };
    match message {
        WM_ERASEBKGND | WM_NCACTIVATE => Some(1),
        WM_NCPAINT => Some(0),
        WM_NCCALCSIZE if key != 0 => Some(0),
        WM_CTLCOLOREDIT => {
            let dc = key as HDC;
            unsafe {
                SetTextColor(dc, if state.dark { 0xeeeeee } else { 0x282624 });
                SetBkColor(dc, if state.dark { 0x101010 } else { 0xfafafa });
            }
            Some(state.brush as LRESULT)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let mut bounds = RECT::default();
            unsafe {
                let dc = BeginPaint(window, &mut paint);
                GetClientRect(window, &mut bounds);
                let _ = draw(window, dc, bounds, state);
                EndPaint(window, &paint);
            }
            Some(0)
        }
        WM_MOUSEMOVE | WM_COMMAND if message == WM_MOUSEMOVE || key >> 16 != 0 => {
            unsafe { InvalidateRect(window, std::ptr::null(), 0) };
            Some(0)
        }
        WM_LBUTTONUP => {
            let scale = dpi_scale(window);
            let x = data as i16 as f32 / scale;
            let y = (data >> 16) as i16 as f32 / scale;
            let hit = |r: D2D_RECT_F| x >= r.left && x < r.right && y >= r.top && y < r.bottom;
            let id = if x > WIDTH as f32 - 46.0 && y < 50.0 {
                CANCEL
            } else if hit(button(APPLY)) {
                APPLY
            } else if !state.error && hit(button(CANCEL)) {
                CANCEL
            } else {
                return Some(0);
            };
            unsafe { PostMessageW(window, ACTION, id, 0) };
            Some(0)
        }
        WM_CLOSE => {
            unsafe { PostMessageW(window, ACTION, CANCEL, 0) };
            Some(0)
        }
        _ => None,
    }
}

fn draw(window: HWND, dc: HDC, bounds: RECT, state: &Prompt<'_>) -> windows::core::Result<()> {
    let (_, target) = open_target_at_dpi(dc, bounds, window_dpi(window))?;
    let (bg, fg, muted, card, line) = if state.dark {
        (0x08090b, 0xeeeeef, 0x96979d, 0x111215, 0x292a2e)
    } else {
        (0xffffff, 0x242628, 0x73767b, 0xfcfcfd, 0xe6e7e9)
    };
    let brush = unsafe { target.CreateSolidColorBrush(&rgb(fg), None)? };
    let canvas = Canvas {
        target: &target,
        brush,
    };
    let width = WIDTH as f32;
    unsafe {
        target.BeginDraw();
        target.Clear(Some(&rgb(bg)));
    }
    let result = use_fonts(|fonts| {
        canvas.tracked_label(
            fonts,
            &state.title.to_uppercase(),
            rect(24.0, 14.0, width - 65.0, 50.0),
            18,
            fg,
            1.6,
        )?;
        canvas.line(width - 31.0, 24.0, width - 21.0, 34.0, muted, 1.5);
        canvas.line(width - 21.0, 24.0, width - 31.0, 34.0, muted, 1.5);
        if state.error {
            canvas.label(
                fonts,
                "!",
                rect(24.0, 65.0, 45.0, 90.0),
                18,
                true,
                0xc95050,
                false,
            )?;
            canvas.label(
                fonts,
                state.help,
                rect(56.0, 67.0, width - 24.0, 145.0),
                13,
                false,
                fg,
                false,
            )?;
        } else {
            canvas.label(
                fonts,
                state.help,
                rect(24.0, 62.0, width - 24.0, 94.0),
                13,
                false,
                muted,
                false,
            )?;
            let edit = rect(24.0, 102.0, width - 24.0, 138.0);
            canvas.rounded(edit, 8.0, card, Some(line));
            if unsafe { GetFocus() == GetDlgItem(window, EDIT as i32) } {
                canvas.outline(edit, 8.0, muted, 1.0);
            }
            canvas.rounded(button(CANCEL), 8.0, bg, Some(line));
            canvas.centered(
                fonts,
                if state.english {
                    "Cancel"
                } else {
                    "Отмена"
                },
                button(CANCEL),
                13,
                fg,
            )?;
        }
        canvas.rounded(button(APPLY), 8.0, fg, None);
        canvas.centered(
            fonts,
            if state.error {
                "OK"
            } else if state.english {
                "Apply"
            } else {
                "Применить"
            },
            button(APPLY),
            13,
            bg,
        )
    });
    let end = unsafe { target.EndDraw(None, None) };
    result.and(end)
}
