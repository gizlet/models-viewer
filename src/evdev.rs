//! The handheld's controls read straight from evdev on Gizlet OS. Taken from
//! gizlet/os's launcher (`launcher/src/evdev.rs`).
//!
//! The R36S reports its buttons and sticks as `r36s_Gamepad` and its volume
//! keys as `gpio-keys-vol`. raylib's DRM platform only knows a fixed
//! mapping, so the game reads Linux's standard gamepad codes itself:
//! BTN_EAST is A, BTN_SOUTH B, BTN_NORTH X and BTN_WEST Y. Keyboards are
//! left to raylib. Devices are rescanned every two seconds for hotplug.

use crate::input::{Button, RawPad};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const KEY_CNT: usize = 0x300;

const KEY_A: usize = 30;
const KEY_VOLUMEDOWN: usize = 114;
const KEY_VOLUMEUP: usize = 115;
const KEY_POWER: usize = 116;
const BTN_SOUTH: usize = 0x130;
const BTN_EAST: usize = 0x131;
const BTN_NORTH: usize = 0x133;
const BTN_WEST: usize = 0x134;
const BTN_TL: usize = 0x136;
const BTN_TR: usize = 0x137;
const BTN_TL2: usize = 0x138;
const BTN_TR2: usize = 0x139;
const BTN_SELECT: usize = 0x13a;
const BTN_START: usize = 0x13b;
const BTN_MODE: usize = 0x13c;
const BTN_THUMBL: usize = 0x13d;
const BTN_THUMBR: usize = 0x13e;
const BTN_DPAD_UP: usize = 0x220;
const BTN_DPAD_DOWN: usize = 0x221;
const BTN_DPAD_LEFT: usize = 0x222;
const BTN_DPAD_RIGHT: usize = 0x223;

const ABS_X: usize = 0x00;
const ABS_Y: usize = 0x01;
const ABS_Z: usize = 0x02;
const ABS_RX: usize = 0x03;
const ABS_RY: usize = 0x04;
const ABS_RZ: usize = 0x05;
const ABS_HAT0X: usize = 0x10;
const ABS_HAT0Y: usize = 0x11;
const ABS_CNT: usize = 0x40;

const MAP: [(usize, Button); 20] = [
    (BTN_DPAD_UP, Button::Up),
    (BTN_DPAD_DOWN, Button::Down),
    (BTN_DPAD_LEFT, Button::Left),
    (BTN_DPAD_RIGHT, Button::Right),
    (BTN_EAST, Button::A),
    (BTN_SOUTH, Button::B),
    (BTN_NORTH, Button::X),
    (BTN_WEST, Button::Y),
    (BTN_TL, Button::L1),
    (BTN_TR, Button::R1),
    (BTN_TL2, Button::L2),
    (BTN_TR2, Button::R2),
    (BTN_SELECT, Button::Select),
    (BTN_START, Button::Start),
    (BTN_MODE, Button::Home),
    (BTN_THUMBL, Button::L3),
    (BTN_THUMBR, Button::R3),
    (KEY_VOLUMEUP, Button::VolumeUp),
    (KEY_VOLUMEDOWN, Button::VolumeDown),
    (KEY_POWER, Button::Power),
];

/// `_IOC(_IOC_READ, 'E', nr, size)`.
fn eviocg(nr: u64, size: usize) -> u64 {
    (2 << 30) | ((size as u64) << 16) | ((b'E' as u64) << 8) | nr
}

fn bit(bits: &[u8], n: usize) -> bool {
    bits.get(n / 8).is_some_and(|b| b & (1 << (n % 8)) != 0)
}

#[derive(Clone, Copy, Default)]
struct Abs {
    value: i32,
    min: i32,
    max: i32,
    flat: i32,
}

impl Abs {
    fn read(fd: libc::c_int, code: usize) -> Option<Self> {
        let mut info: libc::input_absinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info);
        let result = unsafe { libc::ioctl(fd, eviocg(0x40 + code as u64, size) as _, &mut info) };
        (result == 0).then_some(Abs { value: info.value, min: info.minimum, max: info.maximum, flat: info.flat })
    }

    /// -1..1 around the middle of the range, with the driver's dead zone.
    fn centered(self) -> f32 {
        let half = (self.max - self.min) as f32 / 2.0;
        if half <= 0.0 {
            return 0.0;
        }
        let v = self.value as f32 - (self.min as f32 + half);
        if v.abs() <= self.flat as f32 {
            0.0
        } else {
            (v / half).clamp(-1.0, 1.0)
        }
    }

    /// 0..1 from the bottom of the range.
    fn ranged(self) -> f32 {
        let span = (self.max - self.min) as f32;
        if span <= 0.0 {
            0.0
        } else {
            ((self.value - self.min) as f32 / span).clamp(0.0, 1.0)
        }
    }
}

