use crate::windows_canvas::rect;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;

pub(super) struct Settings {
    pub title: D2D_RECT_F,
    pub back: D2D_RECT_F,
    pub language: D2D_RECT_F,
    pub ru: D2D_RECT_F,
    pub en: D2D_RECT_F,
    pub theme: D2D_RECT_F,
    pub light: D2D_RECT_F,
    pub dark: D2D_RECT_F,
    pub path_label: D2D_RECT_F,
    pub path: D2D_RECT_F,
    pub browse: D2D_RECT_F,
    pub desktop: D2D_RECT_F,
    pub desktop_label: D2D_RECT_F,
    pub menu: D2D_RECT_F,
    pub menu_label: D2D_RECT_F,
    pub accept: D2D_RECT_F,
}

pub(super) fn settings(english: bool) -> Settings {
    if english {
        Settings {
            title: rect(45.0, 67.0, 274.9375, 95.0),
            back: rect(762.53125, 71.25, 795.0, 90.75),
            language: rect(45.0, 122.5, 107.328125, 137.5),
            ru: rect(123.328125, 116.0, 160.0, 144.0),
            en: rect(163.0, 116.0, 199.3125, 144.0),
            theme: rect(279.3125, 122.5, 322.359375, 137.5),
            light: rect(338.359375, 116.0, 387.234375, 144.0),
            dark: rect(390.234375, 116.0, 436.71875, 144.0),
            path_label: rect(45.0, 162.0, 795.0, 177.0),
            path: rect(45.0, 185.0, 702.59375, 220.0),
            browse: rect(712.59375, 185.0, 795.0, 220.0),
            desktop: rect(45.0, 233.0, 795.0, 271.0),
            desktop_label: rect(96.0, 244.0, 209.171875, 260.0),
            menu: rect(45.0, 271.0, 795.0, 309.0),
            menu_label: rect(96.0, 282.0, 228.421875, 298.0),
            accept: rect(45.0, 387.0, 305.0, 436.0),
        }
    } else {
        Settings {
            title: rect(45.0, 67.0, 309.03125, 95.0),
            back: rect(753.75, 71.25, 795.0, 90.75),
            language: rect(45.0, 122.5, 77.828125, 137.5),
            ru: rect(93.828125, 116.0, 130.5, 144.0),
            en: rect(133.5, 116.0, 169.8125, 144.0),
            theme: rect(249.8125, 122.5, 281.078125, 137.5),
            light: rect(297.078125, 116.0, 363.078125, 144.0),
            dark: rect(366.078125, 116.0, 427.9375, 144.0),
            path_label: rect(45.0, 162.0, 795.0, 177.0),
            path: rect(45.0, 185.0, 693.390625, 220.0),
            browse: rect(703.390625, 185.0, 795.0, 220.0),
            desktop: rect(45.0, 233.0, 795.0, 271.0),
            desktop_label: rect(96.0, 244.0, 263.84375, 260.0),
            menu: rect(45.0, 271.0, 795.0, 309.0),
            menu_label: rect(96.0, 282.0, 241.875, 298.0),
            accept: rect(45.0, 387.0, 305.0, 436.0),
        }
    }
}
