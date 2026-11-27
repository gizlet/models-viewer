// Downloads every model in sources.mjs at its pinned revision, checks its
// license at the source and writes a handheld copy to assets/models/: one
// self-contained GLB with textures of at most 512 px, static meshes simplified
// to a triangle budget, unused parts and animations removed. Also writes
// assets/models/catalog.json for the app and ASSETS.md with the credits.
//
//   npm run build                 build everything, checking sources.lock.json
//   npm run build -- --update     accept changed downloads and rewrite the lock
//   npm run build -- fox knight   build only the ids containing these words

import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readdir, readFile, unlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { NodeIO } from '@gltf-transform/core';
import { ALL_EXTENSIONS } from '@gltf-transform/extensions';
import {
  dedup,
  dequantize,
  flatten,
  getBounds,
  join,
  metalRough,
  prune,
  resample,
  simplify,
  uninstance,
  unpartition,
  weld,
} from '@gltf-transform/functions';
import draco3d from 'draco3dgltf';
import { unzipSync } from 'fflate';
import { MeshoptDecoder, MeshoptSimplifier } from 'meshoptimizer';
import obj2gltf from 'obj2gltf';
import sharp from 'sharp';
import { CATEGORIES, MODELS, REVISIONS } from './sources.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const OUT = path.join(ROOT, 'assets/models');
const CACHE = path.join(HERE, '.cache');
const LOCK = path.join(HERE, 'sources.lock.json');
const USER_AGENT = 'GizletModelsViewer/0.1 (+https://github.com/gizlet/models-viewer)';
const LICENSES = {
  'CC0-1.0': 'https://creativecommons.org/publicdomain/zero/1.0/',
  'CC-BY-4.0': 'https://creativecommons.org/licenses/by/4.0/',
};
// What the app's loader reads beyond core glTF; anything else required fails the build.
const SUPPORTED_REQUIRED = new Set(['KHR_texture_transform', 'KHR_materials_emissive_strength']);
// Decoded or converted here, so the copies do not carry them.
const STRIPPED = [
  'KHR_draco_mesh_compression',
  'EXT_meshopt_compression',
  'KHR_mesh_quantization',
  'EXT_texture_webp',
  'EXT_mesh_gpu_instancing',
  'KHR_materials_pbrSpecularGlossiness',
  'KHR_materials_variants',
];
const MAX_JOINTS = 100;
const BASE_TEXTURE = 512;
const DETAIL_TEXTURE = 256;
const DEFAULT_TRIANGLES = 16000;

const args = process.argv.slice(2);
const update = args.includes('--update');
const only = args.filter((arg) => !arg.startsWith('--'));
const lock = JSON.parse(await readFile(LOCK, 'utf8').catch(() => '{}'));

await MeshoptDecoder.ready;
await MeshoptSimplifier.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({
  'draco3d.decoder': await draco3d.createDecoderModule(),
  'meshopt.decoder': MeshoptDecoder,
});

const sha256 = (data) => createHash('sha256').update(data).digest('hex');

