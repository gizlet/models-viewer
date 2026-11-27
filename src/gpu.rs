//! Draws a model on a small studio stage with rlgl.
//!
//! Surfaces get a compact metal/roughness shading: a warm key light, a cool
//! fill and a rim light that follow the camera, a sky-to-floor ambient and a
//! soft studio reflection, then filmic tone mapping. Normal maps use
//! per-pixel tangent frames, so meshes need no tangents. Skinned meshes are
//! skinned on the GPU from bone matrices passed as `vec4` rows.
//!
//! The floor is a disc that fades into the backdrop. The model's shadow is
//! its silhouette drawn from the key light into a small texture, blurred
//! where the floor samples it.
//!
//! Shaders are written in GLSL ES 1.00 style and given a prefix for the
//! context raylib made: OpenGL ES 3.0 or 2.0 on Gizlet OS, 3.3 on desktops.

use crate::anim::Player;
use crate::math::{v3, M4, V3};
use crate::scene::{Alpha, Image, Material, Primitive, Sampler, Scene, TexRef};
use raylib_sys as ffi;
use std::ffi::CString;

pub const MAX_BONES: usize = 100;
const SHADOW_SIZE: i32 = 256;

const LOC_POSITION: u32 = 0;
const LOC_TEXCOORD: u32 = 1;
const LOC_NORMAL: u32 = 2;
const LOC_COLOR: u32 = 3;
const LOC_JOINTS: u32 = 7;
const LOC_WEIGHTS: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Studio,
    Clay,
    Wireframe,
    Normals,
}

impl Style {
    pub const ALL: [Style; 4] = [Style::Studio, Style::Clay, Style::Wireframe, Style::Normals];

    fn code(self) -> f32 {
        self as i32 as f32
    }
}

fn glsl_es() -> bool {
    let v = unsafe { ffi::rlGetVersion() };
    v == ffi::rlGlVersion::RL_OPENGL_ES_20 as i32 || v == ffi::rlGlVersion::RL_OPENGL_ES_30 as i32
}

fn glsl_es3() -> bool {
    unsafe { ffi::rlGetVersion() == ffi::rlGlVersion::RL_OPENGL_ES_30 as i32 }
}

fn prefix(vertex: bool) -> &'static str {
    match (glsl_es(), glsl_es3(), vertex) {
        (true, true, true) => "#version 300 es\n#define attribute in\n#define varying out\n",
        (true, true, false) => {
            "#version 300 es\nprecision highp float;\n#define varying in\n#define texture2D texture\nout vec4 fragOut;\n#define FRAG fragOut\n"
        }
        (true, false, true) => "#version 100\n",
        (true, false, false) => {
            "#version 100\n#extension GL_OES_standard_derivatives : enable\n#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n#define FRAG gl_FragColor\n"
        }
        (false, _, true) => "#version 330\n#define attribute in\n#define varying out\n",
        (false, _, false) => "#version 330\n#define varying in\n#define texture2D texture\nout vec4 fragOut;\n#define FRAG fragOut\n",
    }
}

const VERTEX: &str = r#"
attribute vec3 vertexPosition;
attribute vec2 vertexTexCoord;
attribute vec3 vertexNormal;
attribute vec4 vertexColor;
#ifdef SKINNED
attribute vec4 vertexBoneIndices;
attribute vec4 vertexBoneWeights;
uniform vec4 bones[BONES * 3];
mat4 bone(float i) {
    int k = int(i + 0.5) * 3;
    vec4 a = bones[k];
    vec4 b = bones[k + 1];
    vec4 c = bones[k + 2];
    return mat4(a.x, b.x, c.x, 0.0, a.y, b.y, c.y, 0.0, a.z, b.z, c.z, 0.0, a.w, b.w, c.w, 1.0);
}
#endif
uniform mat4 mvp;
uniform mat4 matModel;
uniform mat4 matNormal;
varying vec3 vPos;
varying vec3 vNormal;
varying vec2 vUV;
varying vec4 vColor;
void main() {
    vec4 p = vec4(vertexPosition, 1.0);
    vec4 n = vec4(vertexNormal, 0.0);
#ifdef SKINNED
    mat4 s = bone(vertexBoneIndices.x) * vertexBoneWeights.x + bone(vertexBoneIndices.y) * vertexBoneWeights.y
           + bone(vertexBoneIndices.z) * vertexBoneWeights.z + bone(vertexBoneIndices.w) * vertexBoneWeights.w;
    p = s * p;
    n = s * n;
#endif
    vPos = (matModel * p).xyz;
    vNormal = (matNormal * n).xyz;
    vUV = vertexTexCoord;
    vColor = vertexColor;
    gl_Position = mvp * p;
}
"#;

const SURFACE: &str = r#"
varying vec3 vPos;
varying vec3 vNormal;
varying vec2 vUV;
varying vec4 vColor;
uniform sampler2D texBase;
uniform sampler2D texNormal;
uniform sampler2D texMR;
uniform sampler2D texOcclusion;
uniform sampler2D texEmissive;
uniform vec4 baseColor;
uniform vec3 emissiveColor;
uniform vec4 pbr;
uniform vec4 flags;
uniform vec3 camPos;
uniform vec3 keyDir;
uniform vec3 keyColor;
uniform vec3 fillDir;
uniform vec3 fillColor;
uniform vec3 rimDir;
uniform vec3 rimColor;
uniform vec3 skyColor;
uniform vec3 groundColor;
uniform vec3 wireColor;
uniform float exposure;
uniform float mode;

