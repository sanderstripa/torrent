use crate::windows_canvas::{
    dpi_scale, open_target_at_dpi, rect, rgb, use_fonts, window_dpi, Canvas,
};
use torrent_re_sort::engine::TorrentFile;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::D2D1_ANTIALIAS_MODE_PER_PRIMITIVE;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{BeginPaint, EndPaint, InvalidateRect, PAINTSTRUCT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_DOWN, VK_RETURN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const PROPERTY: windows_sys::core::PCWSTR = windows_sys::w!("TorrentFiles144");
pub const APPLY: usize = 111;
pub const CANCEL: usize = 112;
pub const PRIORITY: usize = 0x1000;
const HEADER: f32 = 143.0;
const ROW: f32 = 42.0;
const WIDTH: f32 = 580.0;

pub struct FileView {
    files: Vec<TorrentFile>,
    title: String,
    english: bool,
    dark: bool,
    order: Vec<usize>,
    name_width: f32,
    height: f32,
    pub priorities: Vec<u8>,
    remembered: Vec<u8>,
    sort: u8,
    ascending: bool,
    scroll: f32,
    horizontal: f32,
    drag: Option<(f32, f32)>,
    row: Option<usize>,
    focus: usize,
    keyboard: bool,
    hover: Option<usize>,
}

impl FileView {
    pub fn new(
        files: Vec<TorrentFile>,
        priorities: Vec<u8>,
        title: String,
        english: bool,
        dark: bool,
    ) -> Self {
        let height = (213.0 + files.len() as f32 * ROW).clamp(300.0, 590.0);
        let order = (0..files.len()).collect();
        let remembered = priorities
            .iter()
            .map(|p| if *p == 0 { 4 } else { *p })
            .collect();
        let name_width = use_fonts(|fonts| {
            let format = fonts.format(13, false)?;
            let mut maximum = 0.0f32;
            for file in &files {
                let encoded = file_name(file).encode_utf16().collect::<Vec<_>>();
                unsafe {
                    let layout =
                        fonts
                            .factory
                            .CreateTextLayout(&encoded, &format, 1_000_000.0, ROW)?;
                    let mut metrics =
                        windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS::default();
                    layout.GetMetrics(&mut metrics)?;
                    maximum = maximum.max(metrics.widthIncludingTrailingWhitespace);
                }
            }
            Ok(maximum)
        })
        .unwrap_or(0.0);
        Self {
            files,
            title,
            english,
            dark,
            order,
            name_width,
            height,
            priorities,
            remembered,
            sort: 0,
            ascending: true,
            scroll: 0.0,
            horizontal: 0.0,
            drag: None,
            row: None,
            focus: APPLY,
            keyboard: false,
            hover: None,
        }
    }

    pub fn size(&self) -> (i32, i32) {
        (WIDTH as i32, self.height as i32)
    }
    pub fn menu_point(&self, index: usize) -> (i32, i32) {
        let row = self
            .order
            .iter()
            .position(|&value| value == index)
            .unwrap_or(0);
        (
            (WIDTH - 150.0) as i32,
            (HEADER + (row as f32 + 1.0) * ROW - self.scroll) as i32,
        )
    }
    pub fn set_priority(&mut self, index: usize, priority: u8) {
        let Some(value) = self.priorities.get_mut(index) else {
            return;
        };
        if priority == 0 {
            if *value != 0 {
                self.remembered[index] = *value;
            }
        } else {
            self.remembered[index] = priority;
        }
        *value = priority;
    }
    fn toggle(&mut self, index: usize) {
        let priority = if self.priorities[index] == 0 {
            self.remembered[index]
        } else {
            0
        };
        self.set_priority(index, priority);
    }
    fn toggle_all(&mut self) {
        let all = self.priorities.iter().all(|&p| p != 0);
        for index in 0..self.priorities.len() {
            if all || self.priorities[index] == 0 {
                self.toggle(index);
            }
        }
    }
    fn sort(&mut self, order: u8) {
        let current = self.row.and_then(|row| self.order.get(row)).copied();
        self.ascending = self.sort != order || !self.ascending;
        self.sort = order;
        self.order.sort_by(|&left, &right| {
            let a = &self.files[left];
            let b = &self.files[right];
            let comparison = if order == 1 {
                crate::windows_app::compare_names(file_name(a), file_name(b))
            } else {
                a.size.cmp(&b.size)
            };
            if self.ascending {
                comparison
            } else {
                comparison.reverse()
            }
        });
        self.row = current.and_then(|index| self.order.iter().position(|&value| value == index));
    }
    fn hmax(&self) -> f32 {
        (self.name_width + 16.0 - (WIDTH - 352.0)).max(0.0)
    }
    fn list_bottom(&self) -> f32 {
        self.height - if self.hmax() > 0.0 { 96.0 } else { 71.0 }
    }
    fn max_scroll(&self) -> f32 {
        (self.order.len() as f32 * ROW - (self.list_bottom() - HEADER)).max(0.0)
    }
    fn track(&self) -> D2D_RECT_F {
        rect(98.0, self.height - 88.0, WIDTH - 254.0, self.height - 78.0)
    }
    fn thumb(&self) -> D2D_RECT_F {
        let track = self.track();
        let len = track.right - track.left;
        let thumb = (len * len / (len + self.hmax())).clamp(28.0, len);
        let ratio = if self.hmax() > 0.0 {
            self.horizontal / self.hmax()
        } else {
            0.0
        };
        let left = track.left + (len - thumb) * ratio;
        rect(left, track.top, left + thumb, track.bottom)
    }
    fn button(&self, id: usize) -> D2D_RECT_F {
        let x = if id == APPLY {
            WIDTH - 128.0
        } else {
            WIDTH - 240.0
        };
        rect(x, self.height - 54.0, x + 104.0, self.height - 18.0)
    }
    fn seek_row(&mut self, row: usize) {
        let row = row.min(self.order.len().saturating_sub(1));
        self.row = Some(row);
        self.focus = PRIORITY;
        let position = row as f32 * ROW;
        if position < self.scroll {
            self.scroll = position;
        }
        let viewport = self.list_bottom() - HEADER;
        if position + ROW > self.scroll + viewport {
            self.scroll = (position + ROW - viewport).clamp(0.0, self.max_scroll());
        }
    }
}

fn file_name(file: &TorrentFile) -> &str {
    std::path::Path::new(&file.path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&file.path)
}

pub fn priority_label(priority: u8, en: bool) -> &'static str {
    let (ru, en_text) = match priority {
        0 => ("Не скачивать", "Skip"),
        1 => ("Низкий", "Low"),
        7.. => ("Высокий", "High"),
        _ => ("Обычный", "Normal"),
    };
    if en {
        en_text
    } else {
        ru
    }
}