struct Device {
    path: String,
    fd: OwnedFd,
    gamepad: bool,
    keys: Vec<bool>,
    abs: [Abs; ABS_CNT],
    has_abs: [bool; ABS_CNT],
    awaiting_report: bool,
}

impl Device {
    fn read_pressed_keys(fd: libc::c_int) -> Option<Vec<bool>> {
        let mut held = [0u8; KEY_CNT / 8];
        let result = unsafe { libc::ioctl(fd, eviocg(0x18, held.len()) as _, held.as_mut_ptr()) };
        (result >= 0).then(|| (0..KEY_CNT).map(|k| bit(&held, k)).collect())
    }

    fn open(path: &str) -> Option<Device> {
        let c = std::ffi::CString::new(path).ok()?;
        let raw = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        if raw < 0 {
            return None;
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let mut keys = [0u8; KEY_CNT / 8];
        if unsafe { libc::ioctl(raw, eviocg(0x20 + EV_KEY as u64, keys.len()) as _, keys.as_mut_ptr()) } < 0 {
            return None;
        }
        let gamepad =
            (BTN_SOUTH..=BTN_THUMBR).any(|b| bit(&keys, b)) || (BTN_DPAD_UP..=BTN_DPAD_RIGHT).any(|b| bit(&keys, b));
        let extra = [KEY_VOLUMEUP, KEY_VOLUMEDOWN, KEY_POWER].iter().any(|&k| bit(&keys, k));
        // Keyboards are raylib's.
        if bit(&keys, KEY_A) || !(gamepad || extra) {
            return None;
        }
        let mut abs_bits = [0u8; ABS_CNT / 8];
        unsafe { libc::ioctl(raw, eviocg(0x20 + EV_ABS as u64, abs_bits.len()) as _, abs_bits.as_mut_ptr()) };
        let mut abs = [Abs::default(); ABS_CNT];
        let mut has_abs = [false; ABS_CNT];
        for (code, slot) in abs.iter_mut().enumerate() {
            if !bit(&abs_bits, code) {
                continue;
            }
            if let Some(state) = Abs::read(raw, code) {
                *slot = state;
                has_abs[code] = true;
            }
        }
        let keys = Self::read_pressed_keys(raw).unwrap_or_else(|| vec![false; KEY_CNT]);
        Some(Device { path: path.to_string(), fd, gamepad, keys, abs, has_abs, awaiting_report: false })
    }

    /// Reads EVIOCGKEY and EVIOCGABS after the event stream has lost state.
    fn refresh_state(&mut self) -> bool {
        let fd = self.fd.as_raw_fd();
        let Some(keys) = Self::read_pressed_keys(fd) else { return false };
        for (code, abs) in self.abs.iter_mut().enumerate() {
            if self.has_abs[code] {
                let Some(state) = Abs::read(fd, code) else { return false };
                *abs = state;
            }
        }
        self.keys = keys;
        true
    }

    /// Applies pending events. False when the device is gone or cannot resync.
    fn read(&mut self) -> bool {
        let mut events: [libc::input_event; 64] = unsafe { std::mem::zeroed() };
        loop {
            let size = std::mem::size_of_val(&events);
            let n = unsafe { libc::read(self.fd.as_raw_fd(), events.as_mut_ptr().cast(), size) };
            if n < 0 {
                return std::io::Error::last_os_error().raw_os_error() == Some(libc::EAGAIN);
            }
            if n == 0 {
                return false;
            }
            let count = n as usize / std::mem::size_of::<libc::input_event>();
            if !self.apply_events(&events[..count], Self::refresh_state) {
                return false;
            }
            if count < events.len() {
                return true;
            }
        }
    }

    fn apply_events(&mut self, events: &[libc::input_event], refresh_state: impl FnOnce(&mut Self) -> bool) -> bool {
        let mut refresh = false;
        for e in events {
            match (e.type_, e.code) {
                (EV_SYN, SYN_DROPPED) => self.awaiting_report = true,
                (EV_SYN, SYN_REPORT) if self.awaiting_report => {
                    self.awaiting_report = false;
                    refresh = true;
                }
                _ if self.awaiting_report || refresh => {}
                (EV_KEY, code) if (code as usize) < KEY_CNT && e.value != 2 => self.keys[code as usize] = e.value != 0,
                (EV_ABS, code) if (code as usize) < ABS_CNT => self.abs[code as usize].value = e.value,
                _ => {}
            }
        }
        // The snapshot also covers events already buffered after SYN_REPORT.
        // If another overflow is in progress, wait for its report first.
        !refresh || self.awaiting_report || refresh_state(self)
    }

    fn apply(&self, pad: &mut RawPad) {
        if self.awaiting_report {
            return;
        }
        for (code, b) in MAP {
            if self.keys[code] {
                pad.set(b);
            }
        }
        if self.has_abs[ABS_HAT0X] {
            let v = self.abs[ABS_HAT0X].value;
            if v < 0 {
                pad.set(Button::Left);
            } else if v > 0 {
                pad.set(Button::Right);
            }
        }
        if self.has_abs[ABS_HAT0Y] {
            let v = self.abs[ABS_HAT0Y].value;
            if v < 0 {
                pad.set(Button::Up);
            } else if v > 0 {
                pad.set(Button::Down);
            }
        }
        for (slot, code) in [ABS_X, ABS_Y, ABS_RX, ABS_RY].into_iter().enumerate() {
            if self.has_abs[code] {
                pad.merge_axis(slot, self.abs[code].centered());
            }
        }
        for (slot, code) in [(4, ABS_Z), (5, ABS_RZ)] {
            if self.has_abs[code] {
                pad.merge_axis(slot, self.abs[code].ranged());
            }
        }
    }
}

#[derive(Default)]
pub struct Evdev {
    devices: Vec<Device>,
    scanned: Option<f64>,
}

impl Evdev {
    pub fn poll(&mut self, time: f64) {
        if self.scanned.is_none_or(|t| time - t > 2.0) {
            self.scanned = Some(time);
            self.scan();
        }
        self.devices.retain_mut(Device::read);
    }

