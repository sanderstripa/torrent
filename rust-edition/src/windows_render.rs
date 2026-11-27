use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;
use windows_numerics::Vector2;

#[derive(Clone, Copy)]
pub enum Icon {
    Add,
    Minimize,
    Close,
    Vpn(u8),
}

pub fn draw(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    area: windows_sys::Win32::Foundation::RECT,
    dark: bool,
    active: bool,
    icon: Icon,
) -> windows::core::Result<()> {
    let width = (area.right - area.left).max(1) as f32;
    let height = (area.bottom - area.top).max(1) as f32;
    let background = if dark { 0x08090b } else { 0xffffff };
    let selected = if dark { 0x202125 } else { 0xededef };
    let foreground = if dark { 0xeeeeef } else { 0x242628 };
    let tile = if active {
        if matches!(icon, Icon::Close) {
            if dark {
                0x54242f
            } else {
                0xf6d1d9
            }
        } else {
            selected
        }
    } else {
        background
    };
    let ink = match icon {
        Icon::Vpn(2) => {
            if dark {
                0x57d6a3
            } else {
                0x087f5b
            }
        }
        Icon::Vpn(3) => 0xc95050,
        _ => foreground,
    };
    unsafe {
        let (factory, target) = open_target(dc, area)?;
        let brush = target.CreateSolidColorBrush(&rgb(tile), None)?;
        let scale = width.min(height) / 32.0;
        let point = |x: f32, y: f32| Vector2 {
            X: width / 2.0 + x * scale,
            Y: height / 2.0 + y * scale,
        };
        let line =
            |x, y, a, b| target.DrawLine(point(x, y), point(a, b), &brush, 1.25 * scale, None);

        target.BeginDraw();
        target.Clear(Some(&rgb(background)));
        target.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: width,
                    bottom: height,
                },
                radiusX: 16.0 * scale,
                radiusY: 16.0 * scale,
            },
            &brush,
        );
        brush.SetColor(&rgb(ink));
        let geometry = match icon {
            Icon::Add => {
                line(-6.0, 0.0, 6.0, 0.0);
                line(0.0, -6.0, 0.0, 6.0);
                Ok(())
            }
            Icon::Minimize => {
                line(-6.0, 0.0, 6.0, 0.0);
                Ok(())
            }
            Icon::Close => {
                line(-5.0, -5.0, 5.0, 5.0);
                line(5.0, -5.0, -5.0, 5.0);
                Ok(())
            }
            Icon::Vpn(_) => (|| {
                let path = factory.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.BeginFigure(point(-3.5, -4.0), D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: point(-3.5, 4.0),
                    size: D2D_SIZE_F {
                        width: 5.5 * scale,
                        height: 5.5 * scale,
                    },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_LARGE,
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                target.DrawGeometry(&path, &brush, 1.25 * scale, None);
                line(-6.0, 0.0, -1.0, 0.0);
                target.FillEllipse(
                    &D2D1_ELLIPSE {
                        point: point(-1.0, 0.0),
                        radiusX: 0.75 * scale,
                        radiusY: 0.75 * scale,
                    },
                    &brush,
                );
                Ok(())
            })(),
        };
        let result = target.EndDraw(None, None);
        geometry.and(result)
    }
}

pub(crate) fn open_target(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    area: windows_sys::Win32::Foundation::RECT,
) -> windows::core::Result<(ID2D1Factory, ID2D1DCRenderTarget)> {
    let rect = RECT {
        left: area.left,
        top: area.top,
        right: area.right,
        bottom: area.bottom,
    };
    unsafe {
        let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let target = factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: 96.0,
            dpiY: 96.0,
            ..Default::default()
        })?;
        target.BindDC(HDC(dc), &rect)?;
        Ok((factory, target))
    }
}

pub(crate) struct Fonts {
    factory: IDWriteFactory,
    loader: IDWriteInMemoryFontFileLoader,
    collection: IDWriteFontCollection,
    formats: std::cell::RefCell<std::collections::HashMap<(u16, bool), IDWriteTextFormat>>,
}

impl Fonts {
    fn load() -> windows::core::Result<Self> {
        use windows::core::Interface;
        let bytes = include_bytes!("../../assets/fonts/Montserrat.ttf");
        unsafe {
            let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let factory5: IDWriteFactory5 = factory.cast()?;
            let loader = factory5.CreateInMemoryFontFileLoader()?;
            factory.RegisterFontFileLoader(&loader)?;
            let result = (|| {
                let font = loader.CreateInMemoryFontFileReference(
                    &factory,
                    bytes.as_ptr().cast(),
                    bytes.len() as u32,
                    None,
                )?;
                let builder = factory5.CreateFontSetBuilder()?;
                builder.AddFontFile(&font)?;
                let fonts = builder.CreateFontSet()?;
                factory5.CreateFontCollectionFromFontSet(&fonts)?.cast()
            })();
            match result {
                Ok(collection) => Ok(Self {
                    factory,
                    loader,
                    collection,
                    formats: Default::default(),
                }),
                Err(error) => {
                    let _ = factory.UnregisterFontFileLoader(&loader);
                    Err(error)
                }
            }
        }
    }

