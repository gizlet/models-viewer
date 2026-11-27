//! Animation playback: samples a clip's channels into each node's local
//! translation, rotation and scale, cross-fades from the previous clip, then
//! works out world matrices and the joint matrices skinned meshes need.

use crate::math::{quat_normalize, quat_slerp, Quat, M4, V3};
use crate::scene::{Bounds, Channel, Interp, Path, Scene};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Local {
    pub t: V3,
    pub r: Quat,
    pub s: V3,
}

impl Local {
    fn mix(&self, o: &Local, k: f32) -> Local {
        Local { t: self.t.lerp(o.t, k), r: quat_slerp(self.r, o.r, k), s: self.s.lerp(o.s, k) }
    }
}

/// Where in a channel's keys time `t` falls: the key before and the
/// fraction of the way to the next.
fn locate(times: &[f32], t: f32) -> (usize, f32) {
    if t <= times[0] || times.len() == 1 {
        return (0, 0.0);
    }
    let last = times.len() - 1;
    if t >= times[last] {
        return (last, 0.0);
    }
    let i = times.partition_point(|&k| k <= t) - 1;
    let span = times[i + 1] - times[i];
    (i, if span > 0.0 { (t - times[i]) / span } else { 0.0 })
}

/// The channel's value at time `t`, three or four floats.
pub fn sample(c: &Channel, t: f32) -> [f32; 4] {
    let width = if c.path == Path::Rotation { 4 } else { 3 };
    let (i, k) = locate(&c.times, t);
    let mut out = [0.0; 4];
    match c.interp {
        Interp::Step => out[..width].copy_from_slice(&c.values[i * width..i * width + width]),
        Interp::Linear => {
            let a = &c.values[i * width..i * width + width];
            let j = (i + 1).min(c.times.len() - 1);
            let b = &c.values[j * width..j * width + width];
            if c.path == Path::Rotation {
                out = quat_slerp([a[0], a[1], a[2], a[3]], [b[0], b[1], b[2], b[3]], k);
            } else {
                for n in 0..width {
                    out[n] = a[n] + (b[n] - a[n]) * k;
                }
            }
        }
        Interp::Cubic => {
            let j = (i + 1).min(c.times.len() - 1);
            let dt = if j > i { c.times[j] - c.times[i] } else { 0.0 };
            let key = |idx: usize, part: usize, n: usize| c.values[(idx * 3 + part) * width + n];
            let (k2, k3) = (k * k, k * k * k);
            let (h00, h10, h01, h11) = (2.0 * k3 - 3.0 * k2 + 1.0, k3 - 2.0 * k2 + k, -2.0 * k3 + 3.0 * k2, k3 - k2);
            for (n, v) in out.iter_mut().enumerate().take(width) {
                // Value of key i, its out-tangent, value of key j, its in-tangent.
                *v = h00 * key(i, 1, n) + h10 * dt * key(i, 2, n) + h01 * key(j, 1, n) + h11 * dt * key(j, 0, n);
            }
            if c.path == Path::Rotation {
                out = quat_normalize(out);
            }
        }
    }
    out
}

/// One playing clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub clip: usize,
    pub time: f32,
}

pub const FADE: f32 = 0.3;

pub struct Player {
    rest: Vec<Local>,
    pub local: Vec<Local>,
    scratch: Vec<Local>,
    pub world: Vec<M4>,
    pub current: Option<Track>,
    previous: Option<(Track, f32)>,
    /// Counts poses worked out, so a cached drawing knows when to follow.
    pub generation: u64,
}

impl Player {
    pub fn new(scene: &Scene) -> Player {
        let rest: Vec<Local> =
            scene.nodes.iter().map(|n| Local { t: n.translation, r: n.rotation, s: n.scale }).collect();
        let mut p = Player {
            local: rest.clone(),
            scratch: rest.clone(),
            world: vec![M4::IDENTITY; rest.len()],
            rest,
            current: None,
            previous: None,
            generation: 0,
        };
        p.update_world(scene);
        p
    }