vec3 lin(vec3 c) { return c * c * (c * 0.305306 + 0.682171) + c * 0.012523; }

vec3 shade(vec3 N, vec3 V, vec3 L, vec3 light, vec3 diffuse, vec3 f0, float a) {
    float NoL = max(dot(N, L), 0.0);
    if (NoL <= 0.0) return vec3(0.0);
    vec3 H = normalize(L + V);
    float NoH = max(dot(N, H), 0.0);
    float VoH = max(dot(V, H), 0.0);
    float a2 = a * a;
    float d = NoH * NoH * (a2 - 1.0) + 1.0;
    float D = a2 / (3.14159 * d * d);
    vec3 F = f0 + (1.0 - f0) * pow(1.0 - VoH, 5.0);
    float k = (a + 1.0) * (a + 1.0) * 0.125;
    float G = 0.25 / ((NoL * (1.0 - k) + k) * (max(dot(N, V), 0.0) * (1.0 - k) + k));
    return (diffuse + D * G * F) * light * NoL;
}

vec3 studio(vec3 r, float rough) {
    float y = r.y;
    vec3 horizon = mix(skyColor, groundColor, 0.35);
    vec3 c = y > 0.0 ? mix(horizon, skyColor * 1.4, sqrt(y)) : mix(horizon, groundColor * 0.6, sqrt(-y));
    float spread = mix(48.0, 2.0, rough);
    c += keyColor * pow(max(dot(r, keyDir), 0.0), spread) * (1.6 - rough);
    c += rimColor * pow(max(dot(r, rimDir), 0.0), spread) * (1.0 - rough) * 0.8;
    return c;
}

void main() {
    vec4 base = baseColor;
    vec4 tex = texture2D(texBase, vUV);
    base *= vec4(lin(tex.rgb), tex.a);
    if (mode < 1.5) base *= vColor;
    if (base.a < flags.y) discard;
    vec3 N = normalize(vNormal);
    if (!gl_FrontFacing) N = -N;
    vec3 V = normalize(camPos - vPos);
    if (flags.x > 0.5 && mode < 0.5) {
        vec3 dp1 = dFdx(vPos);
        vec3 dp2 = dFdy(vPos);
        vec2 duv1 = dFdx(vUV);
        vec2 duv2 = dFdy(vUV);
        vec3 dp2perp = cross(dp2, N);
        vec3 dp1perp = cross(N, dp1);
        vec3 T = dp2perp * duv1.x + dp1perp * duv2.x;
        vec3 B = dp2perp * duv1.y + dp1perp * duv2.y;
        float m = max(dot(T, T), dot(B, B));
        if (m > 1e-20) {
            float inv = inversesqrt(m);
            vec3 t = texture2D(texNormal, vUV).xyz * 2.0 - 1.0;
            t.xy *= pbr.w;
            N = normalize(T * inv * t.x + B * inv * t.y + N * t.z);
        }
    }
    vec3 mr = texture2D(texMR, vUV).rgb;
    float rough = clamp(pbr.y * mr.g, 0.04, 1.0);
    float metal = clamp(pbr.x * mr.b, 0.0, 1.0);
    float ao = mix(1.0, texture2D(texOcclusion, vUV).r, pbr.z);
    vec3 emissive = lin(texture2D(texEmissive, vUV).rgb) * emissiveColor;
    if (mode > 0.5) {
        base = vec4(0.80, 0.78, 0.75, 1.0);
        rough = 0.62;
        metal = 0.0;
        emissive = vec3(0.0);
    }
    vec3 diffuse = base.rgb * (1.0 - metal) / 3.14159;
    vec3 f0 = mix(vec3(0.04), base.rgb, metal);
    float a = rough * rough;
    vec3 color = shade(N, V, keyDir, keyColor, diffuse, f0, a)
               + shade(N, V, fillDir, fillColor, diffuse, f0, a)
               + shade(N, V, rimDir, rimColor, diffuse, f0, a);
    float NoV = max(dot(N, V), 0.0);
    vec3 ambient = mix(groundColor, skyColor, N.y * 0.5 + 0.5);
    vec3 F = f0 + (max(vec3(1.0 - rough), f0) - f0) * pow(1.0 - NoV, 5.0);
    color += (base.rgb * (1.0 - metal) * ambient + studio(reflect(-V, N), rough) * F) * ao;
    color += emissive;
    if (flags.w > 0.5 && mode < 0.5) color = base.rgb;
    color *= exposure;
    color = clamp((color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14), 0.0, 1.0);
    color = pow(color, vec3(1.0 / 2.2));
    if (mode > 2.5) color = N * 0.5 + 0.5;
    if (mode > 1.5 && mode < 2.5) {
        vec3 w = fwidth(vColor.rgb);
        vec3 e = smoothstep(vec3(0.0), w * 0.9, vColor.rgb);
        float edge = 1.0 - min(min(e.x, e.y), e.z);
        color = mix(color, wireColor, edge * 0.7);
    }
    float alpha = base.a;
    if (flags.z > 0.0 && mode < 0.5) {
        alpha = max(base.a * (1.0 - flags.z * 0.8), pow(1.0 - NoV, 3.0) * 0.8);
    }
    FRAG = vec4(color, alpha);
}
"#;

