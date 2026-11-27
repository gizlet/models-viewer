# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.69.1"]
# ///
"""Writes the app's fonts to assets/fonts/.

Downloads Inter, Space Grotesk and Noto Sans SC from google/fonts at a pinned
commit, fixes each variable font at the weight the app uses and keeps only
the characters it can draw: every character in src/ and in the model catalog,
plus printable ASCII and Latin-1. Run it after changing interface strings or
the catalog:

    uv run tools/fonts/subset.py
"""

import hashlib
import io
import json
from pathlib import Path
from urllib.parse import quote
from urllib.request import Request, urlopen

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets/fonts"
LOCK = Path(__file__).with_name("fonts.lock.json")
REVISION = "5026800714d07231937d92eb4cceabf06e05d74e"
FONTS = [
    ("ofl/inter/Inter[opsz,wght].ttf", {"wght": 600, "opsz": 14}, "Inter-SemiBold.ttf", "Inter-OFL.txt", False),
    ("ofl/spacegrotesk/SpaceGrotesk[wght].ttf", {"wght": 700}, "SpaceGrotesk-Bold.ttf", "SpaceGrotesk-OFL.txt", False),
    ("ofl/notosanssc/NotoSansSC[wght].ttf", {"wght": 500}, "NotoSansSC-Medium.ttf", "NotoSansSC-OFL.txt", True),
]
CJK_START = 0x2E80


def fetch(path, lock):
    url = f"https://raw.githubusercontent.com/google/fonts/{REVISION}/{quote(path)}"
    with urlopen(Request(url, headers={"User-Agent": "GizletModelsViewer/0.1"}), timeout=120) as response:
        data = response.read()
    digest = hashlib.sha256(data).hexdigest()
    if lock.get(path, digest) != digest:
        raise SystemExit(f"{path} changed: {digest} (locked {lock[path]})")
    lock[path] = digest
    return data


def characters():
    chars = {chr(c) for c in range(0x20, 0x7F)} | {chr(c) for c in range(0xA0, 0x100)}
    for path in sorted((ROOT / "src").rglob("*.rs")):
        chars |= set(path.read_text(encoding="utf-8"))
    chars |= set((ROOT / "assets/models/catalog.json").read_text(encoding="utf-8"))
    return {c for c in chars if c.isprintable() or c == " "}


def main():
    lock = json.loads(LOCK.read_text()) if LOCK.exists() else {}
    chars = characters()
    OUT.mkdir(parents=True, exist_ok=True)
    for path, axes, name, license_name, cjk in FONTS:
        font = TTFont(io.BytesIO(fetch(path, lock)))
        font = instancer.instantiateVariableFont(font, axes)
        wanted = {c for c in chars if (ord(c) >= CJK_START) == cjk}
        if cjk:
            # Noto also draws the symbols the Latin faces lack.
            wanted |= {c for c in chars if ord(c) >= 0x2000}
        options = subset.Options()
        options.layout_features = []
        options.hinting = False
        options.name_IDs = ["*"]
        options.name_languages = ["*"]
        options.notdef_outline = True
        subsetter = subset.Subsetter(options)
        subsetter.populate(unicodes=[ord(c) for c in wanted])
        subsetter.subset(font)
        font.save(OUT / name)
        license_path = path.rsplit("/", 1)[0] + "/OFL.txt"
        (OUT / license_name).write_bytes(fetch(license_path, lock))
        print(f"{name}: {len(font.getBestCmap())} characters, {(OUT / name).stat().st_size // 1024} KB")
    LOCK.write_text(json.dumps(dict(sorted(lock.items())), indent=2) + "\n")


if __name__ == "__main__":
    main()
