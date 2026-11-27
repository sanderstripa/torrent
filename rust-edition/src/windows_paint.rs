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
    #[cfg(feature = "rqbit")]
    {
        unsafe {
            fill(dc, &area, if dark { 0x08090b } else { 0xffffff });
        }
        return;
    }
    #[cfg(not(feature = "rqbit"))]
    let (surface, border) = if dark {
        (0x202124, 0x303234)
    } else {
        (0xfcfcfd, 0xdadbd9)
    };
    #[cfg(not(feature = "rqbit"))]
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
    #[cfg(feature = "rqbit")]
    let (surface, text) = if dark {
        (0x08090b, 0xeeeeef)
    } else {
        (0xffffff, 0x242628)
    };
    #[cfg(not(feature = "rqbit"))]
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

#[cfg(not(feature = "rqbit"))]
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

#[cfg(not(feature = "rqbit"))]
struct Offscreen {
    dc: HDC,
    bitmap: HBITMAP,
    original: HGDIOBJ,
}

#[cfg(not(feature = "rqbit"))]
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

#[cfg(not(feature = "rqbit"))]
impl Drop for Offscreen {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.original);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

#[cfg(not(feature = "rqbit"))]
pub struct SettingsBackdrop {
    preview: Offscreen,
    width: i32,
    height: i32,
}

#[cfg(not(feature = "rqbit"))]
impl SettingsBackdrop {
    pub fn capture(
        owner: windows_sys::Win32::Foundation::HWND,
        width: i32,
        height: i32,
        _dark: bool,
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

    pub unsafe fn paint(&self, destination: HDC, rect: RECT, _panel: Option<RECT>) {
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

#[cfg(feature = "rqbit")]
pub struct SettingsBackdrop {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    dark: bool,
    scale: f32,
}

#[cfg(feature = "rqbit")]
impl SettingsBackdrop {
    pub fn capture(
        window: windows_sys::Win32::Foundation::HWND,
        width: i32,
        height: i32,
        dark: bool,
    ) -> Option<Self> {
        let width_pixels = usize::try_from(width).ok()?.max(1);
        let height_pixels = usize::try_from(height).ok()?.max(1);
        let length = width_pixels.checked_mul(height_pixels)?.checked_mul(4)?;
        let mut info = BITMAPINFO::default();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut data = null_mut();
        let mut pixels = unsafe {
            let _ = windows_sys::Win32::Graphics::Dwm::DwmFlush();
            let dc = GetDC(window);
            if dc.is_null() {
                return None;
            }
            let memory = CreateCompatibleDC(dc);
            if memory.is_null() {
                ReleaseDC(window, dc);
                return None;
            }
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut data, null_mut(), 0);
            if bitmap.is_null() || data.is_null() {
                if !bitmap.is_null() {
                    DeleteObject(bitmap);
                }
                DeleteDC(memory);
                ReleaseDC(window, dc);
                return None;
            }
            let original = SelectObject(memory, bitmap);
            let copied = BitBlt(memory, 0, 0, width, height, dc, 0, 0, SRCCOPY) != 0;
            let pixels =
                copied.then(|| std::slice::from_raw_parts(data.cast::<u8>(), length).to_vec());
            SelectObject(memory, original);
            DeleteObject(bitmap);
            DeleteDC(memory);
            ReleaseDC(window, dc);
            pixels?
        };
        let mut scratch = vec![0; length];
        let scale = crate::windows_canvas::dpi_scale(window);
        let radius = ((7.0 * scale) as usize).max(3);
        for _ in 0..3 {
            for vertical in [false, true] {
                let (lines, size) = if vertical {
                    (width_pixels, height_pixels)
                } else {
                    (height_pixels, width_pixels)
                };
                for line in 0..lines {
                    for channel in 0..3 {
                        let index = |position: usize| {
                            let (x, y) = if vertical {
                                (line, position)
                            } else {
                                (position, line)
                            };
                            (y * width_pixels + x) * 4 + channel
                        };
                        let mut sum = u32::from(pixels[index(0)]) * (radius as u32 + 1);
                        for i in 1..=radius {
                            sum += u32::from(pixels[index(i.min(size - 1))]);
                        }
                        for position in 0..size {
                            scratch[index(position)] = (sum / (radius as u32 * 2 + 1)) as u8;
                            sum -= u32::from(pixels[index(position.saturating_sub(radius))]);
                            sum += u32::from(pixels[index((position + radius + 1).min(size - 1))]);
                        }
                    }
                }
                std::mem::swap(&mut pixels, &mut scratch);
            }
        }
        let (original, tint, overlay) = if dark {
            (0.55, 13.0, 0.45)
        } else {
            (0.48, 248.0, 0.52)
        };
        for pixel in pixels.chunks_exact_mut(4) {
            for channel in pixel.iter_mut().take(3) {
                *channel = (f32::from(*channel) * original + tint * overlay) as u8;
            }
            pixel[3] = 255;
        }
        Some(Self {
            pixels,
            width: width_pixels,
            height: height_pixels,
            dark,
            scale,
        })
    }

    pub unsafe fn paint(&self, destination: HDC, bounds: RECT, panel: Option<RECT>) {
        use crate::windows_canvas::{open_target, rect, rgb, Canvas};
        use windows::Win32::Graphics::Direct2D::Common::{
            D2D1_ALPHA_MODE_IGNORE, D2D1_PIXEL_FORMAT, D2D_SIZE_U,
        };
        use windows::Win32::Graphics::Direct2D::{
            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
        let Ok((_, target)) = open_target(destination, bounds) else {
            return;
        };
        unsafe {
            let Ok(bitmap) = target.CreateBitmap(
                D2D_SIZE_U {
                    width: self.width as u32,
                    height: self.height as u32,
                },
                Some(self.pixels.as_ptr().cast()),
                self.width as u32 * 4,
                &D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                },
            ) else {
                return;
            };
            target.BeginDraw();
            target.DrawBitmap(
                &bitmap,
                Some(&rect(0.0, 0.0, bounds.right as f32, bounds.bottom as f32)),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            );
            if let (Some(panel), Ok(brush)) = (panel, target.CreateSolidColorBrush(&rgb(0), None)) {
                let canvas = Canvas {
                    target: &target,
                    brush,
                };
                canvas
                    .brush
                    .SetOpacity(if self.dark { 0.012 } else { 0.009 });
                for layer in (1..=24).rev() {
                    let layer = layer as f32 * self.scale;
                    canvas.rounded(
                        rect(
                            panel.left as f32 - layer * 0.7,
                            panel.top as f32 + 8.0 * self.scale,
                            panel.right as f32 + layer * 0.7,
                            panel.bottom as f32 + 6.0 * self.scale + layer,
                        ),
                        14.0 * self.scale,
                        0,
                        None,
                    );
                }
            }
            let _ = target.EndDraw(None, None);
        }
    }
}