const SILHOUETTE: &str = r#"
varying vec3 vPos;
varying vec3 vNormal;
varying vec2 vUV;
varying vec4 vColor;
uniform sampler2D texBase;
uniform vec4 flags;
void main() {
    if (flags.y > 0.0 && texture2D(texBase, vUV).a < flags.y) discard;
    FRAG = vec4(0.0, 0.0, 0.0, 1.0);
}
"#;

const FLOOR_VERTEX: &str = r#"
attribute vec3 vertexPosition;
uniform mat4 mvp;
uniform mat4 matModel;
varying vec3 vPos;
void main() {
    vPos = (matModel * vec4(vertexPosition, 1.0)).xyz;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
"#;

const FLOOR: &str = r#"
varying vec3 vPos;
uniform sampler2D texShadow;
uniform mat4 lightVP;
uniform vec4 floorColor;
uniform vec4 shadow;
void main() {
    float d = length(vPos.xz) / shadow.y;
    float fade = 1.0 - smoothstep(0.25, 1.0, d);
    vec4 lp = lightVP * vec4(vPos, 1.0);
    vec2 uv = lp.xy / lp.w * 0.5 + 0.5;
    float t = shadow.x;
    float s = texture2D(texShadow, uv).r * 0.28
            + (texture2D(texShadow, uv + vec2(t, t)).r + texture2D(texShadow, uv + vec2(-t, t)).r
             + texture2D(texShadow, uv + vec2(t, -t)).r + texture2D(texShadow, uv + vec2(-t, -t)).r) * 0.12
            + (texture2D(texShadow, uv + vec2(2.0 * t, 0.0)).r + texture2D(texShadow, uv - vec2(2.0 * t, 0.0)).r
             + texture2D(texShadow, uv + vec2(0.0, 2.0 * t)).r + texture2D(texShadow, uv - vec2(0.0, 2.0 * t)).r) * 0.06;
    float contact = 1.0 - shadow.w * exp(-d * d * 30.0);
    float light = mix(1.0, s, shadow.z) * contact;
    FRAG = vec4(floorColor.rgb * light, floorColor.a * fade);
}
"#;

struct Program {
    id: u32,
    locs: std::collections::HashMap<&'static str, i32>,
}

const UNIFORMS: [&str; 30] = [
    "mvp",
    "matModel",
    "matNormal",
    "bones",
    "texBase",
    "texNormal",
    "texMR",
    "texOcclusion",
    "texEmissive",
    "texShadow",
    "baseColor",
    "emissiveColor",
    "pbr",
    "flags",
    "camPos",
    "keyDir",
    "keyColor",
    "fillDir",
    "fillColor",
    "rimDir",
    "rimColor",
    "skyColor",
    "groundColor",
    "wireColor",
    "exposure",
    "mode",
    "lightVP",
    "floorColor",
    "shadow",
    "bones[0]",
];

impl Program {
    fn load(defines: &str, vertex: &str, fragment: &str) -> Program {
        let vs = CString::new(format!("{}{defines}{vertex}", prefix(true))).unwrap();
        let fs = CString::new(format!("{}{defines}{fragment}", prefix(false))).unwrap();
        let id = unsafe { ffi::rlLoadShaderProgram(vs.as_ptr(), fs.as_ptr()) };
        let mut locs = std::collections::HashMap::new();
        for name in UNIFORMS {
            let c = CString::new(name).unwrap();
            locs.insert(name, unsafe { ffi::rlGetLocationUniform(id, c.as_ptr()) });
        }
        // Some drivers name a uniform array by its first element.
        if locs["bones"] < 0 {
            locs.insert("bones", locs["bones[0]"]);
        }
        for (name, want) in [
            ("vertexPosition", LOC_POSITION),
            ("vertexTexCoord", LOC_TEXCOORD),
            ("vertexNormal", LOC_NORMAL),
            ("vertexColor", LOC_COLOR),
            ("vertexBoneIndices", LOC_JOINTS),
            ("vertexBoneWeights", LOC_WEIGHTS),
        ] {
            let c = CString::new(name).unwrap();
            let got = unsafe { ffi::rlGetLocationAttrib(id, c.as_ptr()) };
            if got >= 0 && got as u32 != want {
                eprintln!("modelsviewer: attribute {name} is at {got}, expected {want}");
            }
        }
        Program { id, locs }
    }

    fn loc(&self, name: &str) -> i32 {
        self.locs.get(name).copied().unwrap_or(-1)
    }

    fn vec4(&self, name: &str, v: [f32; 4]) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe {
                ffi::rlSetUniform(l, v.as_ptr().cast(), ffi::rlShaderUniformDataType::RL_SHADER_UNIFORM_VEC4 as i32, 1)
            };
        }
    }

    fn vec3(&self, name: &str, v: [f32; 3]) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe {
                ffi::rlSetUniform(l, v.as_ptr().cast(), ffi::rlShaderUniformDataType::RL_SHADER_UNIFORM_VEC3 as i32, 1)
            };
        }
    }

    fn float(&self, name: &str, v: f32) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe {
                ffi::rlSetUniform(
                    l,
                    (&v as *const f32).cast(),
                    ffi::rlShaderUniformDataType::RL_SHADER_UNIFORM_FLOAT as i32,
                    1,
                )
            };
        }
    }

    fn int(&self, name: &str, v: i32) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe {
                ffi::rlSetUniform(
                    l,
                    (&v as *const i32).cast(),
                    ffi::rlShaderUniformDataType::RL_SHADER_UNIFORM_INT as i32,
                    1,
                )
            };
        }
    }

    fn mat(&self, name: &str, m: &M4) {
        let l = self.loc(name);
        if l >= 0 {
            unsafe { ffi::rlSetUniformMatrix(l, m.to_raylib()) };
        }
    }

    fn bones(&self, rows: &[[f32; 4]]) {
        let l = self.loc("bones");
        if l >= 0 && !rows.is_empty() {
            unsafe {
                ffi::rlSetUniform(
                    l,
                    rows.as_ptr().cast(),
                    ffi::rlShaderUniformDataType::RL_SHADER_UNIFORM_VEC4 as i32,
                    rows.len() as i32,
                )
            };
        }
    }

    fn bind_samplers(&self) {
        for (slot, name) in
            ["texBase", "texNormal", "texMR", "texOcclusion", "texEmissive", "texShadow"].iter().enumerate()
        {
            self.int(name, slot as i32);
        }
    }
}

