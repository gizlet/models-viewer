//! The app's screens and what the buttons do on each: the gallery of
//! thumbnails by category, the viewer that turns one model on the stage, and
//! the page about the app and where the models come from.

use crate::anim::Player;
use crate::catalog::{Catalog, Model};
use crate::gpu::{self, Renderer, Stage, Style, View};
use crate::input::{Button, Controls};
use crate::loader::{self, Loader};
use crate::math::{approach, v3};
use crate::scene::Scene;
use crate::settings::{Settings, Store};
use crate::text::{self, s, Align, Fonts, Lang, Weight, T};
use crate::ui::{self, rect};
use raylib_sys::{self as ffi, Rectangle, Texture2D, Vector2};
use std::ffi::CString;
use std::path::PathBuf;

pub const W: f32 = 640.0;
pub const H: f32 = 480.0;

pub const COLS: usize = 4;
const GRID_X: f32 = 24.0;
const GRID_Y: f32 = 104.0;
const GRID_H: f32 = 338.0;
const GAP: f32 = 12.0;
const CARD_W: f32 = (W - 2.0 * GRID_X - GAP * (COLS as f32 - 1.0)) / COLS as f32;
const THUMB_H: f32 = CARD_W * 0.75;
const CARD_H: f32 = THUMB_H + 40.0;
const ROW: f32 = CARD_H + GAP;
const PANEL_W: f32 = 256.0;
const TAB_LEFT: f32 = GRID_X + 24.0;
const TAB_SPAN: f32 = W - 2.0 * TAB_LEFT;
const TAB_GAP: f32 = 18.0;
const FOVY: f32 = 0.62;
const DEFAULT_SPIN: f32 = -0.55;
const DEFAULT_PITCH: f32 = 0.28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Gallery,
    Viewer,
    About,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
}

/// Moves the focus through a grid of `len` items, `cols` wide. Left and
/// right run on through the rows; up and down stop at the edges.
pub fn grid_move(focus: usize, len: usize, cols: usize, dir: Dir) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    match dir {
        Dir::Left => focus.saturating_sub(1),
        Dir::Right => (focus + 1).min(last),
        Dir::Up if focus >= cols => focus - cols,
        Dir::Up => focus,
        // From a short last row's neighbourhood, down lands on the last item.
        Dir::Down if focus / cols < last / cols => (focus + cols).min(last),
        Dir::Down => focus,
    }
}

/// The scroll that keeps row `row` in view, eased from `current`.
pub fn scroll_for(row: usize, current: f32, view: f32) -> f32 {
    let top = row as f32 * ROW;
    let bottom = top + CARD_H;
    if top - current < 0.0 {
        top - 6.0
    } else if bottom - current > view {
        bottom - view + 6.0
    } else {
        current
    }
    .max(0.0)
}

/// The clip to start with: an idle loop if there is one.
pub fn default_clip(names: &[&str]) -> Option<usize> {
    if names.is_empty() {
        return None;
    }
    for want in ["Idle", "Survey", "Unarmed_Idle"] {
        if let Some(i) = names.iter().position(|n| *n == want) {
            return Some(i);
        }
    }
    names.iter().position(|n| n.to_lowercase().contains("idle")).or(Some(0))
}

pub fn style_name(style: Style) -> T {
    match style {
        Style::Studio => T::Studio,
        Style::Clay => T::Clay,
        Style::Wireframe => T::Wireframe,
        Style::Normals => T::Normals,
    }
}

pub fn style_key(style: Style) -> &'static str {
    match style {
        Style::Studio => "studio",
        Style::Clay => "clay",
        Style::Wireframe => "wireframe",
        Style::Normals => "normals",
    }
}

pub fn style_from(key: &str) -> Style {
    Style::ALL.into_iter().find(|s| style_key(*s) == key).unwrap_or(Style::Studio)
}

/// A model on the stage.
pub struct Shown {
    pub index: usize,
    pub scene: Scene,
    pub player: Player,
    pub gpu: gpu::Model,
    pub clip: Option<usize>,
    pub appear: f32,
    shadow_key: Option<(f32, f32, u64)>,
}

impl Shown {
    pub fn new(index: usize, scene: Scene, wire: bool) -> Shown {
        let mut player = Player::new(&scene);
        let names: Vec<&str> = scene.animations.iter().map(|a| a.name.as_str()).collect();
        let clip = default_clip(&names);
        // Bounds and placement come from the rest pose.
        let mut gpu = gpu::Model::new(&scene, &player);
        player.play(clip);
        player.advance(&scene, 0.0);
        gpu.update_pose(&scene, &player);
        if wire {
            gpu.ensure_wire(&scene);
        }
        Shown { index, scene, player, gpu, clip, appear: 0.0, shadow_key: None }
    }

    /// A clip plays, or the last one is still fading out.
    pub fn animated(&self) -> bool {
        self.clip.is_some() || self.player.fading()
    }
}

pub struct Viewer {
    /// The catalog index on show or being loaded.
    pub index: usize,
    pub shown: Option<Shown>,
    pub error: Option<String>,
    pub spin: f32,
    pub spin_goal: f32,
    pub pitch: f32,
    pub pitch_goal: f32,
    pub zoom: f32,
    pub zoom_goal: f32,
    /// Seconds since the model was last turned by hand.
    idle: f32,
    pub info: bool,
    info_k: f32,
    info_scroll: f32,
    info_scroll_goal: f32,
    info_height: f32,
    /// Seconds since the last input, for fading the hints.
    quiet: f32,
    load_started: f32,
}

impl Viewer {
    fn new() -> Viewer {
        Viewer {
            index: 0,
            shown: None,
            error: None,
            spin: DEFAULT_SPIN,
            spin_goal: DEFAULT_SPIN,
            pitch: DEFAULT_PITCH,
            pitch_goal: DEFAULT_PITCH,
            zoom: 1.0,
            zoom_goal: 1.0,
            idle: 0.0,
            info: false,
            info_k: 0.0,
            info_scroll: 0.0,
            info_scroll_goal: 0.0,
            info_height: 0.0,
            quiet: 0.0,
            load_started: 0.0,
        }
    }

