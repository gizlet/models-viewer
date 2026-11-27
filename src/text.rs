//! Interface strings in Simplified Chinese and English, and font atlases
//! holding exactly the characters the app draws.
//!
//! Latin text uses Inter SemiBold and titles Space Grotesk Bold; Chinese
//! comes from Noto Sans SC Medium, subset to the characters used here and in
//! the catalog (`tools/fonts/subset.py`). All three are under the SIL Open
//! Font License. raylib reads one font file per atlas, so each atlas is
//! assembled here, as in The Little Prince: glyphs below U+2E80 come from the
//! Latin face, the rest from Noto, shifted onto the Latin face's baseline.

use raylib_sys::{self as ffi, Color, Font, GlyphInfo, Rectangle, Vector2};
use std::collections::BTreeSet;
use std::ffi::CString;

pub static INTER: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
pub static GROTESK: &[u8] = include_bytes!("../assets/fonts/SpaceGrotesk-Bold.ttf");
pub static NOTO: &[u8] = include_bytes!("../assets/fonts/NotoSansSC-Medium.ttf");

const CJK_START: i32 = 0x2E80;
const PADDING: i32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_env() -> Lang {
        match std::env::var("GIZLET_LANG") {
            Ok(v) if v.starts_with("en") => Lang::En,
            _ => Lang::Zh,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum T {
    AppName,
    All,
    View,
    Category,
    Quit,
    Back,
    About,
    Rotate,
    Zoom,
    Switch,
    AutoRotate,
    AutoOn,
    AutoOff,
    Info,
    Style,
    Animation,
    Reset,
    Studio,
    Clay,
    Wireframe,
    Normals,
    Authors,
    License,
    Source,
    Collection,
    Triangles,
    Vertices,
    Materials,
    Textures,
    Joints,
    Animations,
    Size,
    File,
    Changes,
    Loading,
    LoadFailed,
    NoAnimation,
    RestPose,
    PressAgain,
    HoldToQuit,
    AboutTitle,
    AboutBody,
    AboutCredits,
    AboutCode,
    OpenLicenses,
    Scroll,
    Sources,
    Choose,
}

pub const ALL: [T; 48] = [
    T::AppName,
    T::All,
    T::View,
    T::Category,
    T::Quit,
    T::Back,
    T::About,
    T::Rotate,
    T::Zoom,
    T::Switch,
    T::AutoRotate,
    T::AutoOn,
    T::AutoOff,
    T::Info,
    T::Style,
    T::Animation,
    T::Reset,
    T::Studio,
    T::Clay,
    T::Wireframe,
    T::Normals,
    T::Authors,
    T::License,
    T::Source,
    T::Collection,
    T::Triangles,
    T::Vertices,
    T::Materials,
    T::Textures,
    T::Joints,
    T::Animations,
    T::Size,
    T::File,
    T::Changes,
    T::Loading,
    T::LoadFailed,
    T::NoAnimation,
    T::RestPose,
    T::PressAgain,
    T::HoldToQuit,
    T::AboutTitle,
    T::AboutBody,
    T::AboutCredits,
    T::AboutCode,
    T::OpenLicenses,
    T::Scroll,
    T::Sources,
    T::Choose,
];

pub fn s(t: T, l: Lang) -> &'static str {
    let (zh, en) = match t {
        T::AppName => ("模型展馆", "Models Viewer"),
        T::All => ("全部", "All"),
        T::View => ("查看", "View"),
        T::Category => ("分类", "Category"),
        T::Quit => ("退出", "Quit"),
        T::Back => ("返回", "Back"),
        T::About => ("关于", "About"),
        T::Rotate => ("旋转", "Turn"),
        T::Zoom => ("缩放", "Zoom"),
        T::Switch => ("切换", "Next"),
        T::AutoRotate => ("自转", "Spin"),
        T::AutoOn => ("自动旋转：开", "Auto-rotate on"),
        T::AutoOff => ("自动旋转：关", "Auto-rotate off"),
        T::Info => ("信息", "Info"),
        T::Style => ("风格", "Style"),
        T::Animation => ("动画", "Animation"),
        T::Reset => ("复位", "Reset"),
        T::Studio => ("摄影棚", "Studio"),
        T::Clay => ("白模", "Clay"),
        T::Wireframe => ("线框", "Wireframe"),
        T::Normals => ("法线", "Normals"),
        T::Authors => ("作者", "Authors"),
        T::License => ("授权", "License"),
        T::Source => ("来源", "Source"),
        T::Collection => ("合集", "Collection"),
        T::Triangles => ("三角面", "Triangles"),
        T::Vertices => ("顶点", "Vertices"),
        T::Materials => ("材质", "Materials"),
        T::Textures => ("贴图", "Textures"),
        T::Joints => ("骨骼", "Joints"),
        T::Animations => ("动画", "Animations"),
        T::Size => ("尺寸", "Size"),
        T::File => ("文件", "File"),
        T::Changes => ("改动", "Changes"),
        T::Loading => ("加载中…", "Loading…"),
        T::LoadFailed => ("这个模型无法加载", "This model could not be loaded"),
        T::NoAnimation => ("这个模型没有动画", "This model has no animations"),
        T::RestPose => ("静止姿态", "Rest pose"),
        T::PressAgain => ("再按一次 B 退出", "Press B again to quit"),
        T::HoldToQuit => ("继续按住以退出", "Keep holding to quit"),
        T::AboutTitle => ("关于模型展馆", "About Models Viewer"),
        T::AboutBody => (
            "在掌机上把玩开源社区的 3D 模型。每个模型都可以免费使用、修改和再分发。",
            "Turn over 3D models from the open community on your handheld. Every one is free to use, change and share.",
        ),
        T::AboutCredits => (
            "按 X 查看每个模型的作者、来源、授权和我们做过的改动。",
            "Press X on any model for its authors, source, license and what was changed for the handheld.",
        ),
        T::AboutCode => ("代码以 MIT 许可开源", "The code is open source under the MIT license"),
        T::OpenLicenses => ("全部采用 CC0 或 CC BY 4.0 授权", "All CC0 or CC BY 4.0"),
        T::Scroll => ("滚动", "Scroll"),
        T::Sources => ("模型来源", "Where the models come from"),
        T::Choose => ("选择", "Choose"),
    };
    match l {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

pub fn count(n: usize, l: Lang) -> String {
    match l {
        Lang::Zh => format!("{n} 个模型"),
        Lang::En if n == 1 => "1 model".to_string(),
        Lang::En => format!("{n} models"),
    }
}

/// Thousands separated with commas, in both languages.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn bytes(n: u64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1048576.0)
    } else {
        format!("{} KB", n.div_ceil(1024))
    }
}

/// Metres as centimetres, or metres from two metres up.
pub fn dimensions(size: [f32; 3]) -> String {
    let largest = size.iter().copied().fold(0.0, f32::max);
    if largest >= 2.0 {
        format!("{:.1} × {:.1} × {:.1} m", size[0], size[1], size[2])
    } else {
        let cm = size.map(|v| v * 100.0);
        let fmt = |v: f32| if v < 10.0 { format!("{v:.1}") } else { format!("{v:.0}") };
        format!("{} × {} × {} cm", fmt(cm[0]), fmt(cm[1]), fmt(cm[2]))
    }
}

/// Characters drawn outside the tables above and the catalog.
pub const EXTRA: &str = "·×…→←↑↓▶◀●○—–/:.,()%°'+# ";

pub fn codepoints(catalog_text: &str) -> BTreeSet<i32> {
    // '|' is left out: Inter's is taller than the atlas line.
    let mut set: BTreeSet<i32> = (32..127).filter(|&c| c != '|' as i32).collect();
    for t in ALL {
        for l in [Lang::Zh, Lang::En] {
            set.extend(s(t, l).chars().map(|c| c as i32));
        }
    }
    set.extend("个模型".chars().map(|c| c as i32));
    set.extend(EXTRA.chars().map(|c| c as i32));
    set.extend(catalog_text.chars().filter(|c| !c.is_control()).map(|c| c as i32));
    set
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Text,
    Display,
}

/// The baseline as a fraction of the pixel height raylib scales a face to,
/// from the `hhea` table: ascender / (ascender - descender).
fn baseline(font: &[u8]) -> f32 {
    let u16_at = |o: usize| font.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]));
    let u32_at = |o: usize| font.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let tables = u16_at(4).unwrap_or(0) as usize;
    for i in 0..tables {
        let entry = 12 + i * 16;
        if font.get(entry..entry + 4) == Some(b"hhea") {
            let Some(offset) = u32_at(entry + 8) else { break };
            let ascender = u16_at(offset as usize + 4).unwrap_or(0) as i16 as f32;
            let descender = u16_at(offset as usize + 6).unwrap_or(0) as i16 as f32;
            if ascender - descender > 0.0 {
                return ascender / (ascender - descender);
            }
        }
    }
    0.8
}