async function download(url) {
  await mkdir(CACHE, { recursive: true });
  const cached = path.join(CACHE, createHash('sha1').update(url).digest('hex'));
  let data = await readFile(cached).catch(() => null);
  if (!data) {
    const response = await fetch(url, { headers: { 'User-Agent': USER_AGENT } });
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}: ${url}`);
    data = Buffer.from(await response.arrayBuffer());
    await writeFile(cached, data);
  }
  const hash = sha256(data);
  if (lock[url] && lock[url] !== hash && !update) {
    throw new Error(`${url} changed (sha256 ${hash}, locked ${lock[url]}); rerun with --update after reviewing it`);
  }
  lock[url] = hash;
  return data;
}

const downloadJson = async (url) => JSON.parse((await download(url)).toString('utf8'));

async function readGltf(url, json) {
  const resources = {};
  for (const item of [...(json.buffers ?? []), ...(json.images ?? [])]) {
    if (item.uri && !item.uri.startsWith('data:')) {
      resources[item.uri] = new Uint8Array(await download(new URL(item.uri, url).href));
    }
  }
  return io.readJSON({ json, resources });
}

async function load(model) {
  const { source } = model;
  if (source.kind === 'glb') return io.readBinary(new Uint8Array(await download(source.url)));
  if (source.kind === 'gltf') return readGltf(source.url, await downloadJson(source.url));
  if (source.kind === 'polyhaven') {
    const files = await downloadJson(`https://api.polyhaven.com/files/${source.asset}`);
    const gltf = files.gltf['1k'].gltf;
    const json = await downloadJson(gltf.url);
    const resources = {};
    for (const [uri, file] of Object.entries(gltf.include)) {
      resources[uri] = new Uint8Array(await download(file.url));
    }
    return io.readJSON({ json, resources });
  }
  if (source.kind === 'fuel') {
    const files = unzipSync(new Uint8Array(await download(source.url)));
    const config = Buffer.from(files['model.config']).toString('utf8');
    if (!config.includes('creativecommons.org/licenses/by/4.0')) throw new Error(`${model.id}: no CC BY 4.0 notice`);
    const dir = await mkdtemp(path.join(tmpdir(), 'models-viewer-'));
    await mkdir(path.join(dir, 'meshes'));
    // model.mtl names texture.png beside itself.
    for (const [name, target] of [
      ['meshes/model.obj', 'model.obj'],
      ['meshes/model.mtl', 'model.mtl'],
      ['materials/textures/texture.png', 'texture.png'],
    ]) {
      await writeFile(path.join(dir, 'meshes', target), files[name]);
    }
    const glb = await obj2gltf(path.join(dir, 'meshes/model.obj'), { binary: true, secure: true, checkTransparency: true });
    return io.readBinary(new Uint8Array(glb));
  }
  throw new Error(`${model.id}: unknown source ${source.kind}`);
}

async function credits(model) {
  const { license } = model;
  if (license.kind === 'kaykit') {
    const text = (await download(license.url)).toString('utf8');
    if (!text.includes('creativecommons.org/publicdomain/zero/1.0')) throw new Error(`${model.id}: KayKit license is not CC0`);
    return { authors: ['Kay Lousberg (kaylousberg.com)'], licenses: ['CC0-1.0'] };
  }
  if (license.kind === 'khronos') {
    const meta = await downloadJson(license.url);
    const legal = meta.legal.filter((entry) => !/logo/i.test(entry.artist));
    const licenses = [...new Set(legal.map((entry) => entry.spdx))];
    if (!licenses.every((id) => LICENSES[id]) || legal.length !== meta.legal.length) {
      throw new Error(`${model.id}: unsupported Khronos legal entries ${JSON.stringify(meta.legal)}`);
    }
    return {
      authors: legal.map((entry) => `${entry.artist} (${entry.what.toLowerCase()}, ${entry.spdx})`),
      licenses,
    };
  }
  if (license.kind === 'polyhaven') {
    const info = await downloadJson(`https://api.polyhaven.com/info/${license.asset}`);
    if (info.type !== 2) throw new Error(`${model.id}: not a Poly Haven model`);
    return { authors: Object.keys(info.authors), licenses: ['CC0-1.0'] };
  }
  if (license.kind === 'fuel') {
    return { authors: ['Google Research (© 2020 Google LLC)'], licenses: ['CC-BY-4.0'] };
  }
  if (license.kind === 'nvidia') {
    const manifest = await downloadJson(license.url);
    if (manifest.license !== 'CC-BY-4.0') throw new Error(`${model.id}: NVIDIA license is ${manifest.license}`);
    return {
      authors: ['NVIDIA Corporation (model)', 'codepawl (glTF conversion, simready-glb)'],
      licenses: ['CC-BY-4.0'],
    };
  }
  throw new Error(`${model.id}: unknown license source ${license.kind}`);
}

function disposeTree(node) {
  for (const child of node.listChildren()) disposeTree(child);
  node.dispose();
}

// Disposing an animation leaves its samplers holding their keyframes, so prune would keep them.
function disposeAnimation(anim) {
  for (const channel of anim.listChannels()) channel.dispose();
  for (const sampler of anim.listSamplers()) sampler.dispose();
  anim.dispose();
}

function meshInstances(doc) {
  const instances = [];
  for (const scene of doc.getRoot().listScenes()) {
    scene.traverse((node) => {
      const mesh = node.getMesh();
      if (mesh) instances.push({ node, mesh });
    });
  }
  return instances;
}