/// A vertex array and its buffers.
struct Geometry {
    vao: u32,
    vbos: Vec<u32>,
    ebo: u32,
    /// Indices to draw, or vertices when there is no index buffer.
    count: i32,
    attribs: Vec<(u32, u32, i32, i32, bool)>,
}

impl Geometry {
    fn new(
        attributes: &[(u32, &[f32], i32)],
        colors: Option<&[u8]>,
        indices: Option<&[u16]>,
        count: usize,
    ) -> Geometry {
        unsafe {
            let vao = ffi::rlLoadVertexArray();
            ffi::rlEnableVertexArray(vao);
            let mut vbos = Vec::new();
            let mut attribs = Vec::new();
            for &(loc, data, size) in attributes {
                let vbo = ffi::rlLoadVertexBuffer(data.as_ptr().cast(), std::mem::size_of_val(data) as i32, false);
                ffi::rlSetVertexAttribute(loc, size, ffi::RL_FLOAT as i32, false, 0, 0);
                ffi::rlEnableVertexAttribute(loc);
                vbos.push(vbo);
                attribs.push((vbo, loc, size, ffi::RL_FLOAT as i32, false));
            }
            if let Some(c) = colors {
                let vbo = ffi::rlLoadVertexBuffer(c.as_ptr().cast(), c.len() as i32, false);
                ffi::rlSetVertexAttribute(LOC_COLOR, 4, ffi::RL_UNSIGNED_BYTE as i32, true, 0, 0);
                ffi::rlEnableVertexAttribute(LOC_COLOR);
                vbos.push(vbo);
                attribs.push((vbo, LOC_COLOR, 4, ffi::RL_UNSIGNED_BYTE as i32, true));
            }
            let ebo = match indices {
                Some(i) => ffi::rlLoadVertexBufferElement(i.as_ptr().cast(), std::mem::size_of_val(i) as i32, false),
                None => 0,
            };
            ffi::rlDisableVertexArray();
            Geometry { vao, vbos, ebo, count: count as i32, attribs }
        }
    }

    fn draw(&self) {
        unsafe {
            let has_color = self.attribs.iter().any(|a| a.1 == LOC_COLOR);
            if !has_color {
                let white = [1.0f32; 4];
                ffi::rlSetVertexAttributeDefault(
                    LOC_COLOR as i32,
                    white.as_ptr().cast(),
                    ffi::ShaderAttributeDataType::SHADER_ATTRIB_VEC4 as i32,
                    4,
                );
            }
            if !ffi::rlEnableVertexArray(self.vao) {
                // Without vertex array objects, point the attributes every draw.
                for &(vbo, loc, size, kind, normalized) in &self.attribs {
                    ffi::rlEnableVertexBuffer(vbo);
                    ffi::rlSetVertexAttribute(loc, size, kind, normalized, 0, 0);
                    ffi::rlEnableVertexAttribute(loc);
                }
                if self.ebo != 0 {
                    ffi::rlEnableVertexBufferElement(self.ebo);
                }
            }
            if self.ebo != 0 {
                ffi::rlDrawVertexArrayElements(0, self.count, std::ptr::null());
            } else {
                ffi::rlDrawVertexArray(0, self.count);
            }
            ffi::rlDisableVertexArray();
            ffi::rlDisableVertexBuffer();
            ffi::rlDisableVertexBufferElement();
        }
    }
}

impl Drop for Geometry {
    fn drop(&mut self) {
        unsafe {
            for &vbo in &self.vbos {
                ffi::rlUnloadVertexBuffer(vbo);
            }
            if self.ebo != 0 {
                ffi::rlUnloadVertexBuffer(self.ebo);
            }
            ffi::rlUnloadVertexArray(self.vao);
        }
    }
}

fn flatten<const N: usize>(v: &[[f32; N]]) -> &[f32] {
    unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), v.len() * N) }
}

fn upload(p: &Primitive) -> Geometry {
    let joints: Option<Vec<[f32; 4]>> = p.joints.as_ref().map(|j| j.iter().map(|j| j.map(|v| v as f32)).collect());
    let mut attributes: Vec<(u32, &[f32], i32)> = vec![
        (LOC_POSITION, flatten(&p.positions), 3),
        (LOC_NORMAL, flatten(&p.normals), 3),
        (LOC_TEXCOORD, flatten(&p.uvs), 2),
    ];
    if let (Some(j), Some(w)) = (&joints, &p.weights) {
        attributes.push((LOC_JOINTS, flatten(j), 4));
        attributes.push((LOC_WEIGHTS, flatten(w), 4));
    }
    let colors = p.colors.as_ref().map(|c| unsafe { std::slice::from_raw_parts(c.as_ptr().cast::<u8>(), c.len() * 4) });
    Geometry::new(&attributes, colors, Some(&p.indices), p.indices.len())
}

