//! Buttons from the keyboard, the mouse, raylib's gamepads and, on Gizlet
//! OS, the handheld's own evdev devices, merged into one pad each frame.
//! Adapted from The Little Prince.
//!
//! Keyboard: arrows or WASD move; Enter, Space or K is A; Backspace or J is
//! B; I is X; U is Y; Q and E are L1 and R1; Z and C are L2 and R2; Esc or P
//! is Start; Tab is Select. On the desktop, dragging with the mouse turns the
//! model and the wheel zooms.

use raylib_sys::{self as ffi, GamepadAxis as Axis, GamepadButton as G, KeyboardKey as K};

/// A physical button. Face buttons are named by position, Nintendo style:
/// A right, B bottom, X top, Y left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(all(target_os = "linux", feature = "gizlet-os")), allow(dead_code))]
pub enum Button {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    Home,
    L3,
    R3,
    VolumeUp,
    VolumeDown,
    Power,
}

pub const BUTTONS: usize = 20;

/// The physical state of the controls this frame.
#[derive(Clone, Default)]
pub struct RawPad {
    pub buttons: [bool; BUTTONS],
    /// Left X/Y and right X/Y in -1..1, then the left and right triggers in 0..1.
    pub axes: [f32; 6],
}

impl RawPad {
    pub fn set(&mut self, b: Button) {
        self.buttons[b as usize] = true;
    }

    /// Keeps the strongest deflection reported for this axis.
    pub fn merge_axis(&mut self, slot: usize, value: f32) {
        let axis = &mut self.axes[slot];
        if value.abs() > axis.abs() {
            *axis = value;
        }
    }
}

const KEYS: [(K, Button); 22] = [
    (K::KEY_UP, Button::Up),
    (K::KEY_W, Button::Up),
    (K::KEY_DOWN, Button::Down),
    (K::KEY_S, Button::Down),
    (K::KEY_LEFT, Button::Left),
    (K::KEY_A, Button::Left),
    (K::KEY_RIGHT, Button::Right),
    (K::KEY_D, Button::Right),
    (K::KEY_SPACE, Button::A),
    (K::KEY_ENTER, Button::A),
    (K::KEY_K, Button::A),
    (K::KEY_BACKSPACE, Button::B),
    (K::KEY_J, Button::B),
    (K::KEY_I, Button::X),
    (K::KEY_U, Button::Y),
    (K::KEY_Q, Button::L1),
    (K::KEY_E, Button::R1),
    (K::KEY_Z, Button::L2),
    (K::KEY_C, Button::R2),
    (K::KEY_ESCAPE, Button::Start),
    (K::KEY_P, Button::Start),
    (K::KEY_TAB, Button::Select),
];

const PAD: [(G, Button); 14] = [
    (G::GAMEPAD_BUTTON_LEFT_FACE_UP, Button::Up),
    (G::GAMEPAD_BUTTON_LEFT_FACE_DOWN, Button::Down),
    (G::GAMEPAD_BUTTON_LEFT_FACE_LEFT, Button::Left),
    (G::GAMEPAD_BUTTON_LEFT_FACE_RIGHT, Button::Right),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_RIGHT, Button::A),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_DOWN, Button::B),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_UP, Button::X),
    (G::GAMEPAD_BUTTON_RIGHT_FACE_LEFT, Button::Y),
    (G::GAMEPAD_BUTTON_LEFT_TRIGGER_1, Button::L1),
    (G::GAMEPAD_BUTTON_RIGHT_TRIGGER_1, Button::R1),
    (G::GAMEPAD_BUTTON_LEFT_TRIGGER_2, Button::L2),
    (G::GAMEPAD_BUTTON_RIGHT_TRIGGER_2, Button::R2),
    (G::GAMEPAD_BUTTON_MIDDLE_LEFT, Button::Select),
    (G::GAMEPAD_BUTTON_MIDDLE_RIGHT, Button::Start),
];

const DIRS: [Button; 4] = [Button::Up, Button::Down, Button::Left, Button::Right];
const SHOULDERS: [Button; 2] = [Button::L1, Button::R1];

/// Menu auto-repeat: how long a button has now been held, and whether it
/// repeats this frame. It repeats only while still held, first after 0.38 s
/// and then every 0.11 s.
fn repeat(held: f32, down: bool, dt: f32) -> (f32, bool) {
    const FIRST: f32 = 0.38;
    const EVERY: f32 = 0.11;
    if !down {
        return (0.0, false);
    }
    let now = held + dt;
    let fires = held >= FIRST && ((held - FIRST) / EVERY).floor() != ((now - FIRST) / EVERY).floor();
    (now, fires)
}