fn glyphs(font: &[u8], px: i32, codepoints: &[i32]) -> Vec<GlyphInfo> {
    if codepoints.is_empty() {
        return Vec::new();
    }
    let mut count = 0;
    unsafe {
        let ptr = ffi::LoadFontData(
            font.as_ptr(),
            font.len() as i32,
            px,
            codepoints.as_ptr().cast_mut(),
            codepoints.len() as i32,
            ffi::FontType::FONT_DEFAULT as i32,
            &mut count,
        );
        if ptr.is_null() {
            return Vec::new();
        }
        let out = std::slice::from_raw_parts(ptr, count.max(0) as usize).to_vec();
        // The glyph images now belong to `out`; only the array is freed.
        ffi::MemFree(ptr.cast());
        out
    }
}

fn build(latin: &[u8], px: i32, codepoints: &BTreeSet<i32>) -> Font {
    let (lat, cjk): (Vec<i32>, Vec<i32>) = codepoints.iter().partition(|&&c| c < CJK_START);
    let mut all = glyphs(latin, px, &lat);
    // Symbols Inter lacks come from Noto, which draws most of them.
    let missing: Vec<i32> = lat.iter().copied().filter(|c| all.iter().all(|g| g.value != *c)).collect();
    let shift = ((baseline(latin) - baseline(NOTO)) * px as f32).round() as i32;
    for mut g in glyphs(NOTO, px, &cjk).into_iter().chain(glyphs(NOTO, px, &missing)) {
        g.offsetY += shift;
        all.push(g);
    }
    unsafe {
        let size = std::mem::size_of::<GlyphInfo>() * all.len().max(1);
        let array = ffi::MemAlloc(size as u32) as *mut GlyphInfo;
        std::ptr::copy_nonoverlapping(all.as_ptr(), array, all.len());
        let mut recs: *mut Rectangle = std::ptr::null_mut();
        let atlas = ffi::GenImageFontAtlas(array, &mut recs, all.len() as i32, px, PADDING, 0);
        let texture = ffi::LoadTextureFromImage(atlas);
        ffi::UnloadImage(atlas);
        ffi::SetTextureFilter(texture, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
        Font { baseSize: px, glyphCount: all.len() as i32, glyphPadding: PADDING, texture, recs, glyphs: array }
    }
}

/// Atlases at a few sizes, rasterized at the UI zoom so text stays sharp on
/// a large desktop window. Sizes are in 640×480 canvas pixels.
pub struct Fonts {
    faces: Vec<(Weight, f32, Font)>,
    zoom: f32,
}

impl Fonts {
    pub fn new(zoom: f32, catalog_text: &str) -> Fonts {
        let set = codepoints(catalog_text);
        let mut faces = Vec::new();
        // raylib warns about each symbol a Latin face lacks; Noto fills those in.
        unsafe { ffi::SetTraceLogLevel(ffi::TraceLogLevel::LOG_ERROR as i32) };
        for (w, sizes, face) in
            [(Weight::Text, &[11.0, 13.0, 15.0, 18.0][..], INTER), (Weight::Display, &[22.0, 28.0, 40.0][..], GROTESK)]
        {
            for &px in sizes {
                faces.push((w, px, build(face, (px * zoom).round() as i32, &set)));
            }
        }
        let level =
            if cfg!(feature = "gizlet-os") { ffi::TraceLogLevel::LOG_INFO } else { ffi::TraceLogLevel::LOG_WARNING };
        unsafe { ffi::SetTraceLogLevel(level as i32) };
        Fonts { faces, zoom }
    }

    fn pick(&self, w: Weight, size: f32) -> Font {
        let mut best = None;
        for (fw, px, f) in &self.faces {
            if *fw == w {
                best = Some(*f);
                if *px >= size - 0.5 {
                    break;
                }
            }
        }
        best.expect("fonts")
    }

    pub fn measure(&self, s: &str, size: f32, w: Weight) -> f32 {
        let text = CString::new(s).unwrap_or_default();
        unsafe { ffi::MeasureTextEx(self.pick(w, size), text.as_ptr(), size, 0.0).x }
    }

    pub fn draw(&self, s: &str, x: f32, y: f32, size: f32, color: Color, align: Align, w: Weight) -> f32 {
        let text = CString::new(s).unwrap_or_default();
        let font = self.pick(w, size);
        let width = unsafe { ffi::MeasureTextEx(font, text.as_ptr(), size, 0.0).x };
        let x = match align {
            Align::Left => x,
            Align::Center => x - width / 2.0,
            Align::Right => x - width,
        };
        // Snap to device pixels so small text does not blur.
        let snap = |v: f32| (v * self.zoom).round() / self.zoom;
        unsafe { ffi::DrawTextEx(font, text.as_ptr(), Vector2 { x: snap(x), y: snap(y) }, size, 0.0, color) };
        width
    }

    pub fn text(&self, s: &str, x: f32, y: f32, size: f32, color: Color, align: Align) -> f32 {
        self.draw(s, x, y, size, color, align, Weight::Text)
    }

    /// `s`, shortened with an ellipsis to fit `width`.
    pub fn fit(&self, s: &str, size: f32, width: f32, w: Weight) -> String {
        if self.measure(s, size, w) <= width {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        while chars.len() > 1 {
            chars.pop();
            let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
            if self.measure(&candidate, size, w) <= width {
                return candidate;
            }
        }
        "…".to_string()
    }

    /// Breaks `s` into lines no wider than `width`, at spaces for Latin text
    /// and between any two characters for Chinese.
    pub fn wrap(&self, s: &str, size: f32, width: f32, w: Weight) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        let mut word = String::new();
        let flush_word = |line: &mut String, word: &mut String, lines: &mut Vec<String>| {
            if word.is_empty() {
                return;
            }
            let candidate = format!("{line}{word}");
            if !line.is_empty() && self.measure(candidate.trim_end(), size, w) > width {
                lines.push(line.trim_end().to_string());
                *line = word.trim_start().to_string();
            } else {
                *line = candidate;
            }
            word.clear();
        };
        for c in s.chars() {
            let cjk = (c as i32) >= CJK_START;
            if cjk {
                flush_word(&mut line, &mut word, &mut lines);
                word.push(c);
                flush_word(&mut line, &mut word, &mut lines);
            } else {
                word.push(c);
                if c == ' ' || c == '/' || c == '-' {
                    flush_word(&mut line, &mut word, &mut lines);
                }
            }
        }
        flush_word(&mut line, &mut word, &mut lines);
        if !line.trim().is_empty() {
            lines.push(line.trim_end().to_string());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_string_has_both_languages() {
        for t in ALL {
            assert!(!s(t, Lang::Zh).is_empty() && !s(t, Lang::En).is_empty(), "{t:?}");
        }
    }

    #[test]
    fn numbers_read_well() {
        assert_eq!(grouped(1234567), "1,234,567");
        assert_eq!(grouped(999), "999");
        assert_eq!(bytes(2048), "2 KB");
        assert_eq!(bytes(3 * 1048576 / 2), "1.5 MB");
        assert_eq!(dimensions([0.081, 0.3, 1.2]), "8.1 × 30 × 120 cm");
        assert_eq!(dimensions([4.0, 2.0, 1.0]), "4.0 × 2.0 × 1.0 m");
        assert_eq!(count(1, Lang::En), "1 model");
        assert_eq!(count(61, Lang::Zh), "61 个模型");
    }

    /// Every character the app can draw is in one of its fonts.
    #[test]
    fn fonts_cover_every_character() {
        let catalog =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/models/catalog.json")).unwrap();
        let faces = [INTER, NOTO].map(|f| ttf_parser::Face::parse(f, 0).expect("font"));
        let grotesk = ttf_parser::Face::parse(GROTESK, 0).expect("font");
        for cp in codepoints(&catalog) {
            let c = char::from_u32(cp as u32).unwrap();
            assert!(faces.iter().any(|f| f.glyph_index(c).is_some()), "no glyph for {c:?} (U+{cp:04X})");
            if cp < CJK_START && cp != ' ' as i32 {
                let latin = faces[0].glyph_index(c).is_some() || grotesk.glyph_index(c).is_some();
                let noto = faces[1].glyph_index(c).is_some();
                assert!(latin || noto, "no glyph for {c:?}");
            }
        }
    }
}