    /// A clip is still fading out, toward another clip or the rest pose.
    pub fn fading(&self) -> bool {
        self.previous.is_some()
    }

    /// Starts a clip, fading from the one playing; `None` returns to rest.
    pub fn play(&mut self, clip: Option<usize>) {
        if clip == self.current.map(|t| t.clip) {
            return;
        }
        self.previous = self.current.map(|t| (t, FADE));
        self.current = clip.map(|clip| Track { clip, time: 0.0 });
    }

    fn apply(scene: &Scene, track: Track, out: &mut [Local]) {
        let clip = &scene.animations[track.clip];
        for c in &clip.channels {
            let v = sample(c, track.time);
            let l = &mut out[c.node];
            match c.path {
                Path::Translation => l.t = V3 { x: v[0], y: v[1], z: v[2] },
                Path::Rotation => l.r = v,
                Path::Scale => l.s = V3 { x: v[0], y: v[1], z: v[2] },
            }
        }
    }

    pub fn advance(&mut self, scene: &Scene, dt: f32) {
        let wrap = |track: &mut Track| {
            let d = scene.animations[track.clip].duration;
            track.time = if d > 0.0 { (track.time + dt).rem_euclid(d) } else { 0.0 };
        };
        if let Some(t) = &mut self.current {
            wrap(t);
        }
        if let Some((t, left)) = &mut self.previous {
            wrap(t);
            *left -= dt;
        }
        if self.previous.is_some_and(|(_, left)| left <= 0.0) {
            self.previous = None;
        }
        self.local.copy_from_slice(&self.rest);
        if let Some(track) = self.current {
            Self::apply(scene, track, &mut self.local);
        }
        if let Some((track, left)) = self.previous {
            self.scratch.copy_from_slice(&self.rest);
            Self::apply(scene, track, &mut self.scratch);
            let k = 1.0 - (left / FADE).clamp(0.0, 1.0);
            // Smoothstep, so the blend starts and ends gently.
            let k = k * k * (3.0 - 2.0 * k);
            for (l, p) in self.local.iter_mut().zip(&self.scratch) {
                *l = p.mix(l, k);
            }
        }
        self.generation += 1;
        self.update_world(scene);
    }

    pub fn update_world(&mut self, scene: &Scene) {
        for &i in &scene.order {
            let l = &self.local[i];
            let m = M4::trs(l.t, l.r, l.s);
            self.world[i] = match scene.nodes[i].parent {
                Some(p) => self.world[p].mul(&m),
                None => m,
            };
        }
    }

    /// The joint matrices of a skin: joint world × inverse bind.
    pub fn palette(&self, scene: &Scene, skin: usize, out: &mut Vec<M4>) {
        let s = &scene.skins[skin];
        out.clear();
        out.extend(s.joints.iter().zip(&s.inverse_bind).map(|(&j, ib)| self.world[j].mul(ib)));
    }
}