fn contains(r: D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

pub unsafe fn dispatch(window: HWND, message: u32, key: usize, extra: isize) -> Option<isize> {
    let cell = unsafe {
        GetPropW(window, PROPERTY)
            .cast::<std::cell::RefCell<FileView>>()
            .as_ref()
    }?;
    let mut state = cell.try_borrow_mut().ok()?;
    let scale = dpi_scale(window);
    let raw_x = extra as u16 as i16 as i32;
    let raw_y = (extra >> 16) as u16 as i16 as i32;
    let x = raw_x as f32 / scale;
    let y = raw_y as f32 / scale;
    let command = |id| unsafe { PostMessageW(window, crate::windows_app::ACTION, id, 0) };
    match message {
        WM_PAINT => {
            unsafe {
                paint(window, &state);
            }
            return Some(0);
        }
        WM_ERASEBKGND | WM_NCACTIVATE => return Some(1),
        WM_NCPAINT => return Some(0),
        WM_NCCALCSIZE if key != 0 => return Some(0),
        WM_NCHITTEST => {
            let mut point = windows_sys::Win32::Foundation::POINT { x: raw_x, y: raw_y };
            unsafe {
                windows_sys::Win32::Graphics::Gdi::ScreenToClient(window, &mut point);
            }
            return Some(if (point.y as f32) < 52.0 * scale
                && (point.x as f32) < (WIDTH - 50.0) * scale
            {
                HTCAPTION
            } else {
                HTCLIENT
            } as isize);
        }
        WM_COMMAND if key & 0xffff == APPLY || key & 0xffff == CANCEL => {
            command(key & 0xffff);
            return Some(0);
        }
        WM_MOUSEMOVE => {
            state.hover = [APPLY, CANCEL]
                .into_iter()
                .find(|&id| contains(state.button(id), x, y));
            if let Some((begin, offset)) = state.drag {
                let track = state.track();
                let thumb = state.thumb();
                let range = (track.right - track.left - thumb.right + thumb.left).max(1.0);
                state.horizontal =
                    (offset + (x - begin) * state.hmax() / range).clamp(0.0, state.hmax());
            }
        }
        WM_LBUTTONDOWN if state.hmax() > 0.0 && contains(state.track(), x, y) => {
            let thumb = state.thumb();
            if !contains(thumb, x, y) {
                let track = state.track();
                state.horizontal = ((x - track.left) / (track.right - track.left) * state.hmax())
                    .clamp(0.0, state.hmax());
            }
            state.drag = Some((x, state.horizontal));
            unsafe {
                SetCapture(window);
            }
        }
        WM_CAPTURECHANGED => state.drag = None,
        WM_LBUTTONUP => {
            if state.drag.take().is_some() {
                drop(state);
                unsafe {
                    ReleaseCapture();
                    InvalidateRect(window, std::ptr::null(), 0);
                }
                return Some(0);
            }
            state.keyboard = false;
            if contains(state.button(APPLY), x, y) {
                command(APPLY);
            } else if contains(state.button(CANCEL), x, y) || x > WIDTH - 46.0 && y < 50.0 {
                command(CANCEL);
            } else if (112.0..HEADER).contains(&y) {
                if x < 60.0 {
                    state.toggle_all();
                } else if x < WIDTH - 254.0 {
                    state.sort(1);
                } else if x < WIDTH - 151.0 {
                    state.sort(2);
                }
            } else if (HEADER..state.list_bottom()).contains(&y) {
                let row = ((y - HEADER + state.scroll) / ROW) as usize;
                if let Some(&index) = state.order.get(row) {
                    state.row = Some(row);
                    state.focus = PRIORITY;
                    if x < 60.0 {
                        state.toggle(index);
                    } else if x >= WIDTH - 150.0 {
                        command(PRIORITY + index);
                    }
                }
            }
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let delta = ((key >> 16) as u16 as i16) as f32 / 3.0;
            if message == WM_MOUSEHWHEEL {
                state.horizontal = (state.horizontal + delta).clamp(0.0, state.hmax());
            } else if unsafe { GetKeyState(VK_SHIFT as i32) } < 0 {
                state.horizontal = (state.horizontal - delta).clamp(0.0, state.hmax());
            } else {
                state.scroll = (state.scroll - delta).clamp(0.0, state.max_scroll());
            }
        }
        WM_KEYDOWN => {
            state.keyboard = true;
            match key as u16 {
                VK_TAB => {
                    let order = [APPLY, CANCEL, PRIORITY];
                    let current = order.iter().position(|&id| id == state.focus).unwrap_or(0);
                    let backward = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
                    state.focus = order[(current + if backward { 2 } else { 1 }) % 3];
                    if state.focus == PRIORITY && state.row.is_none() {
                        state.seek_row(0);
                    }
                }
                VK_UP | VK_DOWN => {
                    let row = state.row.unwrap_or(0);
                    let row = if key as u16 == VK_UP {
                        row.saturating_sub(1)
                    } else {
                        row + 1
                    };
                    state.seek_row(row);
                }
                VK_SPACE | VK_RETURN => {
                    if state.focus == PRIORITY {
                        if let Some(index) = state.row.and_then(|row| state.order.get(row)).copied()
                        {
                            if key as u16 == VK_SPACE {
                                state.toggle(index);
                            } else {
                                command(PRIORITY + index);
                            }
                        }
                    } else {
                        command(state.focus);
                    }
                }
                _ => return None,
            }
        }
        _ => return None,
    }
    unsafe {
        InvalidateRect(window, std::ptr::null(), 0);
    }
    Some(0)
}

unsafe fn paint(window: HWND, view: &FileView) {
    let mut data = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(window, &mut data) };
    let mut bounds = RECT::default();
    unsafe {
        GetClientRect(window, &mut bounds);
    }
    let _ = draw(dc, bounds, window_dpi(window), view);
    unsafe {
        EndPaint(window, &data);
    }
}