    /// Back to the starting view, turning the short way round.
    pub fn reset_view(&mut self) {
        let turns = ((self.spin - DEFAULT_SPIN) / std::f32::consts::TAU).round();
        self.spin_goal = DEFAULT_SPIN + turns * std::f32::consts::TAU;
        self.pitch_goal = DEFAULT_PITCH;
        self.zoom_goal = 1.0;
        self.idle = 0.0;
    }

    pub fn loading(&self) -> bool {
        self.error.is_none() && self.shown.as_ref().is_none_or(|s| s.index != self.index)
    }
}

pub struct App {
    pub lang: Lang,
    pub catalog: Catalog,
    pub assets: PathBuf,
    pub fonts: Fonts,
    thumbs: Vec<Option<Texture2D>>,
    thumbs_tried: Vec<bool>,
    pub renderer: Renderer,
    loader: Loader,
    store: Store,
    pub screen: Screen,
    /// 0 is every model; k + 1 is category k.
    pub tab: usize,
    pub list: Vec<usize>,
    pub focus: usize,
    scroll: f32,
    tab_scroll: f32,
    pub viewer: Viewer,
    pub auto_rotate: bool,
    toast: Option<(String, f32)>,
    quit_armed: f32,
    quit_hold: f32,
    pub quit: bool,
    about_scroll: f32,
    about_height: f32,
    pub time: f32,
    /// Seconds since the last button in the gallery, which stops the focus pulse.
    gallery_quiet: f32,
    pub stage: Stage,
    /// The size of what is drawn into, in device pixels: the screen or a capture.
    pub target: (i32, i32),
    /// Where the canvas is drawn this frame and its scale, for clipping.
    canvas: (Rectangle, f32),
}

impl App {
    pub fn new(lang: Lang, zoom: f32, store: Store) -> App {
        let assets = crate::catalog::assets_dir();
        let catalog = Catalog::load(&assets).unwrap_or_else(|e| {
            eprintln!("modelsviewer: {e}");
            std::process::exit(1)
        });
        let catalog_text = std::fs::read_to_string(assets.join("models/catalog.json")).unwrap_or_default();
        let fonts = Fonts::new(zoom, &catalog_text);
        let settings = store.load();
        let n = catalog.models.len();
        let mut app = App {
            lang,
            fonts,
            thumbs: (0..n).map(|_| None).collect(),
            thumbs_tried: vec![false; n],
            renderer: Renderer::new(),
            loader: Loader::new(),
            store,
            screen: Screen::Gallery,
            tab: 0,
            list: Vec::new(),
            focus: 0,
            scroll: 0.0,
            tab_scroll: 0.0,
            viewer: Viewer::new(),
            auto_rotate: settings.auto_rotate,
            toast: None,
            quit_armed: 0.0,
            quit_hold: 0.0,
            quit: false,
            about_scroll: 0.0,
            about_height: 0.0,
            time: 0.0,
            gallery_quiet: 0.0,
            stage: gpu::STAGE,
            target: (W as i32, H as i32),
            canvas: (rect(0.0, 0.0, W, H), 1.0),
            assets,
            catalog,
        };
        app.renderer.style = style_from(&settings.style);
        let tab = settings
            .category
            .as_deref()
            .and_then(|id| app.catalog.categories.iter().position(|c| c.id == id))
            .map_or(0, |k| k + 1);
        app.set_tab(tab);
        if let Some(id) = settings.model {
            if let Some(pos) = app.list.iter().position(|&i| app.catalog.models[i].id == id) {
                app.focus = pos;
                app.scroll = scroll_for(pos / COLS, 0.0, GRID_H);
            }
        }
        app
    }

    pub fn model(&self, index: usize) -> &Model {
        &self.catalog.models[index]
    }

    fn path(&self, index: usize) -> PathBuf {
        self.assets.join(format!("models/{}.glb", self.catalog.models[index].id))
    }

    pub fn set_tab(&mut self, tab: usize) {
        self.tab = tab.min(self.catalog.categories.len());
        self.list = self.catalog.in_category(self.tab.checked_sub(1));
        self.focus = 0;
        self.scroll = 0.0;
    }

    pub fn settings(&self) -> Settings {
        Settings {
            style: style_key(self.renderer.style).into(),
            auto_rotate: self.auto_rotate,
            category: self.tab.checked_sub(1).map(|k| self.catalog.categories[k].id.clone()),
            model: self.list.get(self.focus).map(|&i| self.catalog.models[i].id.clone()),
        }
    }

    pub fn save(&self) {
        self.store.save(&self.settings());
    }