/// The box around everything drawn in the current pose, skinned meshes
/// included, in the scene's own units.
pub fn bounds(scene: &Scene, player: &Player) -> Bounds {
    let mut b = Bounds::EMPTY;
    let mut palette = Vec::new();
    for &i in &scene.order {
        let node = &scene.nodes[i];
        let Some(mesh) = node.mesh else { continue };
        let skin = node.skin.filter(|&s| !scene.skins[s].joints.is_empty());
        if let Some(s) = skin {
            player.palette(scene, s, &mut palette);
        }
        for p in &scene.meshes[mesh] {
            match (skin, &p.joints, &p.weights) {
                (Some(_), Some(joints), Some(weights)) => {
                    for ((pos, j), w) in p.positions.iter().zip(joints).zip(weights) {
                        let v = V3::from(*pos);
                        let mut acc = V3::ZERO;
                        for k in 0..4 {
                            if w[k] > 0.0 {
                                let m = palette.get(j[k] as usize).copied().unwrap_or(M4::IDENTITY);
                                acc = acc + m.point(v) * w[k];
                            }
                        }
                        b.add(acc);
                    }
                }
                _ => {
                    for pos in &p.positions {
                        b.add(player.world[i].point(V3::from(*pos)));
                    }
                }
            }
        }
    }
    if b.valid() {
        b
    } else {
        Bounds { min: V3::ZERO, max: V3::ONE }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Channel;

    fn channel(path: Path, interp: Interp, times: Vec<f32>, values: Vec<f32>) -> Channel {
        Channel { node: 0, path, interp, times, values }
    }

    #[test]
    fn linear_step_and_clamping() {
        let c = channel(Path::Translation, Interp::Linear, vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 2.0, 4.0, 6.0]);
        assert_eq!(sample(&c, 0.5)[..3], [1.0, 2.0, 3.0]);
        assert_eq!(sample(&c, -1.0)[..3], [0.0, 0.0, 0.0]);
        assert_eq!(sample(&c, 9.0)[..3], [2.0, 4.0, 6.0]);
        let s = Channel { interp: Interp::Step, ..c };
        assert_eq!(sample(&s, 0.99)[..3], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn cubic_spline_passes_through_its_keys() {
        // Keys at 0 and 2 with values 0 and 10 and zero tangents.
        let v = vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 10.0, 10.0, 0.0, 0.0, 0.0];
        let c = channel(Path::Scale, Interp::Cubic, vec![0.0, 2.0], v);
        assert_eq!(sample(&c, 0.0)[0], 0.0);
        assert!((sample(&c, 1.0)[0] - 5.0).abs() < 1e-5);
        assert_eq!(sample(&c, 2.0)[0], 10.0);
    }

    #[test]
    fn rotation_keys_interpolate_on_the_sphere() {
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let c = channel(Path::Rotation, Interp::Linear, vec![0.0, 1.0], vec![0.0, 0.0, 0.0, 1.0, 0.0, half, 0.0, half]);
        let q = sample(&c, 0.5);
        let angle = 2.0 * q[3].acos();
        assert!((angle - std::f32::consts::FRAC_PI_4).abs() < 1e-4);
    }

    /// One node whose only clip moves it to x = 1, played and then stopped.
    fn one_node_scene() -> Scene {
        use crate::math::QUAT_IDENTITY;
        use crate::scene::{Animation, Node};
        let node = Node {
            parent: None,
            translation: V3::ZERO,
            rotation: QUAT_IDENTITY,
            scale: V3::ONE,
            mesh: None,
            skin: None,
        };
        let clip = Animation {
            name: "Slide".into(),
            channels: vec![channel(
                Path::Translation,
                Interp::Step,
                vec![0.0, 1.0],
                vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            )],
            duration: 1.0,
        };
        Scene {
            nodes: vec![node],
            order: vec![0],
            meshes: Vec::new(),
            materials: Vec::new(),
            images: Vec::new(),
            skins: Vec::new(),
            animations: vec![clip],
        }
    }

    #[test]
    fn stopping_a_clip_fades_back_to_rest() {
        let scene = one_node_scene();
        let mut player = Player::new(&scene);
        player.play(Some(0));
        player.advance(&scene, 0.1);
        assert_eq!(player.local[0].t.x, 1.0);
        player.play(None);
        assert!(player.fading());
        player.advance(&scene, FADE / 2.0);
        let halfway = player.local[0].t.x;
        assert!(halfway > 0.0 && halfway < 1.0, "halfway through the fade, got {halfway}");
        player.advance(&scene, FADE);
        assert!(!player.fading());
        assert_eq!(player.local[0].t.x, 0.0);
        assert_eq!(player.world[0].point(V3::ZERO), V3::ZERO);
    }
}
