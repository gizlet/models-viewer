//! Drawing helpers for the 640×480 interface: colours, panels, button
//! badges, the hint bar, toasts and the loading ring. One corner radius for
//! everything, after the Gizlet site.

use crate::text::{Align, Fonts, Weight};
use raylib_sys::{self as ffi, Color, Rectangle, Vector2};

pub const fn rgb(hex: u32) -> Color {
    Color { r: (hex >> 16) as u8, g: (hex >> 8) as u8, b: hex as u8, a: 255 }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a: (c.a as f32 * a.clamp(0.0, 1.0)) as u8, ..c }
}

pub fn mix(a: Color, b: Color, k: f32) -> Color {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k.clamp(0.0, 1.0)).round() as u8;
    Color { r: l(a.r, b.r), g: l(a.g, b.g), b: l(a.b, b.b), a: l(a.a, b.a) }
}

pub const INK: Color = rgb(0x0d0e12);
pub const BACK_TOP: Color = rgb(0x181b22);
pub const BACK_BOTTOM: Color = rgb(0x0d0e12);
pub const CARD: Color = rgb(0x1d2029);
pub const CARD_FOCUS: Color = rgb(0x272b36);
pub const LINE: Color = rgb(0x2c303b);
pub const TEXT: Color = rgb(0xf2f3f5);
pub const MUTED: Color = rgb(0xa4a9b5);
pub const FAINT: Color = rgb(0x6b7180);
pub const SUN: Color = rgb(0xffd23a);
pub const STAGE_TOP: Color = rgb(0x30343f);
pub const STAGE_BOTTOM: Color = rgb(0x111216);

pub const RADIUS: f32 = 8.0;

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
    Rectangle { x, y, width: w, height: h }
}

/// A rectangle with corners of `RADIUS` canvas pixels.
pub fn panel(r: Rectangle, color: Color) {
    let short = r.width.min(r.height).max(1.0);
    let roundness = (2.0 * RADIUS / short).min(1.0);
    unsafe { ffi::DrawRectangleRounded(r, roundness, 6, color) };
}

pub fn outline(r: Rectangle, thick: f32, color: Color) {
    let short = r.width.min(r.height).max(1.0);
    let roundness = (2.0 * RADIUS / short).min(1.0);
    unsafe { ffi::DrawRectangleRoundedLinesEx(r, roundness, 6, thick, color) };
}

pub fn gradient(r: Rectangle, top: Color, bottom: Color) {
    unsafe {
        ffi::DrawRectangleGradientV(r.x as i32, r.y as i32, r.width.ceil() as i32, r.height.ceil() as i32, top, bottom)
    };
}

pub fn fill(r: Rectangle, color: Color) {
    unsafe { ffi::DrawRectangleRec(r, color) };
}

/// A button as printed on the handheld, `label` from `Controls::label`.
/// Returns the badge's width.
pub fn badge(fonts: &Fonts, label: &str, x: f32, cy: f32, a: f32) -> f32 {
    let face = Color { a: (235.0 * a) as u8, ..rgb(0xe8e9ee) };
    let ink = alpha(INK, a);
    match label {
        "A" | "B" | "X" | "Y" => {
            unsafe { ffi::DrawCircleV(Vector2 { x: x + 8.0, y: cy }, 8.0, face) };
            fonts.text(label, x + 8.0, cy - 7.0, 11.0, ink, Align::Center);
            16.0
        }
        "dpad" => {
            fill(rect(x + 5.0, cy - 8.0, 6.0, 16.0), face);
            fill(rect(x, cy - 3.0, 16.0, 6.0), face);
            16.0
        }
        _ => {
            let w = fonts.measure(label, 11.0, Weight::Text) + 10.0;
            panel(rect(x, cy - 7.5, w, 15.0), face);
            fonts.text(label, x + w / 2.0, cy - 7.0, 11.0, ink, Align::Center);
            w
        }
    }
}

/// A row of `(buttons, action)` hints, centred at `cy`, with the gaps
/// narrowed when the row would not fit the canvas.
pub fn hints(fonts: &Fonts, items: &[(&[&str], &str)], center_x: f32, cy: f32, a: f32) -> f32 {
    let measure = |item: &(&[&str], &str)| {
        let badges: f32 = item.0.iter().map(|b| badge_width(fonts, b) + 3.0).sum();
        badges + 3.0 + fonts.measure(item.1, 13.0, Weight::Text)
    };
    let content: f32 = items.iter().map(measure).sum();
    let gaps = items.len().saturating_sub(1).max(1) as f32;
    let gap = ((620.0 - content) / gaps).clamp(7.0, 16.0);
    let total = content + gap * items.len().saturating_sub(1) as f32;
    let mut x = center_x - total / 2.0;
    for item in items {
        for b in item.0 {
            x += badge(fonts, b, x, cy, a) + 3.0;
        }
        x += 3.0;
        x += fonts.text(item.1, x, cy - 8.0, 13.0, alpha(MUTED, a), Align::Left);
        x += gap;
    }
    total
}

fn badge_width(fonts: &Fonts, label: &str) -> f32 {
    match label {
        "A" | "B" | "X" | "Y" | "dpad" => 16.0,
        _ => fonts.measure(label, 11.0, Weight::Text) + 10.0,
    }
}

/// A short message in a dark panel, centred at `cy`.
pub fn toast(fonts: &Fonts, msg: &str, cx: f32, cy: f32, a: f32) {
    let w = fonts.measure(msg, 15.0, Weight::Text) + 32.0;
    panel(rect(cx - w / 2.0, cy - 18.0, w, 36.0), alpha(rgb(0x0b0c10), 0.9 * a));
    fonts.text(msg, cx, cy - 9.0, 15.0, alpha(TEXT, a), Align::Center);
}

/// A turning arc for work in progress.
pub fn spinner(cx: f32, cy: f32, r: f32, t: f32, a: f32) {
    let start = (t * 300.0) % 360.0;
    let sweep = 90.0 + 70.0 * (t * 2.1).sin();
    unsafe {
        ffi::DrawRing(Vector2 { x: cx, y: cy }, r - 3.0, r, 0.0, 360.0, 48, alpha(LINE, a));
        ffi::DrawRing(Vector2 { x: cx, y: cy }, r - 3.0, r, start, start + sweep, 32, alpha(SUN, a));
    }
}

/// Draws `tex` fitted into `r`, keeping its aspect.
pub fn picture(tex: ffi::Texture2D, r: Rectangle, tint: Color) {
    let k = (r.width / tex.width as f32).min(r.height / tex.height as f32);
    let (w, h) = (tex.width as f32 * k, tex.height as f32 * k);
    let dest = rect(r.x + (r.width - w) / 2.0, r.y + (r.height - h) / 2.0, w, h);
    let src = rect(0.0, 0.0, tex.width as f32, tex.height as f32);
    unsafe { ffi::DrawTexturePro(tex, src, dest, Vector2 { x: 0.0, y: 0.0 }, 0.0, tint) };
}

/// Clips drawing to `r` (canvas units) while `f` runs.
pub fn clipped(r: Rectangle, zoom: f32, origin: (f32, f32), f: impl FnOnce()) {
    unsafe {
        ffi::BeginScissorMode(
            (origin.0 + r.x * zoom).round() as i32,
            (origin.1 + r.y * zoom).round() as i32,
            (r.width * zoom).round() as i32,
            (r.height * zoom).round() as i32,
        );
    }
    f();
    unsafe { ffi::EndScissorMode() };
}