/// Every triangle with its own three corners, marked with barycentric
/// coordinates in the colour attribute, so the shader can find its edges.
fn upload_wire(p: &Primitive) -> Geometry {
    let n = p.indices.len();
    let pick3 = |a: &[[f32; 3]]| p.indices.iter().map(|&i| a[i as usize]).collect::<Vec<_>>();
    let positions = pick3(&p.positions);
    let normals = pick3(&p.normals);
    let uvs: Vec<[f32; 2]> = p.indices.iter().map(|&i| p.uvs[i as usize]).collect();
    let joints: Option<Vec<[f32; 4]>> =
        p.joints.as_ref().map(|j| p.indices.iter().map(|&i| j[i as usize].map(|v| v as f32)).collect());
    let weights: Option<Vec<[f32; 4]>> = p.weights.as_ref().map(|w| p.indices.iter().map(|&i| w[i as usize]).collect());
    let bary: Vec<u8> = (0..n).flat_map(|k| [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]][k % 3]).collect();
    let mut attributes: Vec<(u32, &[f32], i32)> = vec![
        (LOC_POSITION, flatten(&positions), 3),
        (LOC_NORMAL, flatten(&normals), 3),
        (LOC_TEXCOORD, flatten(&uvs), 2),
    ];
    if let (Some(j), Some(w)) = (&joints, &weights) {
        attributes.push((LOC_JOINTS, flatten(j), 4));
        attributes.push((LOC_WEIGHTS, flatten(w), 4));
    }
    Geometry::new(&attributes, Some(&bary), None, n)
}

struct Texture {
    id: u32,
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { ffi::rlUnloadTexture(self.id) };
    }
}

fn upload_image(img: &Image, sampler: Sampler) -> Texture {
    let format = if img.channels == 4 {
        ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8A8
    } else {
        ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8
    } as i32;
    let pot = img.width.is_power_of_two() && img.height.is_power_of_two();
    let mips = pot || !glsl_es() || glsl_es3();
    unsafe {
        let id = ffi::rlLoadTexture(img.pixels.as_ptr().cast(), img.width as i32, img.height as i32, format, 1);
        let mut levels = 1;
        if mips && !sampler.nearest {
            ffi::rlGenTextureMipmaps(id, img.width as i32, img.height as i32, format, &mut levels);
        }
        let wrap = |repeat: bool, mirror: bool| match (repeat && mips, mirror) {
            (false, _) => ffi::RL_TEXTURE_WRAP_CLAMP,
            (true, true) => ffi::RL_TEXTURE_WRAP_MIRROR_REPEAT,
            (true, false) => ffi::RL_TEXTURE_WRAP_REPEAT,
        } as i32;
        ffi::rlTextureParameters(id, ffi::RL_TEXTURE_WRAP_S as i32, wrap(sampler.repeat_s, sampler.mirror_s));
        ffi::rlTextureParameters(id, ffi::RL_TEXTURE_WRAP_T as i32, wrap(sampler.repeat_t, sampler.mirror_t));
        let (min, mag) = if sampler.nearest {
            (ffi::RL_TEXTURE_FILTER_NEAREST, ffi::RL_TEXTURE_FILTER_NEAREST)
        } else if levels > 1 {
            (ffi::RL_TEXTURE_FILTER_MIP_LINEAR, ffi::RL_TEXTURE_FILTER_LINEAR)
        } else {
            (ffi::RL_TEXTURE_FILTER_LINEAR, ffi::RL_TEXTURE_FILTER_LINEAR)
        };
        ffi::rlTextureParameters(id, ffi::RL_TEXTURE_MIN_FILTER as i32, min as i32);
        ffi::rlTextureParameters(id, ffi::RL_TEXTURE_MAG_FILTER as i32, mag as i32);
        if levels > 1 {
            ffi::rlTextureParameters(id, ffi::RL_TEXTURE_FILTER_ANISOTROPIC as i32, 4);
        }
        ffi::rlDisableTexture();
        Texture { id }
    }
}

struct Part {
    node: usize,
    skin: Option<usize>,
    geometry: Geometry,
    wire: Option<Geometry>,
    material: usize,
    center: V3,
    primitive: (usize, usize),
}

/// A scene's GPU copy and the pose it is drawn in.
pub struct Model {
    parts: Vec<Part>,
    textures: Vec<Option<Texture>>,
    materials: Vec<Material>,
    /// Scales and moves the scene so it stands on the floor at the origin,
    /// about one unit tall or wide.
    pub placement: M4,
    pub radius: f32,
    pub height: f32,
    palettes: Vec<Vec<[f32; 4]>>,
}