/// A stick past its dead zone, ramping up from zero so small pushes give
/// fine control.
fn shaped(x: f32, y: f32, dead: f32) -> (f32, f32) {
    let m = x.hypot(y);
    if m > dead {
        let k = ((m - dead) / (1.0 - dead)).min(1.0);
        let k = k * k / m;
        (x * k, y * k)
    } else {
        (0.0, 0.0)
    }
}

pub struct Controls {
    cur: [bool; BUTTONS],
    prev: [bool; BUTTONS],
    /// Seconds each direction and shoulder button has been held, for auto-repeat.
    held: [f32; 6],
    repeat: [bool; 6],
    stick: (f32, f32),
    look: (f32, f32),
    triggers: (f32, f32),
    /// Mouse drag in canvas pixels and wheel steps this frame.
    pub drag: (f32, f32),
    pub wheel: f32,
    swap_ab: bool,
    /// Buttons a scripted run holds down.
    pub scripted: Vec<Button>,
    #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
    evdev: crate::evdev::Evdev,
}

impl Controls {
    pub fn new() -> Controls {
        Controls {
            cur: [false; BUTTONS],
            prev: [false; BUTTONS],
            held: [0.0; 6],
            repeat: [false; 6],
            stick: (0.0, 0.0),
            look: (0.0, 0.0),
            triggers: (0.0, 0.0),
            drag: (0.0, 0.0),
            wheel: 0.0,
            swap_ab: std::env::var("GIZLET_SWAP_AB").is_ok_and(|v| v == "1"),
            scripted: Vec::new(),
            #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
            evdev: crate::evdev::Evdev::default(),
        }
    }

    #[cfg_attr(not(all(target_os = "linux", feature = "gizlet-os")), allow(unused_variables))]
    pub fn poll(&mut self, dt: f32, time: f64, zoom: f32) {
        let mut pad = RawPad::default();
        unsafe {
            for (k, b) in KEYS {
                if ffi::IsKeyDown(k as i32) {
                    pad.set(b);
                }
            }
        }
        #[cfg(all(target_os = "linux", feature = "gizlet-os"))]
        let handheld = {
            self.evdev.poll(time);
            self.evdev.state(&mut pad)
        };
        #[cfg(not(all(target_os = "linux", feature = "gizlet-os")))]
        let handheld = false;
        if !handheld {
            unsafe {
                for g in 0..4 {
                    if !ffi::IsGamepadAvailable(g) {
                        continue;
                    }
                    for (gb, b) in PAD {
                        if ffi::IsGamepadButtonDown(g, gb as i32) {
                            pad.set(b);
                        }
                    }
                    for (slot, axis) in [
                        Axis::GAMEPAD_AXIS_LEFT_X,
                        Axis::GAMEPAD_AXIS_LEFT_Y,
                        Axis::GAMEPAD_AXIS_RIGHT_X,
                        Axis::GAMEPAD_AXIS_RIGHT_Y,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        pad.merge_axis(slot, ffi::GetGamepadAxisMovement(g, axis as i32));
                    }
                    for (slot, axis) in [(4, Axis::GAMEPAD_AXIS_LEFT_TRIGGER), (5, Axis::GAMEPAD_AXIS_RIGHT_TRIGGER)] {
                        // raylib reports triggers in -1..1, released at -1.
                        pad.merge_axis(slot, (ffi::GetGamepadAxisMovement(g, axis as i32) + 1.0) * 0.5);
                    }
                }
            }
        }
        if self.swap_ab {
            pad.buttons.swap(Button::A as usize, Button::B as usize);
        }
        for b in &self.scripted {
            pad.set(*b);
        }
        self.stick = shaped(pad.axes[0], pad.axes[1], 0.2);
        self.look = shaped(pad.axes[2], pad.axes[3], 0.18);
        self.triggers = (pad.axes[4], pad.axes[5]);
        let (x, y) = (pad.axes[0], pad.axes[1]);
        if y < -0.6 {
            pad.set(Button::Up);
        }
        if y > 0.6 {
            pad.set(Button::Down);
        }
        if x < -0.6 {
            pad.set(Button::Left);
        }
        if x > 0.6 {
            pad.set(Button::Right);
        }
        unsafe {
            self.drag = if ffi::IsMouseButtonDown(ffi::MouseButton::MOUSE_BUTTON_LEFT as i32) {
                let d = ffi::GetMouseDelta();
                (d.x / zoom, d.y / zoom)
            } else {
                (0.0, 0.0)
            };
            self.wheel = ffi::GetMouseWheelMove();
        }
        self.prev = self.cur;
        self.cur = pad.buttons;
        for (i, b) in DIRS.iter().chain(SHOULDERS.iter()).enumerate() {
            (self.held[i], self.repeat[i]) = repeat(self.held[i], self.cur[*b as usize], dt);
        }
    }