fn draw(
    dc: windows_sys::Win32::Graphics::Gdi::HDC,
    bounds: RECT,
    dpi: f32,
    view: &FileView,
) -> windows::core::Result<()> {
    let (_, target) = open_target_at_dpi(dc, bounds, dpi)?;
    let (bg, fg, muted, card, line, selected, track, heading) = if view.dark {
        (
            0x08090b, 0xeeeeef, 0x96979d, 0x111215, 0x292a2e, 0x202125, 0x303136, 0x191919,
        )
    } else {
        (
            0xffffff, 0x242628, 0x73767b, 0xfcfcfd, 0xe6e7e9, 0xededef, 0xe6e7e9, 0xf0f0ee,
        )
    };
    let brush = unsafe { target.CreateSolidColorBrush(&rgb(fg), None)? };
    let canvas = Canvas {
        target: &target,
        brush,
    };
    let en = view.english;
    let tr = |ru, en_text| if en { en_text } else { ru };
    let width = WIDTH;
    unsafe {
        target.BeginDraw();
        target.Clear(Some(&rgb(bg)));
    }
    let result = use_fonts(|fonts| {
        let label = |text, r, color| canvas.label(fonts, text, r, 13, false, color, false);
        let check = |x, y, checked| {
            canvas.rounded(
                rect(x, y, x + 16.0, y + 16.0),
                4.0,
                if checked { fg } else { card },
                (!checked).then_some(line),
            );
            if checked {
                canvas.line(x + 4.0, y + 8.0, x + 7.0, y + 11.0, bg, 1.7);
                canvas.line(x + 7.0, y + 11.0, x + 12.0, y + 5.0, bg, 1.7);
            }
        };
        canvas.label(
            fonts,
            tr("ФАЙЛЫ ТОРРЕНТА", "TORRENT FILES"),
            rect(24.0, 14.0, width - 65.0, 50.0),
            18,
            false,
            fg,
            false,
        )?;
        canvas.line(width - 31.0, 24.0, width - 21.0, 34.0, muted, 1.5);
        canvas.line(width - 21.0, 24.0, width - 31.0, 34.0, muted, 1.5);
        canvas.label(
            fonts,
            &view.title,
            rect(24.0, 51.0, width - 24.0, 79.0),
            19,
            true,
            fg,
            false,
        )?;
        let bytes = view
            .files
            .iter()
            .fold(0i64, |sum, file| sum.saturating_add(file.size));
        let details = format!(
            "{} · {} {}",
            crate::windows_app::formatted_size(bytes, en),
            view.files.len(),
            tr("файлов", "files")
        );
        canvas.label(
            fonts,
            &details,
            rect(24.0, 76.0, width - 24.0, 98.0),
            13,
            false,
            muted,
            false,
        )?;
        canvas.rounded(
            rect(18.0, 112.0, width - 18.0, view.height - 70.0),
            9.0,
            card,
            Some(line),
        );
        canvas.rounded(rect(18.0, 112.0, width - 18.0, HEADER), 7.0, heading, None);
        check(32.0, 120.0, view.priorities.iter().all(|&p| p != 0));
        label(
            tr("Имя файла", "File name"),
            rect(68.0, 112.0, width - 281.0, HEADER),
            if view.sort == 1 { fg } else { muted },
        )?;
        label(
            tr("Размер", "Size"),
            rect(width - 242.0, 112.0, width - 178.0, HEADER),
            if view.sort == 2 { fg } else { muted },
        )?;
        label(
            tr("Приоритет", "Priority"),
            rect(width - 148.0, 112.0, width - 30.0, HEADER),
            muted,
        )?;
        if view.sort != 0 {
            let x = width - if view.sort == 1 { 281.0 } else { 178.0 };
            label(
                if view.ascending { "↑" } else { "↓" },
                rect(x, 112.0, x + 23.0, HEADER),
                fg,
            )?;
        }
        unsafe {
            target.PushAxisAlignedClip(
                &rect(19.0, HEADER, width - 19.0, view.list_bottom()),
                D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            );
        }
        let rows: windows::core::Result<()> = (|| {
            for (row, &index) in view.order.iter().enumerate() {
                let y = HEADER + row as f32 * ROW - view.scroll;
                if y + ROW < HEADER || y > view.list_bottom() {
                    continue;
                }
                if view.row == Some(row) {
                    canvas.rounded(
                        rect(20.0, y + 1.0, width - 20.0, y + 41.0),
                        0.0,
                        selected,
                        None,
                    );
                }
                let priority = view.priorities[index];
                check(32.0, y + 13.0, priority != 0);
                let file = &view.files[index];
                let name = file_name(file);
                file_icon(&canvas, name, y, card, muted);
                unsafe {
                    target.PushAxisAlignedClip(
                        &rect(98.0, y, width - 254.0, y + ROW),
                        D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                    );
                }
                let name_result = label(
                    name,
                    rect(
                        98.0 - view.horizontal,
                        y,
                        98.0 - view.horizontal + (view.name_width + 12.0).max(width - 340.0),
                        y + ROW,
                    ),
                    fg,
                );
                unsafe {
                    target.PopAxisAlignedClip();
                }
                name_result?;
                let size = crate::windows_app::formatted_size(file.size, en);
                canvas.label(
                    fonts,
                    &size,
                    rect(width - 242.0, y, width - 158.0, y + ROW),
                    13,
                    false,
                    muted,
                    false,
                )?;
                canvas.rounded(
                    rect(width - 150.0, y + 3.0, width - 30.0, y + 39.0),
                    8.0,
                    card,
                    Some(line),
                );
                label(
                    priority_label(priority, en),
                    rect(width - 140.0, y + 6.0, width - 48.0, y + 36.0),
                    if priority >= 7 { fg } else { muted },
                )?;
                canvas.line(width - 47.0, y + 18.0, width - 43.0, y + 22.0, muted, 1.5);
                canvas.line(width - 43.0, y + 22.0, width - 39.0, y + 18.0, muted, 1.5);
                canvas.line(20.0, y + ROW, width - 20.0, y + ROW, line, 1.0);
            }
            Ok(())
        })();
        unsafe {
            target.PopAxisAlignedClip();
        }
        rows?;
        if view.hmax() > 0.0 {
            canvas.rounded(view.track(), 5.0, track, None);
            canvas.rounded(
                view.thumb(),
                5.0,
                if view.dark { 0xa7aaad } else { 0x797d82 },
                None,
            );
        }
        for (id, text, primary) in [
            (APPLY, tr("Применить", "Apply"), true),
            (CANCEL, tr("Отмена", "Cancel"), false),
        ] {
            let r = view.button(id);
            if view.keyboard && view.focus == id {
                canvas.rounded(
                    rect(r.left - 3.0, r.top - 3.0, r.right + 3.0, r.bottom + 3.0),
                    8.0,
                    bg,
                    Some(muted),
                );
            }
            let fill = if view.hover == Some(id) && !primary {
                selected
            } else if primary {
                fg
            } else {
                card
            };
            canvas.rounded(r, 8.0, fill, (!primary).then_some(line));
            canvas.centered(fonts, text, r, 13, if primary { bg } else { fg })?;
        }
        Ok(())
    });
    let end = unsafe { target.EndDraw(None, None) };
    result.and(end)
}

fn file_icon(canvas: &Canvas<'_>, name: &str, y: f32, body: u32, muted: u32) {
    canvas.rounded(rect(70.0, y + 12.0, 84.0, y + 30.0), 2.0, muted, None);
    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let line = |x, a, b, c, stroke| canvas.line(x, y + a, b, y + c, body, stroke);
    match ext.as_str() {
        "mkv" | "mp4" | "avi" => {
            line(75.0, 17.0, 80.0, 21.0, 1.7);
            line(80.0, 21.0, 75.0, 25.0, 1.7);
            line(75.0, 25.0, 75.0, 17.0, 1.7);
        }
        "jpg" | "png" | "jpeg" => {
            line(72.0, 27.0, 76.0, 21.0, 1.5);
            line(76.0, 21.0, 79.0, 25.0, 1.5);
            line(79.0, 25.0, 81.0, 22.0, 1.5);
        }
        _ => {
            line(73.0, 18.0, 81.0, 18.0, 1.0);
            line(73.0, 22.0, 81.0, 22.0, 1.0);
            line(73.0, 26.0, 78.0, 26.0, 1.0);
        }
    }
}