    fn scan(&mut self) {
        let Ok(dir) = std::fs::read_dir("/dev/input") else { return };
        let mut paths: Vec<String> = dir
            .flatten()
            .map(|e| e.path().to_string_lossy().into_owned())
            .filter(|p| p.rsplit('/').next().is_some_and(|n| n.starts_with("event")))
            .collect();
        paths.sort();
        for path in paths {
            if !self.devices.iter().any(|d| d.path == path) {
                if let Some(d) = Device::open(&path) {
                    self.devices.push(d);
                }
            }
        }
    }

    /// Adds the devices' state to `pad`; true when a gamepad is among them.
    pub fn state(&self, pad: &mut RawPad) -> bool {
        let mut gamepad = false;
        for d in &self.devices {
            d.apply(pad);
            gamepad |= d.gamepad;
        }
        gamepad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(type_: u16, code: usize, value: i32) -> libc::input_event {
        libc::input_event { type_, code: code as u16, value, ..unsafe { std::mem::zeroed() } }
    }

    #[test]
    fn dropped_release_recovers_buttons_and_axes_across_reads() {
        let fd = std::fs::File::open("/dev/null").unwrap().into();
        let mut device = Device {
            path: String::new(),
            fd,
            gamepad: true,
            keys: vec![false; KEY_CNT],
            abs: [Abs::default(); ABS_CNT],
            has_abs: [false; ABS_CNT],
            awaiting_report: false,
        };
        device.has_abs[ABS_X] = true;
        device.abs[ABS_X] = Abs { min: -100, max: 100, ..Abs::default() };
        let initial = [event(EV_KEY, BTN_DPAD_RIGHT, 1), event(EV_ABS, ABS_X, 75)];
        assert!(device.apply_events(&initial, |_| panic!("unexpected resync")));
        let mut pad = RawPad::default();
        device.apply(&mut pad);
        assert!(pad.buttons[Button::Right as usize]);
        assert_eq!(pad.axes[0], 0.75);

        // Releases were lost. No stale state is exposed while awaiting the report.
        let dropped = [event(EV_SYN, SYN_DROPPED as usize, 0), event(EV_KEY, BTN_DPAD_LEFT, 1)];
        assert!(device.apply_events(&dropped, |_| panic!("resync must wait for SYN_REPORT")));
        assert!(!device.keys[BTN_DPAD_LEFT]);
        let mut pad = RawPad::default();
        device.apply(&mut pad);
        assert_eq!(pad.buttons, [false; crate::input::BUTTONS]);
        assert_eq!(pad.axes, [0.0; 6]);

        let report = [event(EV_SYN, SYN_REPORT as usize, 0), event(EV_ABS, ABS_X, -75)];
        let mut refreshed = false;
        assert!(device.apply_events(&report, |d| {
            refreshed = true;
            d.keys.fill(false);
            d.abs[ABS_X].value = 0;
            true
        }));
        assert!(refreshed);
        device.apply(&mut pad);
        assert_eq!(pad.buttons, [false; crate::input::BUTTONS]);
        assert_eq!(pad.axes, [0.0; 6]);

        let next = [event(EV_KEY, BTN_DPAD_LEFT, 1), event(EV_ABS, ABS_X, -50)];
        assert!(device.apply_events(&next, |_| panic!("unexpected resync")));
        device.apply(&mut pad);
        assert!(pad.buttons[Button::Left as usize]);
        assert_eq!(pad.axes[0], -0.5);
    }
}