    fn format(&self, size: u16, medium: bool) -> windows::core::Result<IDWriteTextFormat> {
        let mut formats = self.formats.borrow_mut();
        if let Some(format) = formats.get(&(size, medium)) {
            return Ok(format.clone());
        }
        unsafe {
            let format = self.factory.CreateTextFormat(
                windows::core::w!("Montserrat"),
                &self.collection,
                if medium {
                    DWRITE_FONT_WEIGHT_MEDIUM
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                },
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                f32::from(size),
                windows::core::w!(""),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let ellipsis = self.factory.CreateEllipsisTrimmingSign(&format)?;
            format.SetTrimming(
                &DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    ..Default::default()
                },
                &ellipsis,
            )?;
            formats.insert((size, medium), format.clone());
            Ok(format)
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            let _ = self.factory.UnregisterFontFileLoader(&self.loader);
        }
    }
}

thread_local! {
    static FONTS: std::cell::OnceCell<Fonts> = const { std::cell::OnceCell::new() };
}

pub(crate) fn use_fonts<T>(
    draw: impl FnOnce(&Fonts) -> windows::core::Result<T>,
) -> windows::core::Result<T> {
    FONTS.with(|cache| {
        if cache.get().is_none() {
            let _ = cache.set(Fonts::load()?);
        }
        draw(cache.get().expect("DirectWrite fonts are initialized"))
    })
}

pub(crate) fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}

pub(crate) struct Canvas<'a> {
    pub(crate) target: &'a ID2D1DCRenderTarget,
    pub(crate) brush: ID2D1SolidColorBrush,
}

impl Canvas<'_> {
    pub(crate) fn rounded(&self, rect: D2D_RECT_F, radius: f32, fill: u32, border: Option<u32>) {
        let shape = D2D1_ROUNDED_RECT {
            rect,
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.brush.SetColor(&rgb(fill));
            self.target.FillRoundedRectangle(&shape, &self.brush);
            if let Some(border) = border {
                self.brush.SetColor(&rgb(border));
                self.target
                    .DrawRoundedRectangle(&shape, &self.brush, 1.0, None);
            }
        }
    }

    pub(crate) fn label(
        &self,
        fonts: &Fonts,
        value: &str,
        rect: D2D_RECT_F,
        size: u16,
        medium: bool,
        color: u32,
        trailing: bool,
    ) -> windows::core::Result<()> {
        let format = fonts.format(size, medium)?;
        unsafe {
            format.SetTextAlignment(if trailing {
                DWRITE_TEXT_ALIGNMENT_TRAILING
            } else {
                DWRITE_TEXT_ALIGNMENT_LEADING
            })?;
            self.brush.SetColor(&rgb(color));
            let encoded: Vec<_> = value.encode_utf16().collect();
            self.target.DrawText(
                &encoded,
                &format,
                &rect,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }

    pub(crate) fn line(&self, x: f32, y: f32, a: f32, b: f32, color: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&rgb(color));
            self.target.DrawLine(
                Vector2 { X: x, Y: y },
                Vector2 { X: a, Y: b },
                &self.brush,
                width,
                None,
            );
        }
    }
    pub(crate) fn circle(&self, x: f32, y: f32, radius: f32, color: u32, filled: bool) {
        let shape = D2D1_ELLIPSE {
            point: Vector2 { X: x, Y: y },
            radiusX: radius,
            radiusY: radius,
        };
        unsafe {
            self.brush.SetColor(&rgb(color));
            if filled {
                self.target.FillEllipse(&shape, &self.brush);
            } else {
                self.target.DrawEllipse(&shape, &self.brush, 1.35, None);
            }
        }
    }
    pub(crate) fn centered(
        &self,
        fonts: &Fonts,
        text: &str,
        rect: D2D_RECT_F,
        size: u16,
        color: u32,
    ) -> windows::core::Result<()> {
        let format = fonts.format(size, false)?;
        unsafe {
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            self.brush.SetColor(&rgb(color));
            self.target.DrawText(
                &text.encode_utf16().collect::<Vec<_>>(),
                &format,
                &rect,
                &self.brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
                DWRITE_MEASURING_MODE_NATURAL,
            );
        }
        Ok(())
    }
    fn success(&self, x: f32, y: f32, fill: u32, stroke: u32) {
        unsafe {
            self.brush.SetColor(&rgb(fill));
            self.target.FillEllipse(
                &D2D1_ELLIPSE {
                    point: Vector2 { X: x, Y: y },
                    radiusX: 11.0,
                    radiusY: 11.0,
                },
                &self.brush,
            );
            self.brush.SetColor(&rgb(stroke));
            let line = |a, b, c, d| {
                self.target.DrawLine(
                    Vector2 { X: x + a, Y: y + b },
                    Vector2 { X: x + c, Y: y + d },
                    &self.brush,
                    2.0,
                    None,
                )
            };
            line(-5.0, 0.0, -1.0, 4.0);
            line(-1.0, 4.0, 5.0, -4.0);
        }
    }
}