impl Model {
    pub fn new(scene: &Scene, player: &Player) -> Model {
        // Every image used by some material slot gets a texture with that slot's sampler.
        let mut samplers: Vec<Option<Sampler>> = vec![None; scene.images.len()];
        for m in &scene.materials {
            for t in
                [m.base_tex, m.normal_tex, m.metal_rough_tex, m.occlusion_tex, m.emissive_tex].into_iter().flatten()
            {
                if t.image < samplers.len() {
                    samplers[t.image].get_or_insert(t.sampler);
                }
            }
        }
        let textures =
            scene.images.iter().zip(samplers).map(|(img, s)| s.map(|s| upload_image(img, s))).collect::<Vec<_>>();
        let mut parts = Vec::new();
        for &i in &scene.order {
            let node = &scene.nodes[i];
            let Some(mesh) = node.mesh else { continue };
            let skin =
                node.skin.filter(|&s| !scene.skins[s].joints.is_empty() && scene.skins[s].joints.len() <= MAX_BONES);
            for (k, p) in scene.meshes[mesh].iter().enumerate() {
                let skin = skin.filter(|_| p.skinned());
                let mut b = crate::scene::Bounds::EMPTY;
                for pos in &p.positions {
                    b.add(V3::from(*pos));
                }
                parts.push(Part {
                    node: i,
                    skin,
                    geometry: upload(p),
                    wire: None,
                    material: p.material,
                    center: if b.valid() { b.center() } else { V3::ZERO },
                    primitive: (mesh, k),
                });
            }
        }
        let bounds = crate::anim::bounds(scene, player);
        let size = bounds.size();
        let span = size.x.max(size.y).max(size.z).max(1e-6);
        let scale = 1.0 / span;
        let c = bounds.center();
        let placement = M4::scale(v3(scale, scale, scale)).mul(&M4::translate(v3(-c.x, -bounds.min.y, -c.z)));
        let half = size * (0.5 * scale);
        Model {
            parts,
            textures,
            materials: scene.materials.clone(),
            placement,
            radius: half.len().max(0.05),
            height: size.y * scale,
            palettes: vec![Vec::new(); scene.skins.len()],
        }
    }

    /// Builds the edge-marked copies the wireframe style draws.
    pub fn ensure_wire(&mut self, scene: &Scene) {
        for part in &mut self.parts {
            if part.wire.is_none() {
                let (mesh, k) = part.primitive;
                part.wire = Some(upload_wire(&scene.meshes[mesh][k]));
            }
        }
    }

    pub fn update_pose(&mut self, scene: &Scene, player: &Player) {
        let mut palette = Vec::new();
        for (s, rows) in self.palettes.iter_mut().enumerate() {
            if scene.skins[s].joints.len() > MAX_BONES {
                continue;
            }
            player.palette(scene, s, &mut palette);
            rows.clear();
            for m in &palette {
                rows.extend(m.rows3());
            }
        }
    }
}

/// The lights and colours of the stage.
#[derive(Clone, Copy)]
pub struct Stage {
    pub sky: [f32; 3],
    pub ground: [f32; 3],
    pub floor: [f32; 4],
    pub exposure: f32,
}

pub const STAGE: Stage =
    Stage { sky: [0.42, 0.45, 0.52], ground: [0.16, 0.15, 0.15], floor: [0.20, 0.21, 0.25, 1.0], exposure: 1.15 };

/// Where the camera is and what it sees, in the stage's units.
#[derive(Clone, Copy)]
pub struct View {
    pub eye: V3,
    pub target: V3,
    pub fovy: f32,
    /// The model's turn about the vertical axis.
    pub spin: f32,
    /// The area of the screen drawn into, in device pixels.
    pub viewport: (i32, i32, i32, i32),
}

pub struct Renderer {
    surface: Program,
    surface_skinned: Program,
    silhouette: Program,
    silhouette_skinned: Program,
    floor: Program,
    floor_quad: Geometry,
    shadow: ffi::RenderTexture2D,
    flat_normal: u32,
    white: u32,
    pub style: Style,
}

fn light_dirs(view: &View) -> (V3, V3, V3) {
    let forward = (view.target - view.eye).norm();
    let right = forward.cross(v3(0.0, 1.0, 0.0)).norm();
    let back = -forward;
    let key = (back * 0.55 + right * -0.6 + v3(0.0, 0.85, 0.0)).norm();
    let fill = (back * 0.8 + right * 0.75 + v3(0.0, 0.2, 0.0)).norm();
    let rim = (forward * 0.9 + right * 0.35 + v3(0.0, 0.55, 0.0)).norm();
    (key, fill, rim)
}

impl Renderer {
    pub fn new() -> Renderer {
        let skinned = format!("#define SKINNED\n#define BONES {MAX_BONES}\n");
        let surface = Program::load("", VERTEX, SURFACE);
        let surface_skinned = Program::load(&skinned, VERTEX, SURFACE);
        let silhouette = Program::load("", VERTEX, SILHOUETTE);
        let silhouette_skinned = Program::load(&skinned, VERTEX, SILHOUETTE);
        let floor = Program::load("", FLOOR_VERTEX, FLOOR);
        for p in [&surface, &surface_skinned, &silhouette, &silhouette_skinned, &floor] {
            unsafe { ffi::rlEnableShader(p.id) };
            p.bind_samplers();
        }
        unsafe { ffi::rlDisableShader() };
        let r = 3.0f32;
        let quad: [[f32; 3]; 6] = [[-r, 0.0, -r], [-r, 0.0, r], [r, 0.0, r], [-r, 0.0, -r], [r, 0.0, r], [r, 0.0, -r]];
        let floor_quad = Geometry::new(&[(LOC_POSITION, flatten(&quad), 3)], None, None, 6);
        let shadow = unsafe { ffi::LoadRenderTexture(SHADOW_SIZE, SHADOW_SIZE) };
        unsafe {
            ffi::SetTextureFilter(shadow.texture, ffi::TextureFilter::TEXTURE_FILTER_BILINEAR as i32);
            ffi::SetTextureWrap(shadow.texture, ffi::TextureWrap::TEXTURE_WRAP_CLAMP as i32);
        }
        let px = [128u8, 128, 255];
        let flat_normal = unsafe {
            ffi::rlLoadTexture(px.as_ptr().cast(), 1, 1, ffi::PixelFormat::PIXELFORMAT_UNCOMPRESSED_R8G8B8 as i32, 1)
        };
        let white = unsafe { ffi::rlGetTextureIdDefault() };
        Renderer {
            surface,
            surface_skinned,
            silhouette,
            silhouette_skinned,
            floor,
            floor_quad,
            shadow,
            flat_normal,
            white,
            style: Style::Studio,
        }
    }

