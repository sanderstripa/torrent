use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HANDLE, RECT};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT;

pub struct Fonts {
    registration: HANDLE,
    title: HFONT,
    normal: HFONT,
    help: HFONT,
}

impl Fonts {
    pub fn new() -> Self {
        let font = include_bytes!("../../assets/fonts/Montserrat.ttf");
        let mut loaded = 0;
        let registration = unsafe {
            AddFontMemResourceEx(
                font.as_ptr().cast(),
                font.len() as u32,
                null_mut(),
                &mut loaded,
            )
        };
        let create = |size: i32| unsafe {
            CreateFontW(
                -size,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                windows_sys::w!("Montserrat"),
            )
        };
        Self {
            registration,
            title: create(18),
            normal: create(15),
            help: create(13),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            for font in [self.title, self.normal, self.help] {
                DeleteObject(font);
            }
            if !self.registration.is_null() {
                RemoveFontMemResourceEx(self.registration);
            }
        }
    }
}

pub struct TransferCard {
    pub title: String,
    pub details: String,
    pub progress_label: String,
    pub footer: String,
    pub progress: i32,
    pub paused: bool,
    pub dark: bool,
    fonts: *const Fonts,
}

impl TransferCard {
    pub fn attach_fonts(&mut self, fonts: &Fonts) {
        self.fonts = fonts;
    }

    pub fn new() -> Self {
        Self {
            title: String::new(),
            details: String::new(),
            progress_label: String::new(),
            footer: String::new(),
            progress: 0,
            paused: false,
            dark: false,
            fonts: std::ptr::null(),
        }
    }
}

fn color(rgb: u32) -> u32 {
    ((rgb & 255) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 255)
}

unsafe fn fill(dc: HDC, area: &RECT, rgb: u32) {
    let brush = unsafe { CreateSolidBrush(color(rgb)) };
    if !brush.is_null() {
        unsafe {
            FillRect(dc, area, brush);
            DeleteObject(brush);
        }
    }
}

unsafe fn rounded(dc: HDC, area: &RECT, background: u32, border: u32, radius: i32) {
    let brush = unsafe { CreateSolidBrush(color(background)) };
    let pen = unsafe { CreatePen(PS_SOLID, 1, color(border)) };
    if brush.is_null() || pen.is_null() {
        unsafe {
            DeleteObject(brush);
            DeleteObject(pen);
        }
        return;
    }
    unsafe {
        let previous_brush = SelectObject(dc, brush);
        let previous_pen = SelectObject(dc, pen);
        RoundRect(
            dc,
            area.left,
            area.top,
            area.right,
            area.bottom,
            radius,
            radius,
        );
        SelectObject(dc, previous_pen);
        SelectObject(dc, previous_brush);
        DeleteObject(pen);
        DeleteObject(brush);
    }
}

unsafe fn label(dc: HDC, mut area: RECT, font: HFONT, rgb: u32, value: &str, right: bool) {
    let text: Vec<_> = value.encode_utf16().collect();
    let old = unsafe { SelectObject(dc, font) };
    unsafe {
        SetTextColor(dc, color(rgb));
        SetBkMode(dc, TRANSPARENT as i32);
        DrawTextW(
            dc,
            text.as_ptr(),
            text.len() as i32,
            &mut area,
            DT_END_ELLIPSIS
                | DT_SINGLELINE
                | DT_VCENTER
                | DT_NOPREFIX
                | if right { DT_RIGHT } else { DT_LEFT },
        );
        if !old.is_null() {
            SelectObject(dc, old);
        }
    }
}

/// Native listbox selection and scrolling are retained. The row contents are
/// painted from a stable snapshot owned by the window's Controls.
pub unsafe fn transfer(draw: &DRAWITEMSTRUCT) {
    if draw.itemID == u32::MAX || draw.itemData == 0 {
        return;
    }
    let row = unsafe { &*(draw.itemData as *const TransferCard) };
    let Some(fonts) = (unsafe { row.fonts.as_ref() }) else {
        return;
    };
    let selected = draw.itemState & 1 != 0;
    let (surface, card, highlight, border, text, muted, track) = if row.dark {
        (
            0x202124, 0x242628, 0x303136, 0x303234, 0xe6e7e9, 0x96979d, 0x303234,
        )
    } else {
        (
            0xfcfcfd, 0xffffff, 0xeeeeef, 0xdadbd9, 0x17191c, 0x73767b, 0xdadbd9,
        )
    };
    let outer = draw.rcItem;
    let card_area = RECT {
        left: outer.left + 2,
        top: outer.top + 2,
        right: outer.right - 2,
        bottom: outer.bottom - 10,
    };
    let x = card_area.left + 16;
    let y = card_area.top;
    let end = (card_area.right - 16).max(x);
    let rect = |left, top, right, bottom| RECT {
        left,
        top,
        right,
        bottom,
    };
    unsafe {
        fill(draw.hDC, &outer, surface);
        rounded(
            draw.hDC,
            &card_area,
            if selected || row.paused {
                highlight
            } else {
                card
            },
            if selected { text } else { border },
            22,
        );
        label(
            draw.hDC,
            rect(x, y + 12, end - 88, y + 42),
            fonts.title,
            text,
            &row.title,
            false,
        );
        label(
            draw.hDC,
            rect(x, y + 43, end - 88, y + 68),
            fonts.normal,
            muted,
            &row.details,
            false,
        );
        label(
            draw.hDC,
            rect(end - 88, y + 30, end, y + 58),
            fonts.title,
            text,
            &row.progress_label,
            true,
        );
        rounded(draw.hDC, &rect(x, y + 79, end, y + 84), track, track, 5);
        let bar =
            ((end - x) as i64 * i64::from(row.progress.clamp(0, 1_000_000)) / 1_000_000) as i32;
        if bar > 0 {
            rounded(draw.hDC, &rect(x, y + 79, x + bar, y + 84), text, text, 5);
        }
        label(
            draw.hDC,
            rect(x, y + 92, end, y + 116),
            fonts.help,
            muted,
            &row.footer,
            true,
        );
    }
}