pub fn card(
    draw: &windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT,
    row: &crate::windows_paint::TransferCard,
) -> windows::core::Result<()> {
    let area = draw.rcItem;
    let width = (area.right - area.left) as f32;
    let selected = draw.itemState & 1 != 0;
    let (background, foreground, muted, body, highlight, border, track) = if row.dark {
        (
            0x08090b, 0xeeeeef, 0x96979d, 0x111215, 0x202125, 0x292a2e, 0x303136,
        )
    } else {
        (
            0xffffff, 0x242628, 0x73767b, 0xfcfcfd, 0xededef, 0xe6e7e9, 0xe6e7e9,
        )
    };
    let (_, target) = open_target(draw.hDC, area)?;
    let brush = unsafe { target.CreateSolidColorBrush(&rgb(foreground), None)? };
    let canvas = Canvas {
        target: &target,
        brush,
    };
    unsafe {
        target.BeginDraw();
        target.Clear(Some(&rgb(background)));
    }
    let result = use_fonts(|fonts| {
        let x = 18.0;
        let y = 2.0;
        let end = width - 18.0;
        canvas.rounded(
            rect(2.0, y, width - 2.0, y + 124.0),
            11.0,
            if selected || row.paused {
                highlight
            } else {
                body
            },
            Some(if selected { foreground } else { border }),
        );
        canvas.label(
            fonts,
            &row.title,
            rect(x, y + 12.0, width - 94.0, y + 42.0),
            17,
            true,
            foreground,
            false,
        )?;
        canvas.label(
            fonts,
            &row.details,
            rect(x, y + 43.0, width - 94.0, y + 68.0),
            15,
            false,
            muted,
            false,
        )?;
        if row.progress_label == "✓" {
            canvas.success(width - 35.0, y + 42.0, foreground, background);
        } else if !row.progress_label.is_empty() {
            canvas.label(
                fonts,
                &row.progress_label,
                rect(width - 77.0, y + 30.0, end, y + 58.0),
                17,
                false,
                foreground,
                true,
            )?;
        }
        canvas.rounded(rect(x, y + 79.0, end, y + 83.0), 4.0, track, None);
        if row.progress > 0 {
            let progress = (row.progress.clamp(0, 1_000_000) as f32) / 1_000_000.0;
            canvas.rounded(
                rect(x, y + 79.0, x + (end - x) * progress, y + 83.0),
                4.0,
                foreground,
                None,
            );
        }
        canvas.label(
            fonts,
            &row.footer,
            rect(width - 294.0, y + 92.0, end, y + 116.0),
            12,
            false,
            muted,
            true,
        )
    });
    let end = unsafe { target.EndDraw(None, None) };
    result.and(end)
}

pub(crate) fn rgb(color: u32) -> D2D1_COLOR_F {
    let component = |shift| ((color >> shift) & 0xff_u32) as f32 / 255.0;
    D2D1_COLOR_F {
        r: component(16),
        g: component(8),
        b: component(0),
        a: 1.0,
    }
}

pub fn title(
    draw: &windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT,
    dark: bool,
) -> windows::core::Result<()> {
    use windows::core::Interface;

    let area = draw.rcItem;
    let (_, target) = open_target(draw.hDC, area)?;
    let background = rgb(if dark { 0x08090b } else { 0xffffff });
    let foreground = rgb(if dark { 0xeeeeef } else { 0x242628 });
    let brush = unsafe { target.CreateSolidColorBrush(&foreground, None)? };
    unsafe {
        target.BeginDraw();
        target.Clear(Some(&background));
    }
    let result = use_fonts(|fonts| {
        let text = "TORRENT.".encode_utf16().collect::<Vec<_>>();
        let format = fonts.format(18, false)?;
        unsafe {
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            let layout = fonts.factory.CreateTextLayout(
                &text,
                &format,
                (area.right - area.left) as f32,
                (area.bottom - area.top) as f32,
            )?;
            let spaced: IDWriteTextLayout1 = layout.cast()?;
            spaced.SetCharacterSpacing(
                0.8,
                0.8,
                0.0,
                DWRITE_TEXT_RANGE {
                    startPosition: 0,
                    length: text.len() as u32,
                },
            )?;
            target.DrawTextLayout(
                Vector2 { X: 0.0, Y: 0.0 },
                &layout,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_CLIP,
            );
        }
        Ok(())
    });
    let end = unsafe { target.EndDraw(None, None) };
    result.and(end)
}