function triangleCount(doc) {
  let triangles = 0;
  for (const { mesh } of meshInstances(doc)) {
    for (const prim of mesh.listPrimitives()) {
      if (prim.getMode() !== 4) continue;
      const count = prim.getIndices()?.getCount() ?? prim.getAttribute('POSITION').getCount();
      triangles += count / 3;
    }
  }
  return Math.round(triangles);
}

function vertexCount(doc) {
  let vertices = 0;
  for (const { mesh } of meshInstances(doc)) {
    for (const prim of mesh.listPrimitives()) vertices += prim.getAttribute('POSITION').getCount();
  }
  return vertices;
}

async function shrinkTextures(doc, notes) {
  let largest = 0;
  for (const texture of doc.getRoot().listTextures()) {
    const materials = texture.listParents().filter((parent) => parent.propertyType === 'Material');
    const base = materials.some(
      (material) => material.getBaseColorTexture() === texture || material.getEmissiveTexture() === texture,
    );
    const normal = materials.some((material) => material.getNormalTexture() === texture);
    const alpha = materials.some(
      (material) => material.getBaseColorTexture() === texture && material.getAlphaMode() !== 'OPAQUE',
    );
    const image = texture.getImage();
    if (!image) continue;
    const meta = await sharp(image).metadata();
    largest = Math.max(largest, meta.width, meta.height);
    const limit = base ? BASE_TEXTURE : DETAIL_TEXTURE;
    let pipeline = sharp(image).resize(limit, limit, { fit: 'inside', withoutEnlargement: true });
    let mime;
    if (alpha && meta.hasAlpha) {
      pipeline = pipeline.png({ compressionLevel: 9, palette: false });
      mime = 'image/png';
    } else {
      pipeline = pipeline.removeAlpha().jpeg({ quality: normal ? 92 : 86, mozjpeg: true, chromaSubsampling: normal ? '4:4:4' : '4:2:0' });
      mime = 'image/jpeg';
    }
    texture.setImage(new Uint8Array(await pipeline.toBuffer())).setMimeType(mime);
    texture.setURI(`${texture.getName() || 'texture'}.${mime === 'image/png' ? 'png' : 'jpg'}`);
  }
  if (largest > BASE_TEXTURE) {
    notes.push([
      `textures reduced from ${largest} px to at most ${BASE_TEXTURE} px`,
      `贴图从 ${largest} 像素缩小到不超过 ${BASE_TEXTURE} 像素`,
    ]);
  }
}

function rotateZUp(doc) {
  const root = doc.getRoot();
  for (const scene of root.listScenes()) {
    const pivot = doc.createNode('z-up').setRotation([-Math.SQRT1_2, 0, 0, Math.SQRT1_2]);
    for (const child of scene.listChildren()) {
      scene.removeChild(child);
      pivot.addChild(child);
    }
    scene.addChild(pivot);
  }
}

