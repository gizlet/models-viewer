//! Models Viewer · 模型展馆: a 3D gallery of open models for Gizlet OS
//! handhelds, in Rust + raylib. Browse 3D models from Khronos, Poly Haven,
//! KayKit, Google Research and NVIDIA by category, then turn, zoom and
//! animate each on a lit stage. Every model is CC0 or CC BY 4.0 and credited
//! in the app.
//!
//! Environment:
//! - `GIZLET_LANG=en`: English (Simplified Chinese otherwise).
//! - `GIZLET_SWAP_AB=1`: swap the A and B buttons.
//! - `GIZLET_SAVE_DIR`: where settings are kept (`./save` otherwise).
//! - `MODELSVIEWER_WINDOW=WxH`: the desktop window size (960x720 by default).
//! - `MODELSVIEWER_MSAA=0`: no multisampling.
//! - `MODELSVIEWER_FPS=N`: frame-rate cap while something moves (60).
//! - `MODELSVIEWER_SECONDS=N`: quit after N seconds.
//! - `MODELSVIEWER_PRESS=A@1,R1@2.5,...`: press buttons at those seconds, for
//!   scripted runs; `MODELSVIEWER_HIDDEN=1` keeps the window out of sight.
//! - `MODELSVIEWER_TIMING=1`: print how long each model takes to read.
//! - `MODELSVIEWER_THUMBS=DIR`: render every model's card picture into DIR and exit.
//! - `MODELSVIEWER_CAPTURE=FILE.png`: render one frame to a PNG and exit, with
//!   `MODELSVIEWER_SCENE` (`gallery`, `viewer`, `info`, `about`, `cover`,
//!   `icon`), `MODELSVIEWER_MODEL` (a catalog id), `MODELSVIEWER_TAB` (a
//!   category id), `MODELSVIEWER_STYLE` and `MODELSVIEWER_AT` (seconds to
//!   play first), at `MODELSVIEWER_SIZE=WxH` (1280x960 by default).

mod anim;
mod app;
mod catalog;
#[cfg(all(target_os = "linux", feature = "gizlet-os"))]
mod evdev;
mod gpu;
mod input;
mod loader;
mod math;
mod scene;
mod settings;
mod text;
mod ui;

use app::{App, Screen, H, W};
use raylib_sys::{self as ffi, ConfigFlags as F, Rectangle};
use std::ffi::CString;

/// Without a window system: raylib's DRM platform on the whole panel.
const KMS: bool = cfg!(feature = "gizlet-os");

