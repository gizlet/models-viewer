//! Small vector, quaternion and matrix types for the scene graph and camera.
//! Matrices are column-major, as OpenGL and glTF store them: element
//! `m[col * 4 + row]`.

use std::ops::{Add, Mul, Neg, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn v3(x: f32, y: f32, z: f32) -> V3 {
    V3 { x, y, z }
}

impl V3 {
    pub const ZERO: V3 = v3(0.0, 0.0, 0.0);
    pub const ONE: V3 = v3(1.0, 1.0, 1.0);

    pub fn from(a: [f32; 3]) -> V3 {
        v3(a[0], a[1], a[2])
    }

    pub fn arr(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn dot(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: V3) -> V3 {
        v3(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x)
    }

    pub fn len(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn norm(self) -> V3 {
        let l = self.len();
        if l > 1e-12 {
            self * (1.0 / l)
        } else {
            self
        }
    }

    pub fn min(self, o: V3) -> V3 {
        v3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    pub fn max(self, o: V3) -> V3 {
        v3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    pub fn lerp(self, o: V3, t: f32) -> V3 {
        self + (o - self) * t
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        v3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        v3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        v3(self.x * k, self.y * k, self.z * k)
    }
}

impl Neg for V3 {
    type Output = V3;
    fn neg(self) -> V3 {
        v3(-self.x, -self.y, -self.z)
    }
}

/// A rotation as `[x, y, z, w]`, glTF's order.
pub type Quat = [f32; 4];

pub const QUAT_IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

pub fn quat_normalize(q: Quat) -> Quat {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l > 1e-12 {
        [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
    } else {
        QUAT_IDENTITY
    }
}

/// Spherical interpolation along the shorter arc.
pub fn quat_slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let mut d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if d < 0.0 {
        d = -d;
        b = [-b[0], -b[1], -b[2], -b[3]];
    }
    if d > 0.9995 {
        return quat_normalize(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t));
    }
    let theta = d.clamp(-1.0, 1.0).acos();
    let s = theta.sin();
    let (wa, wb) = (((1.0 - t) * theta).sin() / s, (t * theta).sin() / s);
    std::array::from_fn(|i| a[i] * wa + b[i] * wb)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M4(pub [f32; 16]);

impl Default for M4 {
    fn default() -> M4 {
        M4::IDENTITY
    }
}

impl M4 {
    pub const IDENTITY: M4 = M4([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);

    pub fn at(&self, row: usize, col: usize) -> f32 {
        self.0[col * 4 + row]
    }

    /// `self * o`: `o` is applied first.
    pub fn mul(&self, o: &M4) -> M4 {
        let mut r = [0.0; 16];
        for col in 0..4 {
            for row in 0..4 {
                r[col * 4 + row] = (0..4).map(|k| self.at(row, k) * o.at(k, col)).sum();
            }
        }
        M4(r)
    }

    pub fn translate(t: V3) -> M4 {
        let mut m = M4::IDENTITY;
        m.0[12] = t.x;
        m.0[13] = t.y;
        m.0[14] = t.z;
        m
    }

    pub fn scale(s: V3) -> M4 {
        let mut m = M4::IDENTITY;
        m.0[0] = s.x;
        m.0[5] = s.y;
        m.0[10] = s.z;
        m
    }

    pub fn rotation(q: Quat) -> M4 {
        let [x, y, z, w] = q;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        M4([
            1.0 - 2.0 * (yy + zz),
            2.0 * (xy + wz),
            2.0 * (xz - wy),
            0.0,
            2.0 * (xy - wz),
            1.0 - 2.0 * (xx + zz),
            2.0 * (yz + wx),
            0.0,
            2.0 * (xz + wy),
            2.0 * (yz - wx),
            1.0 - 2.0 * (xx + yy),
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ])
    }

    /// Translation, rotation and scale, applied scale first as glTF does.
    pub fn trs(t: V3, r: Quat, s: V3) -> M4 {
        let mut m = M4::rotation(r);
        for (col, k) in [s.x, s.y, s.z].into_iter().enumerate() {
            for row in 0..3 {
                m.0[col * 4 + row] *= k;
            }
        }
        m.0[12] = t.x;
        m.0[13] = t.y;
        m.0[14] = t.z;
        m
    }

    pub fn rotate_y(a: f32) -> M4 {
        let (s, c) = a.sin_cos();
        M4([c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0])
    }

    pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> M4 {
        let f = 1.0 / (fovy / 2.0).tan();
        let mut m = [0.0; 16];
        m[0] = f / aspect;
        m[5] = f;
        m[10] = (far + near) / (near - far);
        m[11] = -1.0;
        m[14] = 2.0 * far * near / (near - far);
        M4(m)
    }

    pub fn ortho(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> M4 {
        let mut m = M4::IDENTITY.0;
        m[0] = 2.0 / (r - l);
        m[5] = 2.0 / (t - b);
        m[10] = -2.0 / (f - n);
        m[12] = -(r + l) / (r - l);
        m[13] = -(t + b) / (t - b);
        m[14] = -(f + n) / (f - n);
        M4(m)
    }

    pub fn look_at(eye: V3, target: V3, up: V3) -> M4 {
        let f = (target - eye).norm();
        let s = f.cross(up).norm();
        let u = s.cross(f);
        M4([s.x, u.x, -f.x, 0.0, s.y, u.y, -f.y, 0.0, s.z, u.z, -f.z, 0.0, -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0])
    }

    pub fn point(&self, p: V3) -> V3 {
        let m = &self.0;
        v3(
            m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
            m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
            m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
        )
    }

    /// The determinant of the upper 3×3: negative when the matrix mirrors.
    pub fn det3(&self) -> f32 {
        let c0 = v3(self.0[0], self.0[1], self.0[2]);
        let c1 = v3(self.0[4], self.0[5], self.0[6]);
        let c2 = v3(self.0[8], self.0[9], self.0[10]);
        c0.dot(c1.cross(c2))
    }

    /// The inverse transpose of the upper 3×3, for normals, as a 4×4.
    pub fn normal_matrix(&self) -> M4 {
        let c0 = v3(self.0[0], self.0[1], self.0[2]);
        let c1 = v3(self.0[4], self.0[5], self.0[6]);
        let c2 = v3(self.0[8], self.0[9], self.0[10]);
        let det = c0.dot(c1.cross(c2));
        if det.abs() < 1e-20 {
            return M4::IDENTITY;
        }
        // The inverse transpose's columns are the cofactor columns over the determinant.
        let k = 1.0 / det;
        let (a, b, c) = (c1.cross(c2) * k, c2.cross(c0) * k, c0.cross(c1) * k);
        M4([a.x, a.y, a.z, 0.0, b.x, b.y, b.z, 0.0, c.x, c.y, c.z, 0.0, 0.0, 0.0, 0.0, 1.0])
    }

    /// The rows of the upper 3×4, for packing bone matrices into vec4 uniforms.
    pub fn rows3(&self) -> [[f32; 4]; 3] {
        std::array::from_fn(|row| std::array::from_fn(|col| self.at(row, col)))
    }

    pub fn to_raylib(self) -> raylib_sys::Matrix {
        let m = &self.0;
        raylib_sys::Matrix {
            m0: m[0],
            m1: m[1],
            m2: m[2],
            m3: m[3],
            m4: m[4],
            m5: m[5],
            m6: m[6],
            m7: m[7],
            m8: m[8],
            m9: m[9],
            m10: m[10],
            m11: m[11],
            m12: m[12],
            m13: m[13],
            m14: m[14],
            m15: m[15],
        }
    }
}

/// Frame-rate independent easing toward a target: `rate` per second.
pub fn approach(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    target + (current - target) * (-rate * dt).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (a - b).len() < 1e-5
    }

    #[test]
    fn trs_applies_scale_then_rotation_then_translation() {
        let quarter = [0.0, (std::f32::consts::FRAC_PI_4).sin(), 0.0, (std::f32::consts::FRAC_PI_4).cos()];
        let m = M4::trs(v3(1.0, 2.0, 3.0), quarter, v3(2.0, 2.0, 2.0));
        // +X scaled to 2, turned a quarter about +Y to -Z, then moved.
        assert!(close(m.point(v3(1.0, 0.0, 0.0)), v3(1.0, 2.0, 1.0)));
        let composed = M4::translate(v3(1.0, 2.0, 3.0)).mul(&M4::rotation(quarter)).mul(&M4::scale(v3(2.0, 2.0, 2.0)));
        for (a, b) in m.0.iter().zip(composed.0.iter()) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn look_at_puts_the_target_in_front() {
        let view = M4::look_at(v3(0.0, 0.0, 5.0), V3::ZERO, v3(0.0, 1.0, 0.0));
        assert!(close(view.point(V3::ZERO), v3(0.0, 0.0, -5.0)));
        let proj = M4::perspective(1.0, 1.0, 0.1, 100.0);
        let p = proj.mul(&view);
        let clip = |q: V3| {
            let m = &p.0;
            let w = m[3] * q.x + m[7] * q.y + m[11] * q.z + m[15];
            p.point(q).z / w
        };
        assert!(clip(V3::ZERO) > -1.0 && clip(V3::ZERO) < 1.0);
    }

    #[test]
    fn normal_matrix_keeps_normals_perpendicular_under_uneven_scale() {
        let m = M4::scale(v3(4.0, 1.0, 1.0));
        let n = m.normal_matrix();
        // A surface along the diagonal x = y stretched by 4 in x.
        let tangent = m.point(v3(1.0, 1.0, 0.0));
        let normal = n.point(v3(1.0, -1.0, 0.0));
        assert!(tangent.dot(normal).abs() < 1e-5);
        assert!(m.det3() > 0.0 && M4::scale(v3(-1.0, 1.0, 1.0)).det3() < 0.0);
    }

    #[test]
    fn slerp_takes_the_short_way_and_stays_unit() {
        let a = QUAT_IDENTITY;
        let b = [0.0, 0.0, 0.0, -1.0];
        let q = quat_slerp(a, b, 0.5);
        let l: f32 = q.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((l - 1.0).abs() < 1e-5);
        assert!((q[3].abs() - 1.0).abs() < 1e-5);
    }
}
