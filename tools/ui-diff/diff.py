#!/usr/bin/env python3
"""把离线预览图与《严格参考图》的对应裁图缩放到同宽，输出并排图与 50% 叠加图。

用法：
    python3 tools/ui-diff/diff.py <预览图.png> <参考图.png> [--out 目录] [--crop x0,y0,x1,y1] [--width 840]

--crop 是参考图上要比的那一块（参考图往往一张里有好几个面板），按参考图原始像素给；不给就整张比。
参考图是 AI 生成的样稿，字形不可能逐像素一致；验收按布局 ±2 px（以 420 pt 宽的窗口为基准）、颜色 token、元素齐全，
所以输出的并排图用来肉眼量位置，叠加图用来看错位与色差，不做像素差统计。
"""

import argparse
from pathlib import Path

from PIL import Image


def fit_width(image, width):
    height = round(image.height * width / image.width)
    return image.resize((width, height), Image.LANCZOS)


def flatten(image):
    """透明处垫白，叠加时才不会出现黑块。"""
    background = Image.new("RGBA", image.size, (255, 255, 255, 255))
    background.alpha_composite(image.convert("RGBA"))
    return background


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    parser.add_argument("preview", type=Path)
    parser.add_argument("reference", type=Path)
    parser.add_argument("--out", type=Path, default=Path("target/ui-diff"))
    parser.add_argument("--crop", help="参考图上的裁剪框 x0,y0,x1,y1")
    parser.add_argument("--width", type=int, default=840, help="两张图缩放到的统一宽度（像素）")
    args = parser.parse_args()

    preview = flatten(Image.open(args.preview))
    reference = flatten(Image.open(args.reference))
    if args.crop:
        x0, y0, x1, y1 = (int(v) for v in args.crop.split(","))
        reference = reference.crop((x0, y0, x1, y1))
    preview = fit_width(preview, args.width)
    reference = fit_width(reference, args.width)

    height = max(preview.height, reference.height)
    gap = 24
    side = Image.new("RGBA", (args.width * 2 + gap, height), (255, 255, 255, 255))
    side.paste(reference, (0, 0))
    side.paste(preview, (args.width + gap, 0))

    overlay_base = Image.new("RGBA", (args.width, height), (255, 255, 255, 255))
    overlay_base.paste(reference, (0, 0))
    top = Image.new("RGBA", (args.width, height), (255, 255, 255, 255))
    top.paste(preview, (0, 0))
    overlay = Image.blend(overlay_base, top, 0.5)

    args.out.mkdir(parents=True, exist_ok=True)
    stem = args.preview.stem
    side_path = args.out / f"{stem}-side-by-side.png"
    overlay_path = args.out / f"{stem}-overlay.png"
    side.convert("RGB").save(side_path)
    overlay.convert("RGB").save(overlay_path)
    print(f"并排图 {side_path}\n叠加图 {overlay_path}")
    print(f"参考 {reference.width}×{reference.height}，预览 {preview.width}×{preview.height}（同宽 {args.width}）")


if __name__ == "__main__":
    main()