    fn say(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 1.6));
    }

    /// Shows the model at `focus` in the current list, loading it in the background.
    pub fn open(&mut self, focus: usize) {
        let Some(&index) = self.list.get(focus) else { return };
        self.focus = focus;
        self.screen = Screen::Viewer;
        let v = &mut self.viewer;
        v.index = index;
        v.error = None;
        v.quiet = 0.0;
        v.info_scroll_goal = 0.0;
        v.load_started = self.time;
        if v.shown.as_ref().is_some_and(|s| s.index == index) {
            return;
        }
        self.loader.request(index, self.path(index));
    }

    /// Loads the model at `focus` on this thread, for captures.
    pub fn open_now(&mut self, focus: usize) {
        self.open(focus);
        let index = self.viewer.index;
        match loader::load(&self.path(index)) {
            Ok(scene) => self.show(index, scene),
            Err(e) => self.viewer.error = Some(e),
        }
    }

    fn show(&mut self, index: usize, scene: Scene) {
        let wire = self.renderer.style == Style::Wireframe;
        let shown = Shown::new(index, scene, wire);
        let v = &mut self.viewer;
        v.shown = Some(shown);
        v.spin = DEFAULT_SPIN - 0.9;
        v.spin_goal = DEFAULT_SPIN;
        v.pitch_goal = DEFAULT_PITCH;
        v.zoom = 1.18;
        v.zoom_goal = 1.0;
        v.idle = 0.0;
    }

    fn poll_loader(&mut self) {
        let Some(loaded) = self.loader.poll() else { return };
        if loaded.index != self.viewer.index {
            return;
        }
        match loaded.scene {
            Ok(scene) => {
                if std::env::var_os("MODELSVIEWER_TIMING").is_some() {
                    eprintln!(
                        "modelsviewer: {} read in {:.0} ms",
                        self.model(loaded.index).id,
                        loaded.seconds * 1000.0
                    );
                }
                self.show(loaded.index, scene);
            }
            Err(e) => {
                eprintln!("modelsviewer: {e}");
                self.viewer.error = Some(e);
            }
        }
    }

    pub fn update(&mut self, dt: f32, c: &Controls) {
        self.time += dt;
        if c.any_pressed() || c.active() {
            self.gallery_quiet = 0.0;
        } else {
            self.gallery_quiet += dt;
        }
        if c.down(Button::Select) && c.down(Button::Start) {
            self.quit_hold += dt;
            self.quit |= self.quit_hold > 1.5;
        } else {
            self.quit_hold = 0.0;
        }
        if let Some((_, left)) = &mut self.toast {
            *left -= dt;
        }
        if self.toast.as_ref().is_some_and(|(_, left)| *left <= 0.0) {
            self.toast = None;
        }
        self.quit_armed = (self.quit_armed - dt).max(0.0);
        self.poll_loader();
        if self.quit_hold > 0.0 {
            return;
        }
        match self.screen {
            Screen::Gallery => self.update_gallery(dt, c),
            Screen::Viewer => self.update_viewer(dt, c),
            Screen::About => {
                let (_, y) = c.turn();
                let max = (self.about_height - 300.0).max(0.0);
                self.about_scroll = (self.about_scroll + y * 260.0 * dt).clamp(0.0, max);
                if c.pressed(Button::B) || c.pressed(Button::Start) || c.pressed(Button::A) {
                    self.screen = Screen::Gallery;
                }
            }
        }
    }

    fn update_gallery(&mut self, dt: f32, c: &Controls) {
        let tabs = self.catalog.categories.len() + 1;
        if c.nav(Button::R1) {
            self.set_tab((self.tab + 1) % tabs);
        } else if c.nav(Button::L1) {
            self.set_tab((self.tab + tabs - 1) % tabs);
        }
        for (b, d) in
            [(Button::Left, Dir::Left), (Button::Right, Dir::Right), (Button::Up, Dir::Up), (Button::Down, Dir::Down)]
        {
            if c.nav(b) {
                self.focus = grid_move(self.focus, self.list.len(), COLS, d);
            }
        }
        if c.pressed(Button::A) {
            self.open(self.focus);
        } else if c.pressed(Button::Start) {
            self.about_scroll = 0.0;
            self.screen = Screen::About;
        } else if c.pressed(Button::B) {
            if self.quit_armed > 0.0 {
                self.quit = true;
            } else {
                self.quit_armed = 2.0;
                self.say(s(T::PressAgain, self.lang));
            }
        }
        let goal = scroll_for(self.focus / COLS, self.scroll, GRID_H);
        self.scroll = approach(self.scroll, goal, 14.0, dt);
        let goal = self.tab_scroll_goal();
        self.tab_scroll = approach(self.tab_scroll, goal, 14.0, dt);
    }

    /// The tabs' x positions from the left of the strip, and their widths.
    fn tab_layout(&self) -> (Vec<f32>, Vec<f32>) {
        let tabs = self.catalog.categories.len() + 1;
        let widths: Vec<f32> = (0..tabs).map(|t| self.fonts.measure(&self.tab_title(t), 15.0, Weight::Text)).collect();
        let mut xs = Vec::with_capacity(tabs);
        let mut x = 0.0;
        for w in &widths {
            xs.push(x);
            x += w + TAB_GAP;
        }
        (xs, widths)
    }

    fn tab_scroll_goal(&self) -> f32 {
        let (xs, widths) = self.tab_layout();
        let last = xs.len() - 1;
        let content = xs[last] + widths[last];
        let middle = xs[self.tab] + widths[self.tab] / 2.0;
        (middle - TAB_SPAN / 2.0).clamp(0.0, (content - TAB_SPAN).max(0.0))
    }

    fn step_model(&mut self, delta: isize) {
        let n = self.list.len() as isize;
        if n == 0 {
            return;
        }
        let next = (self.focus as isize + delta).rem_euclid(n) as usize;
        self.open(next);
    }

    /// Whether the next frame will differ from this one.
    pub fn moving(&self) -> bool {
        if self.toast.is_some() || self.quit_hold > 0.0 {
            return true;
        }
        match self.screen {
            Screen::Gallery => self.gallery_quiet < 4.0,
            Screen::About => self.gallery_quiet < 1.0,
            Screen::Viewer => {
                let v = &self.viewer;
                let settling = (v.spin - v.spin_goal).abs() > 1e-3
                    || (v.pitch - v.pitch_goal).abs() > 1e-3
                    || (v.zoom - v.zoom_goal).abs() > 1e-3
                    || (v.info_k - if v.info { 1.0 } else { 0.0 }).abs() > 1e-3
                    || (v.info_scroll - v.info_scroll_goal).abs() > 0.5;
                let shown = v.shown.as_ref();
                settling
                    || v.loading()
                    || self.auto_rotate
                    || v.quiet < 4.2
                    || shown.is_some_and(|s| s.animated() || s.appear < 1.0)
            }
        }
    }

    fn update_viewer(&mut self, dt: f32, c: &Controls) {
        if c.any_pressed() || c.active() {
            self.viewer.quiet = 0.0;
        } else {
            self.viewer.quiet += dt;
        }
        if c.pressed(Button::B) {
            if self.viewer.info {
                self.viewer.info = false;
            } else {
                self.screen = Screen::Gallery;
                self.scroll = scroll_for(self.focus / COLS, self.scroll, GRID_H);
                self.save();
                return;
            }
        }
        if c.nav(Button::R1) {
            self.step_model(1);
        } else if c.nav(Button::L1) {
            self.step_model(-1);
        }
        if c.pressed(Button::A) {
            self.auto_rotate = !self.auto_rotate;
            self.viewer.idle = 2.0;
            self.say(s(if self.auto_rotate { T::AutoOn } else { T::AutoOff }, self.lang));
            self.save();
        }
        if c.pressed(Button::X) {
            self.viewer.info = !self.viewer.info;
            self.viewer.info_scroll_goal = 0.0;
        }
        if c.pressed(Button::Y) {
            let i = Style::ALL.iter().position(|s| *s == self.renderer.style).unwrap_or(0);
            self.renderer.style = Style::ALL[(i + 1) % Style::ALL.len()];
            if self.renderer.style == Style::Wireframe {
                if let Some(sh) = &mut self.viewer.shown {
                    sh.gpu.ensure_wire(&sh.scene);
                }
            }
            self.say(s(style_name(self.renderer.style), self.lang));
            self.save();
        }
        if c.pressed(Button::Select) {
            self.next_clip();
        }
        if c.pressed(Button::Start) {
            self.viewer.reset_view();
        }
        let v = &mut self.viewer;
        let (tx, ty) = c.turn();
        let (lx, ly) = c.look();
        let mut manual = tx != 0.0 || lx != 0.0 || c.drag.0 != 0.0;
        v.spin_goal += (tx * 2.4 + lx * 2.0) * dt + c.drag.0 * 0.012;
        if v.info {
            let max = (v.info_height - (H - 64.0)).max(0.0);
            v.info_scroll_goal = (v.info_scroll_goal + ty * 300.0 * dt).clamp(0.0, max);
        } else if ty != 0.0 || c.drag.1 != 0.0 {
            v.pitch_goal = (v.pitch_goal - ty * 1.5 * dt + c.drag.1 * 0.01).clamp(-0.2, 1.25);
            manual = true;
        }
        let zoom = c.zoom() + -ly;
        v.zoom_goal = (v.zoom_goal * (-zoom * 1.3 * dt - c.wheel * 0.12).exp()).clamp(0.42, 2.6);
        if manual {
            v.idle = 0.0;
        } else {
            v.idle += dt;
        }
        if self.auto_rotate && v.idle > 1.5 {
            let ease = ((v.idle - 1.5) / 1.2).min(1.0);
            v.spin_goal += 0.32 * ease * dt;
        }
        v.spin = approach(v.spin, v.spin_goal, 9.0, dt);
        v.pitch = approach(v.pitch, v.pitch_goal, 9.0, dt);
        v.zoom = approach(v.zoom, v.zoom_goal, 8.0, dt);
        v.info_k = approach(v.info_k, if v.info { 1.0 } else { 0.0 }, 14.0, dt);
        v.info_scroll = approach(v.info_scroll, v.info_scroll_goal, 14.0, dt);
        if let Some(sh) = &mut v.shown {
            sh.appear = (sh.appear + dt * 2.5).min(1.0);
            if sh.animated() {
                sh.player.advance(&sh.scene, dt);
                sh.gpu.update_pose(&sh.scene, &sh.player);
            }
        }
    }

    fn next_clip(&mut self) {
        let lang = self.lang;
        let Some(sh) = &mut self.viewer.shown else { return };
        let n = sh.scene.animations.len();
        if n == 0 {
            self.say(s(T::NoAnimation, lang));
            return;
        }
        let next = match sh.clip {
            Some(i) if i + 1 < n => Some(i + 1),
            Some(_) => None,
            None => Some(0),
        };
        sh.clip = next;
        sh.player.play(next);
        let msg = match next {
            Some(i) => format!("{} · {} / {}", pretty_clip(&sh.scene.animations[i].name), i + 1, n),
            None => s(T::RestPose, lang).to_string(),
        };
        self.say(msg);
    }

    /// The current camera for the model on show.
    pub fn view(&self, viewport: (i32, i32, i32, i32)) -> Option<View> {
        let sh = self.viewer.shown.as_ref()?;
        let v = &self.viewer;
        let (_, _, w, h) = viewport;
        let aspect = w as f32 / h.max(1) as f32;
        let r = sh.gpu.radius;
        let half_h = FOVY * 0.5;
        let half_w = (half_h.tan() * aspect).atan();
        let dist = (r / half_h.sin()).max(r / half_w.sin()) * 1.02 * v.zoom;
        let target = v3(0.0, sh.gpu.height * 0.47, 0.0);
        let eye = target + v3(0.0, v.pitch.sin(), v.pitch.cos()) * dist;
        Some(View { eye, target, fovy: FOVY, spin: v.spin, viewport })
    }

    /// Renders the shadow texture if the pose or view changed. Call before drawing.
    pub fn prepare(&mut self, viewport: (i32, i32, i32, i32)) {
        if self.screen != Screen::Viewer {
            return;
        }
        let Some(view) = self.view(viewport) else { return };
        let Some(sh) = &mut self.viewer.shown else { return };
        let key = (view.spin, self.viewer.pitch, sh.player.generation);
        let moved =
            sh.shadow_key.is_none_or(|k| (k.0 - key.0).abs() > 1e-4 || (k.1 - key.1).abs() > 1e-4 || k.2 != key.2);
        if moved {
            self.renderer.render_shadow(&sh.gpu, &view, &sh.player.world);
            sh.shadow_key = Some(key);
        }
    }

    pub fn force_shadow(&mut self) {
        if let Some(sh) = &mut self.viewer.shown {
            sh.shadow_key = None;
        }
    }

    fn thumb(&mut self, index: usize) -> Option<Texture2D> {
        if !self.thumbs_tried[index] {
            self.thumbs_tried[index] = true;
            let path = self.assets.join(format!("thumbs/{}.png", self.catalog.models[index].id));
            if path.is_file() {
                let c = CString::new(path.to_string_lossy().as_bytes()).unwrap_or_default();
                let mut t = unsafe { ffi::LoadTexture(c.as_ptr()) };
                if t.id != 0 {
                    unsafe {
                        ffi::GenTextureMipmaps(&mut t);
                        ffi::SetTextureFilter(t, ffi::TextureFilter::TEXTURE_FILTER_TRILINEAR as i32);
                    }
                    self.thumbs[index] = Some(t);
                }
            }
        }
        self.thumbs[index]
    }

    // ---- Drawing -------------------------------------------------------

    /// Draws the screen; `dest` is the canvas's place on the display in
    /// device pixels and `k` its scale.
    pub fn draw(&mut self, dest: Rectangle, k: f32) {
        self.canvas = (dest, k);
        match self.screen {
            Screen::Gallery | Screen::About => self.draw_gallery(dest, k),
            Screen::Viewer => self.draw_viewer(dest, k),
        }
    }

    fn begin_2d(dest: Rectangle, k: f32) {
        unsafe {
            ffi::BeginMode2D(ffi::Camera2D {
                offset: Vector2 { x: dest.x, y: dest.y },
                target: Vector2 { x: 0.0, y: 0.0 },
                rotation: 0.0,
                zoom: k,
            })
        };
    }

    fn end_2d() {
        unsafe { ffi::EndMode2D() };
    }

    fn tab_title(&self, tab: usize) -> String {
        match tab.checked_sub(1) {
            None => s(T::All, self.lang).to_string(),
            Some(k) => self.catalog.categories[k].title.get(self.lang).to_string(),
        }
    }

    fn draw_gallery(&mut self, dest: Rectangle, k: f32) {
        // Thumbnails load as their rows come into view.
        let list = self.list.clone();
        let scroll = self.scroll;
        let rows_visible = |row: usize| {
            let top = GRID_Y + row as f32 * ROW - scroll;
            top + CARD_H > GRID_Y - ROW && top < GRID_Y + GRID_H + ROW
        };
        let thumbs: Vec<Option<Texture2D>> = list
            .iter()
            .enumerate()
            .map(|(pos, &i)| if rows_visible(pos / COLS) { self.thumb(i) } else { None })
            .collect();
        let (xs, widths) = self.tab_layout();
        Self::begin_2d(dest, k);
        let f = &self.fonts;
        let lang = self.lang;
        ui::gradient(rect(0.0, 0.0, W, H), ui::BACK_TOP, ui::BACK_BOTTOM);
        f.draw(s(T::AppName, lang), GRID_X, 16.0, 28.0, ui::TEXT, Align::Left, Weight::Display);
        let total = text::count(self.catalog.models.len(), lang);
        f.text(&total, W - GRID_X, 20.0, 13.0, ui::MUTED, Align::Right);
        f.text(s(T::OpenLicenses, lang), W - GRID_X, 37.0, 11.0, ui::FAINT, Align::Right);

        // Category tabs, L and R at either end.
        let tabs = self.catalog.categories.len() + 1;
        let titles: Vec<String> = (0..tabs).map(|t| self.tab_title(t)).collect();
        let left = TAB_LEFT;
        ui::badge(f, "L", GRID_X - 2.0, 74.0, 0.9);
        ui::badge(f, "R", W - GRID_X - 11.0, 74.0, 0.9);
        let (ox, oy) = (dest.x, dest.y);
        let tab_scroll = self.tab_scroll;
        let tab = self.tab;
        ui::clipped(rect(left - 2.0, 56.0, TAB_SPAN + 4.0, 36.0), k, (ox, oy), || {
            for t in 0..tabs {
                let x = left + xs[t] - tab_scroll;
                let selected = t == tab;
                let color = if selected { ui::TEXT } else { ui::FAINT };
                f.text(&titles[t], x, 66.0, 15.0, color, Align::Left);
                if selected {
                    ui::fill(rect(x, 88.0, widths[t], 2.0), ui::SUN);
                }
            }
        });
        ui::fill(rect(GRID_X, 91.0, W - 2.0 * GRID_X, 1.0), ui::LINE);

        // The grid of cards.
        let focus = self.focus;
        let time = self.time;
        let quiet = self.gallery_quiet;
        ui::clipped(rect(0.0, GRID_Y - 8.0, W, GRID_H + 8.0), k, (ox, oy), || {
            for (pos, &i) in list.iter().enumerate() {
                let (col, row) = (pos % COLS, pos / COLS);
                if !rows_visible(row) {
                    continue;
                }
                let x = GRID_X + col as f32 * (CARD_W + GAP);
                let y = GRID_Y + row as f32 * ROW - scroll;
                let focused = pos == focus;
                let r = rect(x, y, CARD_W, CARD_H);
                ui::panel(r, if focused { ui::CARD_FOCUS } else { ui::CARD });
                let thumb_rect = rect(x + 4.0, y + 4.0, CARD_W - 8.0, THUMB_H - 6.0);
                match thumbs[pos] {
                    Some(t) => ui::picture(t, thumb_rect, ui::rgb(0xffffff)),
                    None => {
                        let m = &self.catalog.models[i];
                        let letter: String = m.title.get(lang).chars().take(1).collect();
                        f.draw(
                            &letter,
                            x + CARD_W / 2.0,
                            y + THUMB_H / 2.0 - 18.0,
                            28.0,
                            ui::FAINT,
                            Align::Center,
                            Weight::Display,
                        );
                    }
                }
                let m = &self.catalog.models[i];
                let title_color = if focused { ui::TEXT } else { ui::mix(ui::TEXT, ui::MUTED, 0.4) };
                let title = f.fit(m.title.get(lang), 13.0, CARD_W - 16.0, Weight::Text);
                f.text(&title, x + 8.0, y + THUMB_H + 3.0, 13.0, title_color, Align::Left);
                let sub =
                    if m.stats.animations.is_empty() { m.collection.clone() } else { format!("▶ {}", m.collection) };
                let sub = f.fit(&sub, 11.0, CARD_W - 16.0, Weight::Text);
                f.text(&sub, x + 8.0, y + THUMB_H + 20.0, 11.0, ui::FAINT, Align::Left);
                if focused {
                    let pulse = if quiet < 4.0 { 0.75 + 0.25 * (time * 3.0).sin() } else { 1.0 };
                    ui::outline(r, 2.0, ui::alpha(ui::SUN, pulse));
                }
            }
        });
        // A scroll hint at the bottom edge when more rows wait below.
        let rows = list.len().div_ceil(COLS);
        if rows as f32 * ROW - self.scroll > GRID_H + 4.0 {
            ui::gradient(rect(0.0, GRID_Y + GRID_H - 28.0, W, 28.0), ui::alpha(ui::BACK_BOTTOM, 0.0), ui::BACK_BOTTOM);
        }

        ui::fill(rect(0.0, H - 34.0, W, 34.0), ui::BACK_BOTTOM);
        ui::fill(rect(GRID_X, H - 34.0, W - 2.0 * GRID_X, 1.0), ui::LINE);
        let a = crate::input::label_for;
        ui::hints(
            f,
            &[
                (&["dpad"], s(T::Choose, lang)),
                (&[a(Button::A)], s(T::View, lang)),
                (&["L", "R"], s(T::Category, lang)),
                (&["Start"], s(T::About, lang)),
                (&[a(Button::B)], s(T::Quit, lang)),
            ],
            W / 2.0,
            H - 17.0,
            1.0,
        );
        if self.screen == Screen::About {
            self.draw_about();
        }
        self.draw_toast();
        Self::end_2d();
    }

    fn draw_toast(&self) {
        if let Some((msg, left)) = &self.toast {
            let a = (left / 0.3).min(1.0);
            ui::toast(&self.fonts, msg, W / 2.0, H - 72.0, a);
        }
        if self.quit_hold > 0.0 {
            ui::toast(&self.fonts, s(T::HoldToQuit, self.lang), W / 2.0, H / 2.0, 1.0);
        }
    }

    fn draw_about(&mut self) {
        let lang = self.lang;
        ui::fill(rect(0.0, 0.0, W, H), ui::alpha(ui::INK, 0.82));
        let panel = rect(56.0, 34.0, W - 112.0, H - 68.0);
        ui::panel(panel, ui::rgb(0x15171d));
        ui::outline(panel, 1.0, ui::LINE);
        let f = &self.fonts;
        let x = panel.x + 26.0;
        let width = panel.width - 52.0;
        let mut y = panel.y + 22.0 - self.about_scroll;
        let top = panel.y + 8.0;
        let bottom = panel.y + panel.height - 8.0;
        let mut lines: Vec<(String, f32, ffi::Color, Weight, f32)> = Vec::new();
        lines.push((s(T::AboutTitle, lang).into(), 28.0, ui::TEXT, Weight::Display, 40.0));
        for l in f.wrap(s(T::AboutBody, lang), 15.0, width, Weight::Text) {
            lines.push((l, 15.0, ui::MUTED, Weight::Text, 22.0));
        }
        lines.push((s(T::OpenLicenses, lang).into(), 13.0, ui::SUN, Weight::Text, 30.0));
        lines.push((s(T::Sources, lang).into(), 15.0, ui::TEXT, Weight::Text, 24.0));
        let mut collections: Vec<(String, usize, Vec<String>)> = Vec::new();
        for m in &self.catalog.models {
            match collections.iter_mut().find(|c| c.0 == m.collection) {
                Some(c) => {
                    c.1 += 1;
                    for l in &m.licenses {
                        if !c.2.contains(l) {
                            c.2.push(l.clone());
                        }
                    }
                }
                None => collections.push((m.collection.clone(), 1, m.licenses.clone())),
            }
        }
        for (name, n, licenses) in collections {
            let lic = licenses
                .iter()
                .map(|l| l.replace("-1.0", " 1.0").replace("-4.0", " 4.0").replace('-', " "))
                .collect::<Vec<_>>()
                .join(" / ");
            lines.push((format!("{name}  ·  {}  ·  {lic}", text::count(n, lang)), 13.0, ui::MUTED, Weight::Text, 20.0));
        }
        lines.push((String::new(), 13.0, ui::MUTED, Weight::Text, 8.0));
        for l in f.wrap(s(T::AboutCredits, lang), 13.0, width, Weight::Text) {
            lines.push((l, 13.0, ui::MUTED, Weight::Text, 20.0));
        }
        lines.push((String::new(), 13.0, ui::MUTED, Weight::Text, 8.0));
        lines.push((s(T::AboutCode, lang).into(), 13.0, ui::MUTED, Weight::Text, 20.0));
        let version = option_env!("GIZLET_APP_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
        lines.push((format!("github.com/gizlet/models-viewer  ·  v{version}"), 13.0, ui::FAINT, Weight::Text, 20.0));
        let start = y;
        for (line, size, color, weight, advance) in &lines {
            if y > top - 40.0 && y < bottom {
                let fade = ((y - top) / 14.0).min((bottom - y - size) / 14.0).clamp(0.0, 1.0);
                f.draw(line, x, y, *size, ui::alpha(*color, fade), Align::Left, *weight);
            }
            y += advance;
        }
        self.about_height = y - start;
        ui::fill(rect(0.0, H - 34.0, W, 34.0), ui::INK);
        ui::hints(
            f,
            &[(&["dpad"], s(T::Scroll, lang)), (&[crate::input::label_for(Button::B)], s(T::Back, lang))],
            W / 2.0,
            H - 17.0,
            1.0,
        );
    }

    fn viewport(&self, dest: Rectangle, k: f32) -> (i32, i32, i32, i32) {
        let panel = PANEL_W * self.viewer.info_k * k;
        let screen_h = self.target.1 as f32;
        let w = (dest.width - panel).max(1.0);
        // rlViewport counts y from the bottom of the target.
        let y = screen_h - (dest.y + dest.height);
        (dest.x.round() as i32, y.round() as i32, w.round() as i32, dest.height.round() as i32)
    }

    pub fn stage_viewport(&self, dest: Rectangle, k: f32) -> (i32, i32, i32, i32) {
        self.viewport(dest, k)
    }

    fn draw_viewer(&mut self, dest: Rectangle, k: f32) {
        let lang = self.lang;
        Self::begin_2d(dest, k);
        ui::gradient(rect(0.0, 0.0, W, H), ui::STAGE_TOP, ui::STAGE_BOTTOM);
        let cx = (W - PANEL_W * self.viewer.info_k) / 2.0;
        unsafe {
            ffi::DrawCircleGradient(
                Vector2 { x: cx, y: H * 0.44 },
                260.0,
                ui::alpha(ui::rgb(0x4a5060), 0.55),
                ui::alpha(ui::rgb(0x4a5060), 0.0),
            );
        }
        Self::end_2d();

        let viewport = self.viewport(dest, k);
        if let (Some(view), Some(sh)) = (self.view(viewport), self.viewer.shown.as_ref()) {
            let mut stage = self.stage;
            let a = sh.appear * sh.appear * (3.0 - 2.0 * sh.appear);
            stage.floor[3] = a;
            self.renderer.draw_floor(&sh.gpu, &view, &stage);
            let dim = if self.viewer.loading() { 0.45 } else { 1.0 };
            stage.exposure *= (0.35 + 0.65 * a) * dim;
            self.renderer.draw_model(&sh.gpu, &view, &sh.player.world, &stage);
            gpu::restore_viewport(self.target.0, self.target.1);
        }

        Self::begin_2d(dest, k);
        self.draw_viewer_overlay(lang);
        Self::end_2d();
    }

    fn draw_viewer_overlay(&mut self, lang: Lang) {
        let v = &self.viewer;
        let index = v.index;
        let (info_k, loading, failed, load_started) = (v.info_k, v.loading(), v.error.is_some(), v.load_started);
        let m = self.catalog.models[index].clone();
        let hud = if v.info || v.loading() { 1.0 } else { 1.0 - ((v.quiet - 3.5) / 0.6).clamp(0.0, 1.0) };
        let title_a = 0.6 + 0.4 * hud;
        let f = &self.fonts;
        let left = 24.0;
        let max_w = W - PANEL_W * info_k - 2.0 * left - 70.0;
        let title_lines = f.wrap(m.title.get(lang), 28.0, max_w, Weight::Display);
        let mut y = 16.0;
        for line in title_lines.iter().take(2) {
            f.draw(line, left, y, 28.0, ui::alpha(ui::TEXT, title_a), Align::Left, Weight::Display);
            y += 32.0;
        }
        let byline = format!("{}  ·  {}", m.collection, m.license_text());
        f.text(&byline, left, y + 2.0, 13.0, ui::alpha(ui::MUTED, title_a), Align::Left);
        let pos = self.list.iter().position(|&i| i == index).unwrap_or(0);
        let counter = format!("{} / {}", pos + 1, self.list.len());
        let right = W - PANEL_W * info_k - left;
        f.text(&counter, right, 20.0, 13.0, ui::alpha(ui::MUTED, title_a), Align::Right);
        let style = s(style_name(self.renderer.style), lang);
        f.text(style, right, 38.0, 11.0, ui::alpha(ui::FAINT, title_a), Align::Right);

        if let Some(sh) = &self.viewer.shown {
            if let Some(clip) = sh.clip.filter(|_| sh.index == index) {
                let name = format!("▶ {}", pretty_clip(&sh.scene.animations[clip].name));
                f.text(&name, left, H - 62.0, 13.0, ui::alpha(ui::SUN, 0.5 + 0.5 * hud), Align::Left);
            }
        }
        let cx = (W - PANEL_W * info_k) / 2.0;
        if loading {
            let t = self.time - load_started;
            let a = ((t - 0.15) / 0.2).clamp(0.0, 1.0);
            ui::spinner(cx, H * 0.46, 16.0, self.time, a);
            f.text(s(T::Loading, lang), cx, H * 0.46 + 26.0, 13.0, ui::alpha(ui::MUTED, a), Align::Center);
        } else if failed {
            f.text(s(T::LoadFailed, lang), cx, H * 0.46, 15.0, ui::MUTED, Align::Center);
        }

        if info_k > 0.01 {
            self.draw_info(&m, lang);
        }
        let f = &self.fonts;
        if hud > 0.0 {
            ui::gradient(rect(0.0, H - 40.0, W, 40.0), ui::alpha(ui::INK, 0.0), ui::alpha(ui::INK, 0.75 * hud));
            let b = crate::input::label_for;
            let animated = self.viewer.shown.as_ref().is_some_and(|s| !s.scene.animations.is_empty());
            let a_label = [b(Button::A)];
            let b_label = [b(Button::B)];
            let mut items: Vec<(&[&str], &str)> = Vec::new();
            if info_k > 0.5 {
                items.push((&["dpad"], s(T::Rotate, lang)));
                items.push((&["dpad"], s(T::Scroll, lang)));
                items.push((&["L", "R"], s(T::Switch, lang)));
                items.push((&["X"], s(T::Info, lang)));
            } else {
                items.push((&["dpad"], s(T::Rotate, lang)));
                items.push((&["L2", "R2"], s(T::Zoom, lang)));
                items.push((&["L", "R"], s(T::Switch, lang)));
                items.push((&a_label, s(T::AutoRotate, lang)));
                items.push((&["X"], s(T::Info, lang)));
                items.push((&["Y"], s(T::Style, lang)));
                if animated {
                    items.push((&["Select"], s(T::Animation, lang)));
                }
            }
            items.push((&b_label, s(T::Back, lang)));
            ui::hints(f, &items, W / 2.0, H - 17.0, hud);
        }
        self.draw_toast();
    }

    fn draw_info(&mut self, m: &Model, lang: Lang) {
        let v = &self.viewer;
        let k = v.info_k;
        let x0 = W - PANEL_W * k;
        ui::fill(rect(x0, 0.0, PANEL_W, H), ui::alpha(ui::rgb(0x101116), 0.94));
        ui::fill(rect(x0, 0.0, 1.0, H), ui::LINE);
        let (dest, zoom) = self.canvas;
        // The panel scrolls under a clear strip that keeps the hints readable.
        unsafe {
            ffi::BeginScissorMode(
                (dest.x + x0 * zoom).round() as i32,
                dest.y.round() as i32,
                (PANEL_W * zoom).round() as i32,
                ((H - 44.0) * zoom).round() as i32,
            );
        }
        let f = &self.fonts;
        let x = x0 + 20.0;
        let width = PANEL_W - 40.0;
        let mut y = 20.0 - v.info_scroll;
        let start = y;
        let category = self
            .catalog
            .categories
            .iter()
            .find(|c| c.id == m.category)
            .map_or(String::new(), |c| c.title.get(lang).to_string());
        let mut rows: Vec<(String, Vec<String>)> = Vec::new();
        rows.push((
            s(T::Authors, lang).into(),
            m.authors.iter().flat_map(|a| f.wrap(a, 13.0, width, Weight::Text)).collect(),
        ));
        rows.push((s(T::License, lang).into(), vec![m.license_text()]));
        let source = m.source.trim_start_matches("https://").to_string();
        rows.push((s(T::Source, lang).into(), f.wrap(&source, 11.0, width, Weight::Text)));
        rows.push((s(T::Size, lang).into(), vec![text::dimensions(m.stats.size)]));
        let anims = m.stats.animations.len();
        let figures: Vec<(T, String)> = [
            (T::Triangles, text::grouped(m.stats.triangles as u64)),
            (T::Vertices, text::grouped(m.stats.vertices as u64)),
            (T::Materials, m.stats.materials.to_string()),
            (T::Textures, m.stats.textures.to_string()),
            (T::Joints, m.stats.joints.to_string()),
            (T::Animations, anims.to_string()),
            (T::File, text::bytes(m.stats.bytes)),
        ]
        .into_iter()
        .filter(|(label, _)| !(*label == T::Joints && m.stats.joints == 0))
        .collect();
        for line in f.wrap(m.title.get(lang), 22.0, width, Weight::Display) {
            f.draw(&line, x, y, 22.0, ui::TEXT, Align::Left, Weight::Display);
            y += 26.0;
        }
        f.text(&category, x, y + 2.0, 13.0, ui::SUN, Align::Left);
        y += 30.0;
        for (label, values) in &rows {
            f.text(label, x, y, 11.0, ui::FAINT, Align::Left);
            y += 15.0;
            let small = label == s(T::Source, lang);
            for value in values {
                f.text(value, x, y, if small { 11.0 } else { 13.0 }, ui::TEXT, Align::Left);
                y += if small { 15.0 } else { 18.0 };
            }
            y += 8.0;
        }
        // Figures in two columns.
        let col = width / 2.0;
        for (i, (label, value)) in figures.iter().enumerate() {
            let cx = x + if i % 2 == 0 { 0.0 } else { col };
            let row_y = y + (i / 2) as f32 * 36.0;
            f.text(s(*label, lang), cx, row_y, 11.0, ui::FAINT, Align::Left);
            f.text(value, cx, row_y + 15.0, 13.0, ui::TEXT, Align::Left);
        }
        y += figures.len().div_ceil(2) as f32 * 36.0 + 6.0;
        f.text(s(T::Changes, lang), x, y, 11.0, ui::FAINT, Align::Left);
        y += 15.0;
        for line in f.wrap(m.changes.get(lang), 11.0, width, Weight::Text) {
            f.text(&line, x, y, 11.0, ui::MUTED, Align::Left);
            y += 15.0;
        }
        let height = y - start + 30.0;
        unsafe { ffi::EndScissorMode() };
        self.viewer.info_height = height;
    }
}