    pub fn down(&self, b: Button) -> bool {
        self.cur[b as usize]
    }

    pub fn pressed(&self, b: Button) -> bool {
        self.cur[b as usize] && !self.prev[b as usize]
    }

    /// Pressed, or repeating while held: for moving through lists.
    pub fn nav(&self, b: Button) -> bool {
        self.pressed(b) || DIRS.iter().chain(SHOULDERS.iter()).position(|d| *d == b).is_some_and(|i| self.repeat[i])
    }

    /// Any button went down this frame.
    pub fn any_pressed(&self) -> bool {
        (0..BUTTONS).any(|i| self.cur[i] && !self.prev[i])
    }

    /// Something is being held or moved.
    pub fn active(&self) -> bool {
        self.cur.iter().any(|&b| b)
            || self.stick != (0.0, 0.0)
            || self.look != (0.0, 0.0)
            || self.drag != (0.0, 0.0)
            || self.wheel != 0.0
    }

    /// Turning: the left stick when pushed, else the D-pad, right (+x) and down (+y).
    pub fn turn(&self) -> (f32, f32) {
        if self.stick != (0.0, 0.0) {
            return self.stick;
        }
        let axis = |neg: Button, pos: Button| self.down(pos) as i32 as f32 - self.down(neg) as i32 as f32;
        (axis(Button::Left, Button::Right), axis(Button::Up, Button::Down))
    }

    /// The right stick, right (+x) and down (+y).
    pub fn look(&self) -> (f32, f32) {
        self.look
    }

    /// Zooming in (+) or out (-): R2 and L2, analog triggers where present.
    pub fn zoom(&self) -> f32 {
        let r = if self.down(Button::R2) { 1.0 } else { self.triggers.1 };
        let l = if self.down(Button::L2) { 1.0 } else { self.triggers.0 };
        r - l
    }
}

fn swapped() -> bool {
    static SWAP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SWAP.get_or_init(|| std::env::var("GIZLET_SWAP_AB").is_ok_and(|v| v == "1"))
}

/// The label printed on the button that does `b`'s job, after the A/B swap.
pub fn label_for(b: Button) -> &'static str {
    match (b, swapped()) {
        (Button::A, false) | (Button::B, true) => "A",
        (Button::A, true) | (Button::B, false) => "B",
        (Button::X, _) => "X",
        (Button::Y, _) => "Y",
        (Button::L1, _) => "L",
        (Button::R1, _) => "R",
        (Button::L2, _) => "L2",
        (Button::R2, _) => "R2",
        (Button::Start, _) => "Start",
        (Button::Select, _) => "Select",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::{repeat, shaped, RawPad};

    #[test]
    fn idle_controller_preserves_active_sticks_and_triggers() {
        let active = [-0.7, 0.2, -0.9, 0.6, 0.5, 1.0];
        for active_first in [true, false] {
            let mut pad = RawPad::default();
            let idle = [0.0; 6];
            let controllers = if active_first { [active, idle] } else { [idle, active] };
            for axes in controllers {
                for (slot, value) in axes.into_iter().enumerate() {
                    pad.merge_axis(slot, value);
                }
            }
            assert_eq!(pad.axes, active);
        }
    }

    #[test]
    fn sticks_ignore_drift_and_reach_full_speed() {
        assert_eq!(shaped(0.1, 0.1, 0.2), (0.0, 0.0));
        let (x, y) = shaped(1.0, 0.0, 0.2);
        assert!((x - 1.0).abs() < 1e-6 && y == 0.0);
        let (x, _) = shaped(0.5, 0.0, 0.2);
        assert!(x > 0.0 && x < 0.5);
    }

    /// Counts repeats over `frames` held frames, then one released frame.
    fn run(frames: usize) -> (usize, bool) {
        let (mut held, mut n) = (0.0, 0);
        for _ in 0..frames {
            let (h, fires) = repeat(held, true, 1.0 / 60.0);
            held = h;
            n += fires as usize;
        }
        (n, repeat(held, false, 1.0 / 60.0).1)
    }

    #[test]
    fn repeats_while_held_only() {
        assert_eq!(run(20).0, 0, "no repeat before 0.38 s");
        let (n, on_release) = run(60);
        assert!((5..=6).contains(&n), "about one repeat per 0.11 s after 0.38 s, got {n}");
        assert!(!on_release, "releasing a held direction must not step the list");
    }
}