async function prepare(model) {
  const notes = [];
  const doc = await load(model);
  const root = doc.getRoot();
  if (model.source.kind === 'fuel') notes.push(['converted from OBJ', '从 OBJ 转换']);
  if (model.source.kind === 'gltf' || model.source.kind === 'polyhaven') notes.push(['packed into one GLB', '打包为单个 GLB']);
  if (model.drop?.length) {
    const names = new Set(root.listNodes().map((node) => node.getName()));
    const missing = model.drop.filter((name) => !names.has(name));
    if (missing.length) throw new Error(`${model.id}: no nodes named ${missing.join(', ')}`);
    for (const node of root.listNodes()) if (model.drop.includes(node.getName()) && !node.isDisposed()) disposeTree(node);
    notes.push([`removed ${model.drop.length} prop${model.drop.length === 1 ? "" : "s"}`, `移除 ${model.drop.length} 件道具`]);
  }
  const animations = root.listAnimations();
  if (model.anims?.length) {
    const names = new Set(animations.map((anim) => anim.getName()));
    const missing = model.anims.filter((name) => !names.has(name));
    if (missing.length) throw new Error(`${model.id}: no animations named ${missing.join(', ')}`);
    for (const anim of animations) if (!model.anims.includes(anim.getName())) disposeAnimation(anim);
    notes.push([`kept ${model.anims.length} of ${animations.length} animations`, `保留 ${animations.length} 个动画中的 ${model.anims.length} 个`]);
  }
  if (model.upAxis === 'z') rotateZUp(doc);
  // The app's shader works tangents out per pixel.
  for (const mesh of root.listMeshes()) {
    for (const prim of mesh.listPrimitives()) prim.setAttribute('TANGENT', null);
  }
  const used = root.listExtensionsUsed().map((ext) => ext.extensionName);
  if (used.includes('KHR_texture_basisu')) throw new Error(`${model.id}: KTX2 textures cannot be read on the handheld`);
  await doc.transform(uninstance(), dequantize());
  if (used.includes('KHR_materials_pbrSpecularGlossiness')) await doc.transform(metalRough());
  await doc.transform(dedup(), prune(), weld());

  const skinned = root.listSkins().length > 0;
  const animated = root.listAnimations().length > 0;
  const before = triangleCount(doc);
  const budget = model.maxTriangles ?? DEFAULT_TRIANGLES;
  if (!skinned && !animated) {
    await doc.transform(flatten(), join(), weld());
    // Models just over the budget are left whole.
    if (before > budget * 1.15) {
      for (const error of [0.005, 0.02, 0.05, 0.1]) {
        await doc.transform(simplify({ simplifier: MeshoptSimplifier, ratio: budget / triangleCount(doc), error }));
        if (triangleCount(doc) <= budget * 1.1) break;
      }
      const [from, to] = [before, triangleCount(doc)].map((n) => n.toLocaleString('en'));
      notes.push([`simplified from ${from} to ${to} triangles`, `三角面从 ${from} 简化到 ${to}`]);
    }
  }
  // Resampling gives every sampler its own copy of the keyframe times; dedup shares them again.
  if (animated) await doc.transform(resample({ tolerance: 1e-3 }), dedup());
  await shrinkTextures(doc, notes);
  await doc.transform(prune(), unpartition());
  for (const ext of root.listExtensionsUsed()) if (STRIPPED.includes(ext.extensionName)) ext.dispose();
  const required = root.listExtensionsRequired().map((ext) => ext.extensionName);
  const unsupported = required.filter((name) => !SUPPORTED_REQUIRED.has(name));
  if (unsupported.length) throw new Error(`${model.id}: requires ${unsupported.join(', ')}`);
  for (const skin of root.listSkins()) {
    if (skin.listJoints().length > MAX_JOINTS) throw new Error(`${model.id}: ${skin.listJoints().length} joints`);
  }
  const glb = Buffer.from(await io.writeBinary(doc));
  const bounds = getBounds(root.getDefaultScene() ?? root.listScenes()[0]);
  const size = bounds.max.map((max, axis) => Math.round((max - bounds.min[axis]) * 1000) / 1000);
  return {
    glb,
    notes,
    stats: {
      triangles: triangleCount(doc),
      vertices: vertexCount(doc),
      materials: root.listMaterials().length,
      textures: root.listTextures().length,
      joints: Math.max(0, ...root.listSkins().map((skin) => skin.listJoints().length)),
      animations: root.listAnimations().map((anim) => anim.getName()),
      bytes: glb.length,
      size,
    },
  };
}

async function build(model) {
  const [{ glb, notes, stats }, credit] = await Promise.all([prepare(model), credits(model)]);
  await writeFile(path.join(OUT, `${model.id}.glb`), glb);
  const sentence = (text) => text[0].toUpperCase() + text.slice(1);
  const changes = {
    en: notes.length ? sentence(`${notes.map(([en]) => en).join('; ')}.`) : 'Copied unchanged.',
    'zh-CN': notes.length ? `${notes.map(([, zh]) => zh).join('；')}。` : '未作改动。',
  };
  console.log(
    `${model.id.padEnd(36)} ${String(stats.triangles).padStart(6)} tris ${String(Math.round(stats.bytes / 1024)).padStart(5)} KB  ${changes.en}`,
  );
  return {
    id: model.id,
    title: { en: model.en, 'zh-CN': model.zh },
    category: model.category,
    collection: model.collection,
    authors: credit.authors,
    licenses: credit.licenses,
    source: model.page,
    changes,
    stats,
  };
}

async function pool(items, limit, fn) {
  const results = new Array(items.length);
  let next = 0;
  const workers = Array.from({ length: limit }, async () => {
    while (next < items.length) {
      const index = next++;
      results[index] = await fn(items[index]);
    }
  });
  await Promise.all(workers);
  return results;
}