/// Clip names as their authors wrote them, with underscores as spaces.
pub fn pretty_clip(name: &str) -> String {
    name.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_moves_stay_inside() {
        assert_eq!(grid_move(0, 10, 4, Dir::Left), 0);
        assert_eq!(grid_move(3, 10, 4, Dir::Right), 4);
        assert_eq!(grid_move(9, 10, 4, Dir::Right), 9);
        assert_eq!(grid_move(2, 10, 4, Dir::Up), 2);
        assert_eq!(grid_move(5, 10, 4, Dir::Up), 1);
        assert_eq!(grid_move(7, 10, 4, Dir::Down), 9, "a short last row catches the move");
        assert_eq!(grid_move(9, 10, 4, Dir::Down), 9);
        assert_eq!(grid_move(1, 10, 4, Dir::Down), 5);
        assert_eq!(grid_move(0, 0, 4, Dir::Down), 0);
    }

    #[test]
    fn scrolling_keeps_the_focused_row_in_view() {
        assert_eq!(scroll_for(0, 0.0, GRID_H), 0.0);
        let s = scroll_for(5, 0.0, GRID_H);
        let top = 5.0 * ROW - s;
        assert!(top >= 0.0 && top + CARD_H <= GRID_H);
        let back = scroll_for(1, s, GRID_H);
        assert!(1.0 * ROW - back >= 0.0);
    }

    #[test]
    fn idle_clips_come_first() {
        assert_eq!(default_clip(&[]), None);
        assert_eq!(default_clip(&["Walk", "Idle", "Run"]), Some(1));
        assert_eq!(default_clip(&["Run", "Survey"]), Some(1));
        assert_eq!(default_clip(&["Run", "2H_Melee_Idle"]), Some(1));
        assert_eq!(default_clip(&["Run", "Walk"]), Some(0));
    }

    #[test]
    fn styles_round_trip_by_name() {
        for s in Style::ALL {
            assert_eq!(style_from(style_key(s)), s);
        }
        assert_eq!(style_from("nonsense"), Style::Studio);
    }
}
