use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::HDC;
use windows_numerics::Vector2;

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
    pub(crate) factory: IDWriteFactory,
    loader: IDWriteInMemoryFontFileLoader,
    collection: IDWriteFontCollection,
    formats: std::cell::RefCell<std::collections::HashMap<(u16, bool), IDWriteTextFormat>>,
}

impl Fonts {
    pub(crate) fn load(bytes: &'static [u8]) -> windows::core::Result<Self> {
        use windows::core::Interface;
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

    pub(crate) fn fresh_format(
        &self,
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
    ) -> windows::core::Result<IDWriteTextFormat> {
        unsafe {
            self.factory.CreateTextFormat(
                windows::core::w!("Montserrat"),
                &self.collection,
                weight,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                windows::core::w!(""),
            )
        }
    }

    pub(crate) fn format(
        &self,
        size: u16,
        medium: bool,
    ) -> windows::core::Result<IDWriteTextFormat> {
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
            let _ = cache.set(Fonts::load(include_bytes!(
                "../../assets/fonts/Montserrat.ttf"
            ))?);
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

    pub(crate) fn outline(&self, rect: D2D_RECT_F, radius: f32, stroke: u32, width: f32) {
        unsafe {
            self.brush.SetColor(&rgb(stroke));
            self.target.DrawRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect,
                    radiusX: radius,
                    radiusY: radius,
                },
                &self.brush,
                width,
                None,
            );
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
    pub(crate) fn success(&self, x: f32, y: f32, fill: u32, stroke: u32) {
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

pub(crate) fn rgb(color: u32) -> D2D1_COLOR_F {
    let component = |shift| ((color >> shift) & 0xff_u32) as f32 / 255.0;
    D2D1_COLOR_F {
        r: component(16),
        g: component(8),
        b: component(0),
        a: 1.0,
    }
}