function assetsMarkdown(entries) {
  const lines = [
    '# Assets',
    '',
    'Every model in Models Viewer is CC0 1.0 or CC BY 4.0. The handheld copies in `assets/models/` are made from the',
    'pinned sources below by `tools/models/build.mjs`, which records each download in `tools/models/sources.lock.json`.',
    'The changes column describes what the build did to each one. The same credits are shown in the app on every',
    "model's information panel.",
    '',
    '| Model | Authors | Collection | License | Changes |',
    '| --- | --- | --- | --- | --- |',
  ];
  for (const entry of entries) {
    const licenses = entry.licenses.map((id) => `[${id}](${LICENSES[id]})`).join(', ');
    lines.push(
      `| [${entry.title.en}](${entry.source}) | ${entry.authors.join('; ')} | ${entry.collection} | ${licenses} | ${entry.changes.en} |`,
    );
  }
  lines.push(
    '',
    '## Sources',
    '',
    `- Khronos glTF Sample Assets at \`${REVISIONS.khronos}\`; each model's \`metadata.json\` lists its authors and licenses.`,
    `- KayKit Adventurers 1.0 at \`${REVISIONS.kaykitAdventurers}\` and KayKit Skeletons 1.0 at \`${REVISIONS.kaykitSkeletons}\`, by Kay Lousberg, CC0 1.0.`,
    '- Poly Haven models through the [Poly Haven API](https://polyhaven.com/our-api), CC0 1.0 ([license](https://polyhaven.com/license)).',
    '- Google Scanned Objects from [Gazebo Fuel](https://app.gazebosim.org/GoogleResearch/fuel/collections/Scanned%20Objects%20by%20Google%20Research), version 1 of each model, CC BY 4.0, © 2020 Google LLC.',
    `- NVIDIA SimReady Assets at \`${REVISIONS.nvidia}\` (CC BY 4.0), as converted to glTF by [codepawl/simready-glb](https://huggingface.co/datasets/codepawl/simready-glb) at \`${REVISIONS.codepawl}\` (CC BY 4.0).`,
    '',
    '## Fonts',
    '',
    '- Inter and Space Grotesk, SIL Open Font License 1.1 (`assets/fonts/Inter-OFL.txt`, `assets/fonts/SpaceGrotesk-OFL.txt`).',
    '- Noto Sans SC, SIL Open Font License 1.1 (`assets/fonts/NotoSansSC-OFL.txt`), subset to the characters the app uses.',
    '',
  );
  return lines.join('\n');
}

await mkdir(OUT, { recursive: true });
const selected = only.length ? MODELS.filter((model) => only.some((word) => model.id.includes(word))) : MODELS;
if (new Set(MODELS.map((model) => model.id)).size !== MODELS.length) throw new Error('duplicate model ids');
const rebuilt = await pool(selected, 4, build);
let entries = rebuilt;
if (only.length) {
  // A partial build keeps the other models' entries from the last full one.
  const previous = JSON.parse(await readFile(path.join(OUT, 'catalog.json'), 'utf8').catch(() => 'null'));
  const byId = new Map((previous?.models ?? []).map((entry) => [entry.id, entry]));
  for (const entry of rebuilt) byId.set(entry.id, entry);
  const missing = MODELS.filter((model) => !byId.has(model.id)).map((model) => model.id);
  if (missing.length) throw new Error(`build every model once before a partial build; missing ${missing.join(', ')}`);
  entries = MODELS.map((model) => byId.get(model.id));
} else {
  const kept = new Set(MODELS.map((model) => `${model.id}.glb`));
  for (const file of await readdir(OUT)) {
    if (file.endsWith('.glb') && !kept.has(file)) await unlink(path.join(OUT, file));
  }
}
const catalog = {
  schema: 1,
  categories: CATEGORIES.map(({ id, en, zh }) => ({ id, title: { en, 'zh-CN': zh } })),
  models: entries,
};
await writeFile(path.join(OUT, 'catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`);
await writeFile(path.join(ROOT, 'ASSETS.md'), assetsMarkdown(entries));
const total = entries.reduce((sum, entry) => sum + entry.stats.bytes, 0);
console.log(`${entries.length} models, ${(total / 1048576).toFixed(1)} MiB`);
const sorted = Object.fromEntries(Object.entries(lock).sort(([a], [b]) => a.localeCompare(b)));
await writeFile(LOCK, `${JSON.stringify(sorted, null, 2)}\n`);