    fn bind(&self, slot: i32, id: u32) {
        unsafe {
            ffi::rlActiveTextureSlot(slot);
            ffi::rlEnableTexture(id);
        }
    }

    fn texture(&self, model: &Model, t: Option<TexRef>, fallback: u32) -> u32 {
        t.and_then(|t| model.textures.get(t.image)).and_then(|t| t.as_ref()).map_or(fallback, |t| t.id)
    }

    fn light_vp(model: &Model, view: &View) -> M4 {
        let (key, _, _) = light_dirs(view);
        // Light the floor from high above, a little toward the key light.
        let dir = (key + v3(0.0, 1.6, 0.0)).norm();
        let center = v3(0.0, model.height * 0.4, 0.0);
        let r = model.radius * 1.3 + 0.1;
        let eye = center + dir * 4.0;
        let up = if dir.y.abs() > 0.99 { v3(0.0, 0.0, 1.0) } else { v3(0.0, 1.0, 0.0) };
        M4::ortho(-r, r, -r, r, 0.1, 10.0).mul(&M4::look_at(eye, center, up))
    }

    fn model_matrix(model: &Model, view: &View, part: &Part, world: &[M4]) -> M4 {
        let base = M4::rotate_y(view.spin).mul(&model.placement);
        if part.skin.is_some() {
            base
        } else {
            base.mul(&world[part.node])
        }
    }

    /// Draws the model's silhouette from the light into the shadow texture.
    pub fn render_shadow(&self, model: &Model, view: &View, world: &[M4]) {
        let vp = Self::light_vp(model, view);
        unsafe {
            ffi::BeginTextureMode(self.shadow);
            ffi::ClearBackground(ffi::Color { r: 255, g: 255, b: 255, a: 255 });
            ffi::rlDrawRenderBatchActive();
            ffi::rlEnableDepthTest();
            ffi::rlDisableBackfaceCulling();
        }
        for part in &model.parts {
            let material = &model.materials[part.material];
            if material.blended() && material.base_color[3] < 0.5 {
                continue;
            }
            let program = if part.skin.is_some() { &self.silhouette_skinned } else { &self.silhouette };
            unsafe { ffi::rlEnableShader(program.id) };
            let m = Self::model_matrix(model, view, part, world);
            program.mat("mvp", &vp.mul(&m));
            if let Some(s) = part.skin {
                program.bones(&model.palettes[s]);
            }
            let cutoff = if let Alpha::Mask(c) = material.alpha { c } else { -1.0 };
            program.vec4("flags", [0.0, cutoff, 0.0, 0.0]);
            self.bind(0, self.texture(model, material.base_tex, self.white));
            part.geometry.draw();
        }
        unsafe {
            ffi::rlDisableShader();
            ffi::rlActiveTextureSlot(0);
            ffi::rlDisableTexture();
            ffi::rlDisableDepthTest();
            ffi::rlEnableBackfaceCulling();
            ffi::EndTextureMode();
        }
    }

    fn set_lights(&self, p: &Program, view: &View, stage: &Stage) {
        let (key, fill, rim) = light_dirs(view);
        p.vec3("camPos", view.eye.arr());
        p.vec3("keyDir", key.arr());
        p.vec3("keyColor", [2.6, 2.45, 2.2]);
        p.vec3("fillDir", fill.arr());
        p.vec3("fillColor", [0.55, 0.62, 0.78]);
        p.vec3("rimDir", rim.arr());
        p.vec3("rimColor", [1.5, 1.55, 1.7]);
        p.vec3("skyColor", stage.sky);
        p.vec3("groundColor", stage.ground);
        p.vec3("wireColor", [0.13, 0.15, 0.21]);
        p.float("exposure", stage.exposure);
        p.float("mode", self.style.code());
    }

    fn projection(view: &View) -> M4 {
        let (_, _, w, h) = view.viewport;
        M4::perspective(view.fovy, w as f32 / h.max(1) as f32, 0.02, 40.0).mul(&M4::look_at(
            view.eye,
            view.target,
            v3(0.0, 1.0, 0.0),
        ))
    }

    pub fn draw_floor(&self, model: &Model, view: &View, stage: &Stage) {
        let vp = Self::projection(view);
        let (x, y, w, h) = view.viewport;
        unsafe {
            ffi::rlDrawRenderBatchActive();
            ffi::rlViewport(x, y, w, h);
            ffi::rlEnableShader(self.floor.id);
            ffi::rlEnableColorBlend();
            ffi::rlSetBlendMode(ffi::rlBlendMode::RL_BLEND_ALPHA as i32);
            ffi::rlDisableBackfaceCulling();
        }
        let scale = (model.radius * 2.2).max(0.9);
        let m = M4::scale(v3(scale / 3.0, 1.0, scale / 3.0));
        self.floor.mat("mvp", &vp.mul(&m));
        self.floor.mat("matModel", &m);
        self.floor.mat("lightVP", &Self::light_vp(model, view));
        self.floor.vec4("floorColor", stage.floor);
        let strength = if self.style == Style::Normals { 0.0 } else { 0.62 };
        self.floor.vec4("shadow", [1.6 / SHADOW_SIZE as f32, scale, strength, 0.25]);
        self.bind(5, self.shadow.texture.id);
        self.floor_quad.draw();
        unsafe {
            ffi::rlActiveTextureSlot(5);
            ffi::rlDisableTexture();
            ffi::rlActiveTextureSlot(0);
            ffi::rlDisableShader();
            ffi::rlEnableBackfaceCulling();
        }
    }

