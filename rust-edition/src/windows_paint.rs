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
    pub fn heading(&self) -> HFONT {
        self.title
    }

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
    if value.is_empty() {
        return;
    }
    let text: Vec<_> = value.encode_utf16().chain(Some(0)).collect();
    let old = unsafe { SelectObject(dc, font) };
    unsafe {
        SetTextColor(dc, color(rgb));
        SetBkMode(dc, TRANSPARENT as i32);
        DrawTextW(
            dc,
            text.as_ptr(),
            (text.len() - 1) as i32,
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
pub unsafe fn transfer(draw: &DRAWITEMSTRUCT, row: &TransferCard) {
    let Some(fonts) = (unsafe { row.fonts.as_ref() }) else {
        return;
    };
    crate::windows_app::trace_ui("painting queue card");
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
        crate::windows_app::trace_ui("painted queue card background");
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

pub unsafe fn background(dc: HDC, area: RECT, dark: bool) {
    let (surface, border) = if dark {
        (0x202124, 0x303234)
    } else {
        (0xfcfcfd, 0xdadbd9)
    };
    unsafe {
        fill(dc, &area, surface);
        fill(
            dc,
            &RECT {
                top: 61,
                bottom: 62,
                ..area
            },
            border,
        );
    }
}

pub unsafe fn control_background(dc: HDC, dark: bool) -> HBRUSH {
    let (surface, text) = if dark {
        (0x202124, 0xe6e7e9)
    } else {
        (0xfcfcfd, 0x17191c)
    };
    unsafe {
        SetTextColor(dc, color(text));
        SetBkColor(dc, color(surface));
        SetDCBrushColor(dc, color(surface));
        GetStockObject(DC_BRUSH)
    }
}

pub unsafe fn caption_button(draw: &DRAWITEMSTRUCT, text: &str, dark: bool) {
    let (surface, ink, pressed) = if dark {
        (0x202124, 0x96979d, 0x303136)
    } else {
        (0xfcfcfd, 0x73767b, 0xeeeeef)
    };
    let area = draw.rcItem;
    let font = unsafe {
        CreateFontW(
            -19,
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
            windows_sys::w!("Segoe UI Symbol"),
        )
    };
    unsafe {
        fill(draw.hDC, &area, surface);
        let active = draw.itemState & (1 | 16) != 0;
        if active {
            rounded(draw.hDC, &area, pressed, pressed, 32);
        }
        let old = SelectObject(draw.hDC, font);
        let mut area = area;
        let text: Vec<_> = text.encode_utf16().collect();
        SetTextColor(draw.hDC, color(ink));
        SetBkMode(draw.hDC, TRANSPARENT as i32);
        DrawTextW(
            draw.hDC,
            text.as_ptr(),
            text.len() as i32,
            &mut area,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        if !old.is_null() {
            SelectObject(draw.hDC, old);
        }
        DeleteObject(font);
    }
}

struct Offscreen {
    dc: HDC,
    bitmap: HBITMAP,
    original: HGDIOBJ,
}

impl Offscreen {
    unsafe fn new(reference: HDC, width: i32, height: i32) -> Option<Self> {
        let dc = unsafe { CreateCompatibleDC(reference) };
        if dc.is_null() {
            return None;
        }
        let bitmap = unsafe { CreateCompatibleBitmap(reference, width, height) };
        if bitmap.is_null() {
            unsafe { DeleteDC(dc) };
            return None;
        }
        let original = unsafe { SelectObject(dc, bitmap) };
        if original.is_null() {
            unsafe {
                DeleteObject(bitmap);
                DeleteDC(dc);
            }
            return None;
        }
        Some(Self {
            dc,
            bitmap,
            original,
        })
    }
}

impl Drop for Offscreen {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.original);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

pub struct SettingsBackdrop {
    preview: Offscreen,
    width: i32,
    height: i32,
}

impl SettingsBackdrop {
    pub fn capture(
        owner: windows_sys::Win32::Foundation::HWND,
        width: i32,
        height: i32,
    ) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let reference = unsafe { GetWindowDC(owner) };
        if reference.is_null() {
            return None;
        }
        let small_width = (width / 16).max(1);
        let small_height = (height / 16).max(1);
        let snapshot = unsafe { Offscreen::new(reference, width, height) };
        let preview = unsafe { Offscreen::new(reference, small_width, small_height) };
        unsafe { ReleaseDC(owner, reference) };
        let (snapshot, preview) = (snapshot?, preview?);
        let valid = unsafe {
            windows_sys::Win32::Storage::Xps::PrintWindow(
                owner,
                snapshot.dc,
                windows_sys::Win32::UI::WindowsAndMessaging::PW_RENDERFULLCONTENT,
            )
        };
        if valid == 0 {
            return None;
        }
        unsafe {
            SetStretchBltMode(preview.dc, HALFTONE);
            if StretchBlt(
                preview.dc,
                0,
                0,
                small_width,
                small_height,
                snapshot.dc,
                0,
                0,
                width,
                height,
                SRCCOPY,
            ) == 0
            {
                return None;
            }
        }
        Some(Self {
            preview,
            width: small_width,
            height: small_height,
        })
    }

    pub unsafe fn paint(&self, destination: HDC, rect: RECT) {
        unsafe {
            let preserve = SaveDC(destination);
            SetStretchBltMode(destination, HALFTONE);
            StretchBlt(
                destination,
                0,
                0,
                rect.right,
                rect.bottom,
                self.preview.dc,
                0,
                0,
                self.width,
                self.height,
                SRCCOPY,
            );
            RestoreDC(destination, preserve);
        }
    }
}
