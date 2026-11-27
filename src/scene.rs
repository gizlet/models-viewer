//! A glTF 2.0 binary read into plain data the renderer uploads: meshes split
//! into pieces of at most 65,535 vertices (raylib draws 16-bit indices),
//! materials, decoded images, the node tree, skins and animations.
//!
//! This runs on a worker thread, so it touches no GL state.

use crate::math::{quat_normalize, v3, Quat, M4, V3};
use gltf::animation::util::ReadOutputs;
use gltf::texture::{MagFilter, MinFilter, WrappingMode};
use std::collections::HashMap;

pub const MAX_VERTICES: usize = 65535;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Alpha {
    Opaque,
    Mask(f32),
    Blend,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sampler {
    pub repeat_s: bool,
    pub repeat_t: bool,
    pub mirror_s: bool,
    pub mirror_t: bool,
    pub nearest: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexRef {
    pub image: usize,
    pub sampler: Sampler,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub base_color: [f32; 4],
    pub base_tex: Option<TexRef>,
    pub metallic: f32,
    pub roughness: f32,
    pub metal_rough_tex: Option<TexRef>,
    pub normal_tex: Option<TexRef>,
    pub normal_scale: f32,
    pub occlusion_tex: Option<TexRef>,
    pub occlusion_strength: f32,
    pub emissive: [f32; 3],
    pub emissive_tex: Option<TexRef>,
    pub alpha: Alpha,
    pub double_sided: bool,
    pub unlit: bool,
    /// KHR_materials_transmission, drawn as see-through glass.
    pub transmission: f32,
}

impl Default for Material {
    fn default() -> Material {
        Material {
            base_color: [1.0; 4],
            base_tex: None,
            metallic: 0.0,
            roughness: 0.6,
            metal_rough_tex: None,
            normal_tex: None,
            normal_scale: 1.0,
            occlusion_tex: None,
            occlusion_strength: 1.0,
            emissive: [0.0; 3],
            emissive_tex: None,
            alpha: Alpha::Opaque,
            double_sided: false,
            unlit: false,
            transmission: 0.0,
        }
    }
}

impl Material {
    pub fn blended(&self) -> bool {
        self.alpha == Alpha::Blend || self.transmission > 0.0
    }
}

/// An 8-bit image, three or four channels.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Primitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Option<Vec<[u8; 4]>>,
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
    pub indices: Vec<u16>,
    pub material: usize,
}

impl Primitive {
    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn skinned(&self) -> bool {
        self.joints.is_some() && self.weights.is_some()
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub parent: Option<usize>,
    pub translation: V3,
    pub rotation: Quat,
    pub scale: V3,
    pub mesh: Option<usize>,
    pub skin: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Skin {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<M4>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Path {
    Translation,
    Rotation,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interp {
    Linear,
    Step,
    Cubic,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub node: usize,
    pub path: Path,
    pub interp: Interp,
    pub times: Vec<f32>,
    /// Three or four floats per key; cubic splines store in-tangent, value
    /// and out-tangent for each key.
    pub values: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    pub channels: Vec<Channel>,
    pub duration: f32,
}

pub struct Scene {
    pub nodes: Vec<Node>,
    /// Parents before children.
    pub order: Vec<usize>,
    pub meshes: Vec<Vec<Primitive>>,
    pub materials: Vec<Material>,
    pub images: Vec<Image>,
    pub skins: Vec<Skin>,
    pub animations: Vec<Animation>,
}

#[derive(Debug)]
pub struct LoadError(pub String);

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn sampler(texture: &gltf::Texture) -> Sampler {
    let s = texture.sampler();
    let nearest = s.mag_filter() == Some(MagFilter::Nearest)
        && matches!(s.min_filter(), Some(MinFilter::Nearest | MinFilter::NearestMipmapNearest));
    Sampler {
        repeat_s: s.wrap_s() != WrappingMode::ClampToEdge,
        repeat_t: s.wrap_t() != WrappingMode::ClampToEdge,
        mirror_s: s.wrap_s() == WrappingMode::MirroredRepeat,
        mirror_t: s.wrap_t() == WrappingMode::MirroredRepeat,
        nearest,
    }
}

fn tex_ref(texture: gltf::Texture) -> TexRef {
    TexRef { image: texture.source().index(), sampler: sampler(&texture) }
}

/// KHR_texture_transform as an affine map of texture coordinates:
/// `[a, b, c, d, e, f]` gives `u' = a u + c v + e`, `v' = b u + d v + f`.
type UvTransform = [f32; 6];

fn uv_transform(info: &gltf::texture::Info) -> Option<UvTransform> {
    let t = info.texture_transform()?;
    let [ox, oy] = t.offset();
    let [sx, sy] = t.scale();
    let (s, c) = t.rotation().sin_cos();
    // translation * rotation * scale, as the extension defines.
    Some([c * sx, -s * sx, s * sy, c * sy, ox, oy])
}

fn material(m: gltf::Material) -> (Material, Option<UvTransform>) {
    let pbr = m.pbr_metallic_roughness();
    let base = pbr.base_color_texture();
    let transform = base.as_ref().and_then(uv_transform);
    let strength = m.emissive_strength().unwrap_or(1.0);
    let alpha = match m.alpha_mode() {
        gltf::material::AlphaMode::Opaque => Alpha::Opaque,
        gltf::material::AlphaMode::Mask => Alpha::Mask(m.alpha_cutoff().unwrap_or(0.5)),
        gltf::material::AlphaMode::Blend => Alpha::Blend,
    };
    let normal = m.normal_texture();
    let occlusion = m.occlusion_texture();
    let material = Material {
        base_color: pbr.base_color_factor(),
        base_tex: base.map(|i| tex_ref(i.texture())),
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        metal_rough_tex: pbr.metallic_roughness_texture().map(|i| tex_ref(i.texture())),
        normal_scale: normal.as_ref().map_or(1.0, |n| n.scale()),
        normal_tex: normal.map(|n| tex_ref(n.texture())),
        occlusion_strength: occlusion.as_ref().map_or(1.0, |o| o.strength()),
        occlusion_tex: occlusion.map(|o| tex_ref(o.texture())),
        emissive: m.emissive_factor().map(|v| v * strength),
        emissive_tex: m.emissive_texture().map(|i| tex_ref(i.texture())),
        alpha,
        double_sided: m.double_sided(),
        unlit: m.unlit(),
        transmission: m.transmission().map_or(0.0, |t| t.transmission_factor()),
    };
    (material, transform)
}

fn image(data: gltf::image::Data) -> Image {
    use gltf::image::Format;
    let (w, h) = (data.width, data.height);
    let n = (w * h) as usize;
    let px = &data.pixels;
    let wide = |i: usize| px.get(i * 2 + 1).copied().unwrap_or(0);
    let (channels, pixels): (u8, Vec<u8>) = match data.format {
        Format::R8G8B8 => (3, data.pixels),
        Format::R8G8B8A8 => (4, data.pixels),
        Format::R8 => (3, px.iter().flat_map(|&v| [v, v, v]).collect()),
        Format::R8G8 => (4, px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect()),
        Format::R16 => (3, (0..n).flat_map(|i| [wide(i); 3]).collect()),
        Format::R16G16 => (4, (0..n).flat_map(|i| [wide(2 * i), wide(2 * i), wide(2 * i), wide(2 * i + 1)]).collect()),
        Format::R16G16B16 => (3, (0..n * 3).map(wide).collect()),
        Format::R16G16B16A16 => (4, (0..n * 4).map(wide).collect()),
        Format::R32G32B32FLOAT | Format::R32G32B32A32FLOAT => {
            let c = if data.format == Format::R32G32B32FLOAT { 3 } else { 4 };
            let f = |i: usize| f32::from_le_bytes([px[i * 4], px[i * 4 + 1], px[i * 4 + 2], px[i * 4 + 3]]);
            (c, (0..n * c as usize).map(|i| (f(i).clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8).collect())
        }
    };
    Image { width: w, height: h, channels, pixels }
}

/// Smooth normals weighted by triangle area, for meshes that come without.
pub fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut acc = vec![V3::ZERO; positions.len()];
    for t in indices.as_chunks::<3>().0 {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| V3::from(positions[i as usize]));
        let n = (b - a).cross(c - a);
        for &i in t {
            acc[i as usize] = acc[i as usize] + n;
        }
    }
    acc.into_iter().map(|n| if n.len() > 0.0 { n.norm().arr() } else { [0.0, 1.0, 0.0] }).collect()
}

/// Splits a triangle list into pieces whose vertices fit 16-bit indices.
pub fn split(source: &Primitive32) -> Vec<Primitive> {
    let mut out = Vec::new();
    let mut piece = Primitive { material: source.material, ..Default::default() };
    let mut map: HashMap<u32, u16> = HashMap::new();
    let push_vertex = |piece: &mut Primitive, i: usize| {
        piece.positions.push(source.positions[i]);
        piece.normals.push(source.normals[i]);
        piece.uvs.push(source.uvs[i]);
        if let Some(c) = &source.colors {
            piece.colors.get_or_insert_with(Vec::new).push(c[i]);
        }
        if let (Some(j), Some(w)) = (&source.joints, &source.weights) {
            piece.joints.get_or_insert_with(Vec::new).push(j[i]);
            piece.weights.get_or_insert_with(Vec::new).push(w[i]);
        }
    };
    for tri in source.indices.as_chunks::<3>().0 {
        let fresh = tri.iter().filter(|i| !map.contains_key(i)).count();
        if piece.positions.len() + fresh > MAX_VERTICES {
            out.push(std::mem::replace(&mut piece, Primitive { material: source.material, ..Default::default() }));
            map.clear();
        }
        for &i in tri {
            let index = *map.entry(i).or_insert_with(|| {
                push_vertex(&mut piece, i as usize);
                (piece.positions.len() - 1) as u16
            });
            piece.indices.push(index);
        }
    }
    if !piece.indices.is_empty() {
        out.push(piece);
    }
    out
}

/// A primitive as read, before splitting, with 32-bit indices.
#[derive(Default)]
pub struct Primitive32 {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Option<Vec<[u8; 4]>>,
    pub joints: Option<Vec<[u16; 4]>>,
    pub weights: Option<Vec<[f32; 4]>>,
    pub indices: Vec<u32>,
    pub material: usize,
}

impl Scene {
    pub fn from_glb(bytes: &[u8]) -> Result<Scene, LoadError> {
        let (doc, buffers, images) = gltf::import_slice(bytes).map_err(|e| LoadError(e.to_string()))?;
        let default_material = doc.materials().len();
        let mut materials = Vec::new();
        let mut transforms = Vec::new();
        for m in doc.materials() {
            let (material, transform) = material(m);
            materials.push(material);
            transforms.push(transform);
        }
        materials.push(Material::default());
        transforms.push(None);

        let mut meshes = Vec::new();
        for mesh in doc.meshes() {
            let mut pieces = Vec::new();
            for prim in mesh.primitives() {
                if prim.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = prim.reader(|b| Some(&buffers[b.index()]));
                let Some(positions) = reader.read_positions() else { continue };
                let positions: Vec<[f32; 3]> = positions.collect();
                let n = positions.len();
                let indices: Vec<u32> = match reader.read_indices() {
                    Some(i) => i.into_u32().filter(|&i| (i as usize) < n).collect(),
                    None => (0..n as u32).collect(),
                };
                let indices: Vec<u32> = indices[..indices.len() / 3 * 3].to_vec();
                let normals = match reader.read_normals() {
                    Some(r) => r.collect(),
                    None => smooth_normals(&positions, &indices),
                };
                let material = prim.material().index().unwrap_or(default_material);
                let mut uvs: Vec<[f32; 2]> =
                    reader.read_tex_coords(0).map(|t| t.into_f32().collect()).unwrap_or_else(|| vec![[0.0; 2]; n]);
                if let Some([a, b, c, d, e, f]) = transforms[material] {
                    for uv in &mut uvs {
                        let [u, v] = *uv;
                        *uv = [a * u + c * v + e, b * u + d * v + f];
                    }
                }
                let colors = reader.read_colors(0).map(|c| c.into_rgba_u8().collect::<Vec<_>>());
                let joints = reader.read_joints(0).map(|j| j.into_u16().collect::<Vec<_>>());
                let weights = reader.read_weights(0).map(|w| {
                    w.into_f32()
                        .map(|w| {
                            let sum: f32 = w.iter().sum();
                            if sum > 0.0 {
                                w.map(|v| v / sum)
                            } else {
                                [1.0, 0.0, 0.0, 0.0]
                            }
                        })
                        .collect::<Vec<_>>()
                });
                let lengths_match = normals.len() == n
                    && uvs.len() == n
                    && colors.as_ref().is_none_or(|c| c.len() == n)
                    && joints.as_ref().is_none_or(|j| j.len() == n)
                    && weights.as_ref().is_none_or(|w| w.len() == n);
                if !lengths_match {
                    return Err(LoadError(format!("mesh {} has attributes of different lengths", mesh.index())));
                }
                let (joints, weights) = match (joints, weights) {
                    (Some(j), Some(w)) => (Some(j), Some(w)),
                    _ => (None, None),
                };
                let source = Primitive32 { positions, normals, uvs, colors, joints, weights, indices, material };
                pieces.extend(split(&source));
            }
            meshes.push(pieces);
        }

        let mut nodes: Vec<Node> = doc
            .nodes()
            .map(|n| {
                let (t, r, s) = n.transform().decomposed();
                Node {
                    parent: None,
                    translation: V3::from(t),
                    rotation: quat_normalize(r),
                    scale: V3::from(s),
                    mesh: n.mesh().map(|m| m.index()),
                    skin: n.skin().map(|s| s.index()),
                }
            })
            .collect();
        for n in doc.nodes() {
            for c in n.children() {
                nodes[c.index()].parent = Some(n.index());
            }
        }
        let roots: Vec<usize> = match doc.default_scene().or_else(|| doc.scenes().next()) {
            Some(scene) => scene.nodes().map(|n| n.index()).collect(),
            None => (0..nodes.len()).filter(|&i| nodes[i].parent.is_none()).collect(),
        };
        let mut order = Vec::new();
        let mut stack: Vec<usize> = roots.into_iter().rev().collect();
        let children: Vec<Vec<usize>> = doc.nodes().map(|n| n.children().map(|c| c.index()).collect()).collect();
        let mut seen = vec![false; nodes.len()];
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut seen[i], true) {
                continue;
            }
            order.push(i);
            stack.extend(children[i].iter().rev());
        }

        let skins = doc
            .skins()
            .map(|s| {
                let joints: Vec<usize> = s.joints().map(|j| j.index()).collect();
                let reader = s.reader(|b| Some(&buffers[b.index()]));
                let mut inverse_bind: Vec<M4> = reader
                    .read_inverse_bind_matrices()
                    .map(|m| m.map(|c| M4([c[0], c[1], c[2], c[3]].concat().try_into().unwrap())).collect())
                    .unwrap_or_default();
                inverse_bind.resize(joints.len(), M4::IDENTITY);
                Skin { joints, inverse_bind }
            })
            .collect();

        let animations = doc
            .animations()
            .enumerate()
            .map(|(i, a)| {
                let mut duration: f32 = 0.0;
                let channels: Vec<Channel> = a
                    .channels()
                    .filter_map(|c| {
                        let reader = c.reader(|b| Some(&buffers[b.index()]));
                        let times: Vec<f32> = reader.read_inputs()?.collect();
                        let (path, values): (Path, Vec<f32>) = match reader.read_outputs()? {
                            ReadOutputs::Translations(t) => (Path::Translation, t.flatten().collect()),
                            ReadOutputs::Rotations(r) => (Path::Rotation, r.into_f32().flatten().collect()),
                            ReadOutputs::Scales(s) => (Path::Scale, s.flatten().collect()),
                            ReadOutputs::MorphTargetWeights(_) => return None,
                        };
                        let interp = match c.sampler().interpolation() {
                            gltf::animation::Interpolation::Linear => Interp::Linear,
                            gltf::animation::Interpolation::Step => Interp::Step,
                            gltf::animation::Interpolation::CubicSpline => Interp::Cubic,
                        };
                        let width = if path == Path::Rotation { 4 } else { 3 };
                        let per_key = if interp == Interp::Cubic { width * 3 } else { width };
                        if times.is_empty() || values.len() < times.len() * per_key {
                            return None;
                        }
                        duration = duration.max(*times.last().unwrap());
                        Some(Channel { node: c.target().node().index(), path, interp, times, values })
                    })
                    .collect();
                Animation {
                    name: a.name().map_or_else(|| format!("Animation {}", i + 1), str::to_string),
                    channels,
                    duration,
                }
            })
            .filter(|a| !a.channels.is_empty())
            .collect();

        Ok(Scene {
            nodes,
            order,
            meshes,
            materials,
            images: images.into_iter().map(image).collect(),
            skins,
            animations,
        })
    }

    #[cfg(test)]
    pub fn triangles(&self) -> usize {
        self.nodes
            .iter()
            .filter_map(|n| n.mesh)
            .map(|m| self.meshes[m].iter().map(Primitive::triangles).sum::<usize>())
            .sum()
    }
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: V3,
    pub max: V3,
}

impl Bounds {
    pub const EMPTY: Bounds = Bounds { min: v3(f32::MAX, f32::MAX, f32::MAX), max: v3(f32::MIN, f32::MIN, f32::MIN) };

    pub fn add(&mut self, p: V3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    pub fn valid(&self) -> bool {
        self.min.x <= self.max.x
    }

    pub fn center(&self) -> V3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> V3 {
        self.max - self.min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_keeps_every_triangle_within_sixteen_bits() {
        // A strip of 50,000 quads: 100,002 vertices, 100,000 triangles.
        let quads = 50_000u32;
        let positions: Vec<[f32; 3]> = (0..=quads).flat_map(|i| [[i as f32, 0.0, 0.0], [i as f32, 1.0, 0.0]]).collect();
        let n = positions.len();
        let indices: Vec<u32> =
            (0..quads).flat_map(|i| [2 * i, 2 * i + 2, 2 * i + 1, 2 * i + 1, 2 * i + 2, 2 * i + 3]).collect();
        let source = Primitive32 {
            normals: vec![[0.0, 0.0, 1.0]; n],
            uvs: vec![[0.0; 2]; n],
            positions: positions.clone(),
            indices: indices.clone(),
            ..Default::default()
        };
        let pieces = split(&source);
        assert!(pieces.len() >= 2);
        assert_eq!(pieces.iter().map(Primitive::triangles).sum::<usize>(), quads as usize * 2);
        let mut rebuilt = Vec::new();
        for p in &pieces {
            assert!(p.positions.len() <= MAX_VERTICES);
            rebuilt.extend(p.indices.iter().map(|&i| p.positions[i as usize]));
        }
        let original: Vec<[f32; 3]> = indices.iter().map(|&i| positions[i as usize]).collect();
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn smooth_normals_face_the_winding() {
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let n = smooth_normals(&positions, &[0, 1, 2]);
        assert_eq!(n, vec![[0.0, 0.0, 1.0]; 3]);
    }

    /// Every bundled model reads, draws something and matches its catalog figures.
    #[test]
    fn every_bundled_model_loads() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        let catalog = crate::catalog::Catalog::load(&dir).expect("catalog");
        for m in &catalog.models {
            let bytes = std::fs::read(dir.join(format!("models/{}.glb", m.id))).expect("model file");
            let scene = Scene::from_glb(&bytes).unwrap_or_else(|e| panic!("{}: {e}", m.id));
            assert_eq!(scene.triangles() as u32, m.stats.triangles, "{}", m.id);
            assert_eq!(scene.animations.len(), m.stats.animations.len(), "{}", m.id);
            for material in &scene.materials {
                for t in [
                    material.base_tex,
                    material.normal_tex,
                    material.metal_rough_tex,
                    material.occlusion_tex,
                    material.emissive_tex,
                ]
                .into_iter()
                .flatten()
                {
                    let img = &scene.images[t.image];
                    assert!(img.width <= 512 && img.height <= 512, "{}: {}×{}", m.id, img.width, img.height);
                    assert_eq!(img.pixels.len(), (img.width * img.height * img.channels as u32) as usize, "{}", m.id);
                }
            }
            for skin in &scene.skins {
                assert!(skin.joints.len() <= crate::gpu::MAX_BONES, "{}", m.id);
            }
            let player = crate::anim::Player::new(&scene);
            let b = crate::anim::bounds(&scene, &player);
            let size = b.size();
            assert!(size.x > 0.0 && size.y > 0.0 && size.z >= 0.0, "{}: empty bounds", m.id);
        }
    }
}
