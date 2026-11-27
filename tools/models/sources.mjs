// The Models Viewer catalog: every model the app ships, where it comes from
// and how its handheld copy is prepared. `build.mjs` downloads the pinned
// sources, checks each license at the source and writes assets/models/.

export const REVISIONS = {
  khronos: 'e124e8d8d7330a5372bc2b4d00d39a4a9d7997ea',
  kaykitAdventurers: '672074b73ba276876a19e8816ecdc5241817ab47',
  kaykitSkeletons: '15b62b9bad122f72926c10fb14d622c73819fa54',
  nvidia: '99bb6c7c1247ff3a1f63e484996336e9e35b9cf8',
  codepawl: 'd529a9403ea062d57b3c0c5d7c01473b0db0beef',
};

export const CATEGORIES = [
  { id: 'characters', en: 'Characters', zh: '角色' },
  { id: 'animals', en: 'Animals', zh: '动物' },
  { id: 'toys', en: 'Toys', zh: '玩具' },
  { id: 'home', en: 'Home', zh: '家居' },
  { id: 'gadgets', en: 'Gadgets', zh: '器物' },
  { id: 'food', en: 'Food', zh: '美食' },
  { id: 'art', en: 'Art', zh: '艺术' },
];

const ADVENTURER_ANIMS = ['Idle', 'Walking_A', 'Running_A', 'Jump_Full_Short', 'Cheer', 'Dodge_Forward', 'Death_A'];

function kaykit(pack, file, en, zh, { drop = [], anims = [] } = {}) {
  const repo =
    pack === 'skeletons'
      ? ['KayKit-Character-Pack-Skeletons-1.0', REVISIONS.kaykitSkeletons, 'kaykit_character_pack_skeletons']
      : ['KayKit-Character-Pack-Adventures-1.0', REVISIONS.kaykitAdventurers, 'kaykit_character_pack_adventures'];
  const [name, rev, addon] = repo;
  const raw = `https://raw.githubusercontent.com/KayKit-Game-Assets/${name}/${rev}`;
  return {
    id: `kaykit-${file.toLowerCase().replaceAll('_', '-')}`,
    en,
    zh,
    category: 'characters',
    source: { kind: 'glb', url: `${raw}/addons/${addon}/Characters/gltf/${file}.glb` },
    license: { kind: 'kaykit', url: `${raw}/LICENSE.txt` },
    page: `https://github.com/KayKit-Game-Assets/${name}/tree/${rev}`,
    collection: pack === 'skeletons' ? 'KayKit Skeletons 1.0' : 'KayKit Adventurers 1.0',
    drop,
    anims,
  };
}

function khronos(name, en, zh, category, options = {}) {
  const base = `https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/${REVISIONS.khronos}/Models/${name}`;
  const source = options.gltf
    ? { kind: 'gltf', url: `${base}/glTF/${options.gltf}` }
    : { kind: 'glb', url: `${base}/glTF-Binary/${name}.glb` };
  return {
    id: `khronos-${name.replace(/([a-z])([A-Z])/g, '$1-$2').toLowerCase()}`,
    en,
    zh,
    category,
    source,
    license: { kind: 'khronos', url: `${base}/metadata.json` },
    page: `https://github.com/KhronosGroup/glTF-Sample-Assets/tree/${REVISIONS.khronos}/Models/${name}`,
    collection: 'Khronos glTF Sample Assets',
    ...options,
  };
}

function polyhaven(asset, en, zh, category, options = {}) {
  return {
    id: `polyhaven-${asset.toLowerCase().replaceAll('_', '-')}`,
    en,
    zh,
    category,
    source: { kind: 'polyhaven', asset },
    license: { kind: 'polyhaven', asset },
    page: `https://polyhaven.com/a/${asset}`,
    collection: 'Poly Haven',
    ...options,
  };
}

function google(name, en, zh, category, options = {}) {
  const api = `https://fuel.gazebosim.org/1.0/GoogleResearch/models/${name}/1`;
  return {
    id: `google-${(options.slug ?? name).toLowerCase().replaceAll('_', '-')}`,
    en,
    zh,
    category,
    source: { kind: 'fuel', url: `${api}/${name}.zip` },
    license: { kind: 'fuel' },
    page: `https://app.gazebosim.org/GoogleResearch/fuel/models/${name}`,
    collection: 'Google Scanned Objects',
    upAxis: 'z',
    ...options,
  };
}