fn env_size(name: &str) -> Option<(i32, i32)> {
    let v = std::env::var(name).ok()?;
    let (w, h) = v.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

fn env_f32(name: &str) -> Option<f32> {
    std::env::var(name).ok()?.parse().ok()
}

/// Where the 640×480 canvas goes on the screen, letterboxed, and its scale.
fn canvas() -> (Rectangle, f32) {
    let (sw, sh) = unsafe { (ffi::GetRenderWidth() as f32, ffi::GetRenderHeight() as f32) };
    let k = (sw / W).min(sh / H);
    let (w, h) = (W * k, H * k);
    (Rectangle { x: ((sw - w) / 2.0).floor(), y: ((sh - h) / 2.0).floor(), width: w, height: h }, k)
}

fn open_window(w: i32, h: i32, flags: u32) -> bool {
    unsafe {
        ffi::SetConfigFlags(flags);
        ffi::InitWindow(w, h, c"Models Viewer · 模型展馆".as_ptr());
        // raylib reports the window ready even when the DRM platform failed
        // (another program holding the display); then no GL context exists.
        ffi::IsWindowReady() && ffi::rlGetShaderIdDefault() != 0
    }
}

/// `A@1,R1@2.5`: buttons and the second each is pressed, held for 0.1 s.
fn parse_presses(spec: &str) -> Vec<(f64, input::Button)> {
    use input::Button as B;
    spec.split(',')
        .filter_map(|item| {
            let (name, at) = item.trim().split_once('@')?;
            let b = match name {
                "Up" => B::Up,
                "Down" => B::Down,
                "Left" => B::Left,
                "Right" => B::Right,
                "A" => B::A,
                "B" => B::B,
                "X" => B::X,
                "Y" => B::Y,
                "L1" => B::L1,
                "R1" => B::R1,
                "L2" => B::L2,
                "R2" => B::R2,
                "Select" => B::Select,
                "Start" => B::Start,
                _ => {
                    eprintln!("modelsviewer: unknown button {name}");
                    return None;
                }
            };
            Some((at.parse().ok()?, b))
        })
        .collect()
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        let version = option_env!("GIZLET_APP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
        println!("modelsviewer {version}");
        return;
    }
    let capture = std::env::var("MODELSVIEWER_CAPTURE").ok();
    let thumbs = std::env::var("MODELSVIEWER_THUMBS").ok();
    let offline = capture.is_some() || thumbs.is_some();
    // Captures draw off screen at their own size; their window only holds the GL context.
    let capture_size = env_size("MODELSVIEWER_SIZE").unwrap_or((1280, 960));
    let (w, h) = match (KMS, offline) {
        (true, _) => (0, 0),
        (false, true) => (640, 480),
        (false, false) => env_size("MODELSVIEWER_WINDOW").unwrap_or((960, 720)),
    };
    let mut flags = F::FLAG_VSYNC_HINT as u32;
    if !KMS {
        flags |= F::FLAG_WINDOW_RESIZABLE as u32;
    }
    if offline || std::env::var("MODELSVIEWER_HIDDEN").is_ok_and(|v| v == "1") {
        flags |= F::FLAG_WINDOW_HIDDEN as u32;
    }
    let msaa = std::env::var("MODELSVIEWER_MSAA").map_or(true, |v| v != "0");
    unsafe {
        ffi::SetTraceLogLevel(if KMS { ffi::TraceLogLevel::LOG_INFO } else { ffi::TraceLogLevel::LOG_WARNING } as i32);
    }
    let mut ready = open_window(w, h, flags | if msaa { F::FLAG_MSAA_4X_HINT as u32 } else { 0 });
    if !ready && msaa {
        eprintln!("modelsviewer: no display with 4x multisampling, trying without");
        unsafe { ffi::CloseWindow() };
        ready = open_window(w, h, flags);
    }
    if !ready {
        eprintln!("modelsviewer: the display could not be opened (is another program using it?)");
        std::process::exit(1);
    }
    unsafe {
        // raylib's one batch buffer is rewritten at every flush; on Panfrost
        // rewriting a buffer the frame still uses stalls the CPU. Several in
        // rotation keep each flush on a fresh one (see The Little Prince).
        let batch = Box::leak(Box::new(ffi::rlLoadRenderBatch(16, 4096)));
        ffi::rlSetRenderBatchActive(batch);
        ffi::SetExitKey(0);
        if !KMS {
            ffi::SetWindowMinSize(320, 240);
        }
    }
    let lang = text::Lang::from_env();
    let zoom = if capture.is_some() { (capture_size.0 as f32 / W).max(1.0) } else { canvas().1.max(1.0) };
    let store = settings::Store::from_env(!offline);
    let mut app = App::new(lang, zoom, store);

    if let Some(dir) = thumbs {
        render_thumbnails(&mut app, &dir);
        unsafe { ffi::CloseWindow() };
        return;
    }
    if let Some(path) = capture {
        render_capture(&mut app, &path, capture_size);
        unsafe { ffi::CloseWindow() };
        return;
    }

    let fps = std::env::var("MODELSVIEWER_FPS").ok().and_then(|v| v.parse().ok()).unwrap_or(60);
    let quit_after = env_f32("MODELSVIEWER_SECONDS");
    let presses = std::env::var("MODELSVIEWER_PRESS").map(|s| parse_presses(&s)).unwrap_or_default();
    let mut controls = input::Controls::new();
    let start = unsafe { ffi::GetTime() };
    let mut busy = true;
    while !unsafe { ffi::WindowShouldClose() } && !app.quit {
        // Drawing slows down when nothing moves, to save the battery.
        unsafe { ffi::SetTargetFPS(if busy { fps } else { 20 }) };
        let dt = unsafe { ffi::GetFrameTime() }.min(0.1);
        let now = unsafe { ffi::GetTime() } - start;
        if quit_after.is_some_and(|s| now >= s as f64) {
            break;
        }
        let (dest, k) = canvas();
        app.target = unsafe { (ffi::GetRenderWidth(), ffi::GetRenderHeight()) };
        controls.scripted = presses.iter().filter(|(at, _)| now >= *at && now < at + 0.1).map(|(_, b)| *b).collect();
        controls.poll(dt, now, k);
        app.update(dt, &controls);
        busy = controls.active() || app.moving();
        app.prepare(app.stage_viewport(dest, k));
        unsafe {
            ffi::BeginDrawing();
            ffi::ClearBackground(ui::INK);
        }
        app.draw(dest, k);
        unsafe { ffi::EndDrawing() };
    }
    app.save();
    unsafe { ffi::CloseWindow() };
}

/// Steps the app with no buttons held, at 60 steps a second.
fn settle(app: &mut App, seconds: f32) {
    let idle = input::Controls::new();
    for _ in 0..(seconds * 60.0) as usize {
        app.update(1.0 / 60.0, &idle);
    }
}

/// Writes `image` as an opaque PNG: blending leaves partial alpha in an
/// off-screen target that the screen would never show.
fn export(mut image: ffi::Image, path: &str) {
    unsafe { ffi::ImageFormat(&mut image, ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8 as i32) };
    let c = CString::new(path).unwrap_or_default();
    if !unsafe { ffi::ExportImage(image, c.as_ptr()) } {
        eprintln!("modelsviewer: could not write {path}");
        std::process::exit(1);
    }
    unsafe { ffi::UnloadImage(image) };
}

/// Draws the model on show into an off-screen target of `w`×`h` device
/// pixels over `background`, and returns the picture right way up.
fn still(app: &mut App, w: i32, h: i32, background: impl Fn()) -> ffi::Image {
    let viewport = (0, 0, w, h);
    app.target = (w, h);
    app.force_shadow();
    app.prepare(viewport);
    unsafe {
        let target = ffi::LoadRenderTexture(w, h);
        ffi::BeginTextureMode(target);
        ffi::ClearBackground(ui::INK);
        background();
        if let (Some(view), Some(sh)) = (app.view(viewport), app.viewer.shown.as_ref()) {
            app.renderer.draw_floor(&sh.gpu, &view, &app.stage);
            app.renderer.draw_model(&sh.gpu, &view, &sh.player.world, &app.stage);
            gpu::restore_viewport(w, h);
        }
        ffi::EndTextureMode();
        let mut image = ffi::LoadImageFromTexture(target.texture);
        ffi::ImageFlipVertical(&mut image);
        ffi::UnloadRenderTexture(target);
        image
    }
}

/// The card pictures: every model at three-quarters, twice the size and scaled down.
fn render_thumbnails(app: &mut App, dir: &str) {
    let _ = std::fs::create_dir_all(dir);
    app.set_tab(0);
    let (w, h) = (512, 384);
    for pos in 0..app.list.len() {
        app.open_now(pos);
        settle(app, 1.2);
        app.stage.floor = [0.16, 0.17, 0.21, 1.0];
        let image = still(app, w, h, || unsafe {
            ffi::DrawRectangleGradientV(0, 0, w, h, ui::rgb(0x2a2e39), ui::rgb(0x1d2029));
        });
        let mut image = image;
        unsafe { ffi::ImageResize(&mut image, w / 2, h / 2) };
        let id = &app.model(app.viewer.index).id;
        export(image, &format!("{dir}/{id}.png"));
        println!("{id}");
    }
}

fn render_capture(app: &mut App, path: &str, (sw, sh): (i32, i32)) {
    let scene = std::env::var("MODELSVIEWER_SCENE").unwrap_or_else(|_| "gallery".into());
    let at = env_f32("MODELSVIEWER_AT").unwrap_or(1.5);
    if let Ok(id) = std::env::var("MODELSVIEWER_TAB") {
        let tab = app.catalog.categories.iter().position(|c| c.id == id).map_or(0, |k| k + 1);
        app.set_tab(tab);
    }
    if let Ok(style) = std::env::var("MODELSVIEWER_STYLE") {
        app.renderer.style = app::style_from(&style);
    }
    let model = std::env::var("MODELSVIEWER_MODEL").ok();
    if let Some(id) = &model {
        match app.list.iter().position(|&i| &app.catalog.models[i].id == id) {
            Some(pos) => app.focus = pos,
            None => {
                eprintln!("modelsviewer: no model {id} in this tab");
                std::process::exit(1);
            }
        }
    }
    match scene.as_str() {
        "gallery" | "about" => {
            settle(app, at);
            if scene == "about" {
                app.screen = Screen::About;
                settle(app, 0.2);
            }
        }
        "viewer" | "info" | "cover" | "icon" => {
            app.open_now(app.focus);
            app.viewer.info = scene == "info";
            settle(app, at);
            if scene == "cover" {
                // A little room around the model in the wide frame.
                app.viewer.zoom = 1.22;
                app.viewer.zoom_goal = 1.22;
            }
        }
        other => {
            eprintln!("modelsviewer: unknown scene {other}");
            std::process::exit(1);
        }
    }
    let image = match scene.as_str() {
        "icon" => {
            app.stage.floor = [0.10, 0.17, 0.62, 1.0];
            still(app, sw, sh, || unsafe {
                ffi::DrawRectangleGradientV(0, 0, sw, sh, ui::rgb(0x3d55ff), ui::rgb(0x1b2bb8));
            })
        }
        "cover" => cover(app, sw, sh),
        _ => {
            let k = (sw as f32 / W).min(sh as f32 / H);
            let dest = Rectangle { x: 0.0, y: 0.0, width: W * k, height: H * k };
            app.target = (sw, sh);
            app.prepare(app.stage_viewport(dest, k));
            unsafe {
                let target = ffi::LoadRenderTexture(sw, sh);
                ffi::BeginTextureMode(target);
                ffi::ClearBackground(ui::INK);
                app.draw(dest, k);
                ffi::EndTextureMode();
                let mut image = ffi::LoadImageFromTexture(target.texture);
                ffi::ImageFlipVertical(&mut image);
                ffi::UnloadRenderTexture(target);
                image
            }
        }
    };
    export(image, path);
}

/// The Store cover: the model on show to the right, the app's name to the left.
fn cover(app: &mut App, w: i32, h: i32) -> ffi::Image {
    let lang = app.lang;
    let split = (w as f32 * 0.42) as i32;
    let viewport = (split, 0, w - split, h);
    app.target = (w, h);
    app.force_shadow();
    app.prepare(viewport);
    unsafe {
        let target = ffi::LoadRenderTexture(w, h);
        ffi::BeginTextureMode(target);
        ffi::ClearBackground(ui::INK);
        ffi::DrawRectangleGradientV(0, 0, w, h, ui::STAGE_TOP, ui::STAGE_BOTTOM);
        ffi::DrawCircleGradient(
            ffi::Vector2 { x: (split + (w - split) / 2) as f32, y: h as f32 * 0.46 },
            h as f32 * 0.6,
            ui::alpha(ui::rgb(0x4a5060), 0.6),
            ui::alpha(ui::rgb(0x4a5060), 0.0),
        );
        if let (Some(view), Some(sh)) = (app.view(viewport), app.viewer.shown.as_ref()) {
            app.renderer.draw_floor(&sh.gpu, &view, &app.stage);
            app.renderer.draw_model(&sh.gpu, &view, &sh.player.world, &app.stage);
            gpu::restore_viewport(w, h);
        }
        let k = h as f32 / 360.0;
        ffi::BeginMode2D(ffi::Camera2D {
            offset: ffi::Vector2 { x: 0.0, y: 0.0 },
            target: ffi::Vector2 { x: 0.0, y: 0.0 },
            rotation: 0.0,
            zoom: k,
        });
        let f = &app.fonts;
        f.draw(text::s(text::T::AppName, lang), 48.0, 118.0, 40.0, ui::TEXT, text::Align::Left, text::Weight::Display);
        let other = if lang == text::Lang::Zh { "Models Viewer" } else { "模型展馆" };
        f.draw(other, 50.0, 166.0, 22.0, ui::MUTED, text::Align::Left, text::Weight::Display);
        ui::fill(ui::rect(50.0, 202.0, 36.0, 3.0), ui::SUN);
        f.text(&text::count(app.catalog.models.len(), lang), 50.0, 216.0, 15.0, ui::TEXT, text::Align::Left);
        f.text(text::s(text::T::OpenLicenses, lang), 50.0, 238.0, 13.0, ui::MUTED, text::Align::Left);
        ffi::EndMode2D();
        ffi::EndTextureMode();
        let mut image = ffi::LoadImageFromTexture(target.texture);
        ffi::ImageFlipVertical(&mut image);
        ffi::UnloadRenderTexture(target);
        image
    }
}