    pub fn draw_model(&self, model: &Model, view: &View, world: &[M4], stage: &Stage) {
        let vp = Self::projection(view);
        let wire = self.style == Style::Wireframe;
        let (x, y, w, h) = view.viewport;
        unsafe {
            ffi::rlDrawRenderBatchActive();
            ffi::rlViewport(x, y, w, h);
            ffi::rlEnableDepthTest();
            ffi::rlEnableDepthMask();
        }
        for p in [&self.surface, &self.surface_skinned] {
            unsafe { ffi::rlEnableShader(p.id) };
            self.set_lights(p, view, stage);
        }
        // Opaque parts first, then see-through ones from back to front.
        let mut order: Vec<(usize, f32)> = Vec::with_capacity(model.parts.len());
        for (i, part) in model.parts.iter().enumerate() {
            let m = Self::model_matrix(model, view, part, world);
            let c = m.point(part.center);
            order.push((i, (c - view.eye).len()));
        }
        let blended = |i: usize| self.style == Style::Studio && model.materials[model.parts[i].material].blended();
        order.sort_by(|a, b| blended(a.0).cmp(&blended(b.0)).then(b.1.total_cmp(&a.1)));
        let mut last_program = 0;
        for (i, _) in order {
            let part = &model.parts[i];
            let material = &model.materials[part.material];
            let program = if part.skin.is_some() { &self.surface_skinned } else { &self.surface };
            if program.id != last_program {
                unsafe { ffi::rlEnableShader(program.id) };
                last_program = program.id;
            }
            let m = Self::model_matrix(model, view, part, world);
            program.mat("mvp", &vp.mul(&m));
            program.mat("matModel", &m);
            program.mat("matNormal", &m.normal_matrix());
            if let Some(s) = part.skin {
                program.bones(&model.palettes[s]);
            }
            let studio = self.style == Style::Studio;
            let cutoff = if let Alpha::Mask(c) = material.alpha { c } else { -1.0 };
            program.vec4("baseColor", if studio { material.base_color } else { [1.0; 4] });
            program.vec3("emissiveColor", material.emissive);
            program.vec4(
                "pbr",
                [material.metallic, material.roughness, material.occlusion_strength, material.normal_scale],
            );
            let normal_map = if material.normal_tex.is_some() { 1.0 } else { 0.0 };
            let unlit = if material.unlit { 1.0 } else { 0.0 };
            program.vec4("flags", [normal_map, cutoff, material.transmission, unlit]);
            self.bind(0, self.texture(model, material.base_tex, self.white));
            self.bind(1, self.texture(model, material.normal_tex, self.flat_normal));
            self.bind(2, self.texture(model, material.metal_rough_tex, self.white));
            self.bind(3, self.texture(model, material.occlusion_tex, self.white));
            self.bind(4, self.texture(model, material.emissive_tex, self.white));
            let see_through = blended(i);
            unsafe {
                if material.double_sided || see_through || wire {
                    ffi::rlDisableBackfaceCulling();
                } else {
                    ffi::rlEnableBackfaceCulling();
                    let mirrored = m.det3() < 0.0;
                    ffi::rlSetCullFace(if mirrored {
                        ffi::rlCullMode::RL_CULL_FACE_FRONT
                    } else {
                        ffi::rlCullMode::RL_CULL_FACE_BACK
                    } as i32);
                }
                if see_through {
                    ffi::rlEnableColorBlend();
                    ffi::rlSetBlendMode(ffi::rlBlendMode::RL_BLEND_ALPHA as i32);
                    ffi::rlDisableDepthMask();
                } else {
                    ffi::rlDisableColorBlend();
                    ffi::rlEnableDepthMask();
                }
            }
            match (&part.wire, wire) {
                (Some(g), true) => g.draw(),
                _ => part.geometry.draw(),
            }
        }
        unsafe {
            for slot in (0..5).rev() {
                ffi::rlActiveTextureSlot(slot);
                ffi::rlDisableTexture();
            }
            ffi::rlDisableShader();
            ffi::rlEnableColorBlend();
            ffi::rlEnableDepthMask();
            ffi::rlDisableDepthTest();
            ffi::rlEnableBackfaceCulling();
            ffi::rlSetCullFace(ffi::rlCullMode::RL_CULL_FACE_BACK as i32);
        }
    }
}

/// Hands the whole target back to raylib's 2D drawing after a 3D pass.
pub fn restore_viewport(width: i32, height: i32) {
    unsafe { ffi::rlViewport(0, 0, width, height) };
}

impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            for p in [&self.surface, &self.surface_skinned, &self.silhouette, &self.silhouette_skinned, &self.floor] {
                ffi::rlUnloadShaderProgram(p.id);
            }
            ffi::UnloadRenderTexture(self.shadow);
            ffi::rlUnloadTexture(self.flat_normal);
        }
    }
}