function nvidia(asset, en, zh, category, options = {}) {
  return {
    id: `nvidia-${asset.replace(/_a0\d$|_b0\d$/, '').replace(/([a-z])([A-Z])/g, '$1-$2').replaceAll('_', '-').toLowerCase()}`,
    en,
    zh,
    category,
    source: {
      kind: 'glb',
      url: `https://huggingface.co/datasets/codepawl/simready-glb/resolve/${REVISIONS.codepawl}/engine/${asset}/${asset}.glb`,
    },
    license: {
      kind: 'nvidia',
      url: `https://huggingface.co/datasets/nvidia/simready-assets/resolve/${REVISIONS.nvidia}/${asset}/com.nvidia.simready.packaging.json`,
    },
    page: `https://huggingface.co/datasets/nvidia/simready-assets/tree/${REVISIONS.nvidia}/${asset}`,
    collection: 'NVIDIA SimReady',
    ...options,
  };
}

export const MODELS = [
  kaykit('adventurers', 'Knight', 'Knight', '骑士', {
    drop: ['1H_Sword_Offhand', 'Badge_Shield', 'Rectangle_Shield', 'Spike_Shield', '2H_Sword'],
    anims: [...ADVENTURER_ANIMS, '1H_Melee_Attack_Chop', 'Block'],
  }),
  kaykit('adventurers', 'Mage', 'Mage', '法师', {
    drop: ['Spellbook', 'Spellbook_open', '1H_Wand'],
    anims: [...ADVENTURER_ANIMS, 'Spellcasting', 'Spellcast_Shoot'],
  }),
  kaykit('adventurers', 'Barbarian', 'Barbarian', '野蛮人', {
    drop: ['1H_Axe_Offhand', 'Barbarian_Round_Shield', '1H_Axe', 'Mug'],
    anims: [...ADVENTURER_ANIMS, '2H_Melee_Idle', '2H_Melee_Attack_Spin', '2H_Melee_Attack_Chop'],
  }),
  kaykit('adventurers', 'Rogue_Hooded', 'Hooded Rogue', '兜帽游侠', {
    drop: ['1H_Crossbow', '2H_Crossbow', 'Throwable'],
    anims: [...ADVENTURER_ANIMS, 'Dualwield_Melee_Attack_Stab', 'Dualwield_Melee_Attack_Slice'],
  }),
  kaykit('skeletons', 'Skeleton_Warrior', 'Skeleton Warrior', '骷髅战士', {
    anims: ['Idle', 'Skeletons_Awaken_Standing', 'Walking_D_Skeletons', 'Running_A', 'Taunt', 'Hit_A', 'Death_C_Skeletons', 'Death_C_Skeletons_Resurrect'],
  }),
  kaykit('skeletons', 'Skeleton_Mage', 'Skeleton Mage', '骷髅法师', {
    anims: ['Idle', 'Skeletons_Awaken_Floor', 'Walking_D_Skeletons', 'Spellcasting', 'Spellcast_Summon', 'Cheer', 'Death_C_Skeletons'],
  }),

  khronos('Fox', 'Fox', '狐狸', 'animals'),
  khronos('BarramundiFish', 'Barramundi', '尖吻鲈', 'animals'),
  polyhaven('street_rat', 'Street Rat', '街头老鼠', 'animals'),
  google('Dino_3', 'Tyrannosaurus', '霸王龙', 'animals', { slug: 'tyrannosaurus' }),
  google('Dino_4', 'Styracosaurus', '戟龙', 'animals', { slug: 'styracosaurus' }),
  google('Dino_5', 'Triceratops', '三角龙', 'animals', { slug: 'triceratops' }),
  google('Weisshai_Great_White_Shark', 'Great White Shark', '大白鲨', 'animals', { slug: 'great-white-shark' }),

  google('Dog', 'Plush Puppy', '毛绒小狗', 'toys', { slug: 'plush-puppy' }),
  google('Elephant', 'Plush Elephant', '毛绒大象', 'toys', { slug: 'plush-elephant' }),
  google('LACING_SHEEP', 'Wooden Sheep', '木头小羊', 'toys', { slug: 'wooden-sheep' }),
  google('BUNNY_RACER', 'Bunny Racer', '兔子赛车', 'toys', { slug: 'bunny-racer' }),
  polyhaven('rubber_duck_toy', 'Rubber Duck', '橡皮鸭', 'toys'),
  khronos('ToyCar', 'Toy Car', '玩具车', 'toys', { drop: ['Fabric'] }),

  polyhaven('sofa_02', 'Velvet Sofa', '丝绒沙发', 'home'),
  polyhaven('ArmChair_01', 'Victorian Armchair', '维多利亚扶手椅', 'home'),
  polyhaven('Rockingchair_01', 'Rocking Chair', '摇椅', 'home'),
  khronos('SheenChair', 'Sheen Chair', '绒面椅', 'home'),
  polyhaven('potted_plant_04', 'Potted Succulent', '多肉盆栽', 'home'),
  khronos('Lantern', 'Street Lantern', '街灯', 'home'),
  khronos('AnisotropyBarnLamp', 'Barn Lamp', '谷仓灯', 'home'),
  polyhaven('tea_set_01', 'Tea Set', '茶具', 'home'),
  polyhaven('vintage_grandfather_clock_01', 'Grandfather Clock', '落地摆钟', 'home'),
  nvidia('lamp_table_bedside_a01', 'Bedside Lamp', '床头台灯', 'home'),
  nvidia('dishware_mug_a01', 'Enamel Mug', '搪瓷杯', 'home'),

  polyhaven('Camera_01', 'Vintage Camera', '复古相机', 'gadgets'),
  polyhaven('Television_01', 'Retro Television', '复古电视', 'gadgets'),
  khronos('BoomBox', 'Boom Box', '手提音响', 'gadgets'),
  polyhaven('cassette_player', 'Cassette Player', '卡带播放器', 'gadgets'),
  polyhaven('alarm_clock_01', 'Alarm Clock', '闹钟', 'gadgets'),
  polyhaven('vintage_pocket_watch', 'Pocket Watch', '怀表', 'gadgets'),
  khronos('FlightHelmet', 'Flight Helmet', '飞行头盔', 'gadgets', { gltf: 'FlightHelmet.gltf' }),
  khronos('WaterBottle', 'Water Bottle', '水壶', 'gadgets'),
  nvidia('engineAssembly_v8_engine_a01', 'V8 Engine', 'V8 发动机', 'gadgets', { maxTriangles: 30000 }),
  nvidia('robotic_system_arm_b01', 'Robotic Arm', '机械臂', 'gadgets'),
  nvidia('fireExtinguisher_abc_a03', 'Fire Extinguisher', '灭火器', 'gadgets'),
  nvidia('powerTool_drill_b01', 'Cordless Drill', '电钻', 'gadgets'),
  nvidia('ppe_helmet_hardHat_a01', 'Hard Hat', '安全帽', 'gadgets'),
  nvidia('equipment_bench_vise_a01', 'Bench Vise', '台虎钳', 'gadgets'),

  khronos('Avocado', 'Avocado', '牛油果', 'food'),
  khronos('MandarinOrange', 'Mandarin Orange', '橘子', 'food', { gltf: 'MandarinOrange.gltf' }),
  polyhaven('food_apple_01', 'Apple', '苹果', 'food'),
  polyhaven('croissant', 'Croissant', '可颂', 'food'),
  polyhaven('lemon', 'Lemon', '柠檬', 'food'),
  polyhaven('food_pomegranate_01', 'Pomegranate', '石榴', 'food'),
  polyhaven('carrot_cake', 'Carrot Cake', '胡萝卜蛋糕', 'food'),

  polyhaven('marble_bust_01', 'Marble Bust', '大理石胸像', 'art'),
  polyhaven('horse_statue_01', 'Horse Statue', '骏马雕像', 'art'),
  polyhaven('concrete_cat_statue', 'Cat Statue', '水泥猫雕像', 'art'),
  polyhaven('carved_wooden_elephant', 'Carved Elephant', '木雕大象', 'art'),
  polyhaven('antique_ceramic_vase_01', 'Ceramic Vase', '古董陶瓶', 'art'),
  polyhaven('garden_gnome', 'Garden Gnome', '花园地精', 'art'),
  polyhaven('chess_set', 'Chess Set', '国际象棋', 'art'),
  polyhaven('treasure_chest', 'Treasure Chest', '宝箱', 'art'),
  polyhaven('Ukulele_01', 'Ukulele', '尤克里里', 'art'),
  khronos('Corset', 'Corset', '紧身胸衣', 'art'),
];
