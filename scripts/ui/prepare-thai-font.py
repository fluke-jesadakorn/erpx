"""Build browser subsets from the unchanged PDF font.

Use fonttools==4.63.0 and brotli==1.2.0; see public/fonts/OFL.txt.
The non-Thai subset loads only when content explicitly uses Noto Sans Thai for
another script, including previously saved rich text with Latin characters.
"""
from importlib.metadata import version
from pathlib import Path
from fontTools import subset
from fontTools.ttLib import TTFont

for package, pinned in [("fonttools", "4.63.0"), ("brotli", "1.2.0")]:
    if version(package) != pinned:
        raise RuntimeError("Install the pinned packages in scripts/ui/font-requirements.txt")

root = Path(__file__).resolve().parents[2]
source = root / "public/fonts/NotoSansThai.ttf"
cmap = TTFont(source).getBestCmap()
thai = set(range(0x0E00, 0x0E80)) | {0x200C, 0x200D, 0x25CC}
for filename, unicodes in [("NotoSansThai.woff2", thai),
                          ("NotoSansThaiOther.woff2", set(cmap) - thai)]:
    font = TTFont(source, recalcTimestamp=False)
    options = subset.Options()
    options.flavor = "woff2"
    options.layout_features = ["*"]
    worker = subset.Subsetter(options=options)
    worker.populate(unicodes=unicodes)
    worker.subset(font)
    font.flavor = "woff2"
    destination = root / "public/fonts" / filename
    font.save(destination)
    result = TTFont(destination).getBestCmap()
    assert all(codepoint in result for codepoint in set(cmap) & unicodes)
    print(f"{filename}: {destination.stat().st_size} bytes; glyph coverage preserved")
