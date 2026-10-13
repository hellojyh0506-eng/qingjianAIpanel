#!/usr/bin/env python3
"""从 lucide-static 的 icon-nodes.json 生成 crates/qingjian-render/src/panel/icon/data.rs。

用法：
    npm pack lucide-static && tar xzf lucide-static-*.tgz   # 解出 package/
    python3 tools/lucide-icons/gen.py <package 目录>

circle / rect / line / polyline / polygon / ellipse 都转成 path 的 d 字符串，运行时只需要一个 path 解析器。
要加图标：改下面的 ICONS，重新生成，提交生成结果。Lucide 为 ISC 许可，见 crates/qingjian-render/src/panel/icon/LICENSE-lucide。
"""

import json
import subprocess
import sys
from pathlib import Path

ICONS = """
pin minus x volume-2 copy star mic message-square sparkles file-text scan-text target lightbulb
chevron-down chevron-right chevron-up plus settings paperclip image send refresh-cw circle-alert
loader-circle circle-check ellipsis book-plus message-circle check trash clock arrow-up pencil
""".split()

OUT = Path(__file__).resolve().parents[2] / "crates/qingjian-render/src/panel/icon/data.rs"


def num(value):
    text = f"{float(value):.4f}".rstrip("0").rstrip(".")
    return text if text not in ("", "-0") else "0"


def to_path(tag, a):
    if tag == "path":
        return a["d"]
    if tag == "circle":
        cx, cy, r = float(a["cx"]), float(a["cy"]), float(a["r"])
        return (f"M{num(cx - r)} {num(cy)}a{num(r)} {num(r)} 0 1 0 {num(2 * r)} 0"
                f"a{num(r)} {num(r)} 0 1 0 {num(-2 * r)} 0z")
    if tag == "ellipse":
        cx, cy, rx, ry = (float(a[k]) for k in ("cx", "cy", "rx", "ry"))
        return (f"M{num(cx - rx)} {num(cy)}a{num(rx)} {num(ry)} 0 1 0 {num(2 * rx)} 0"
                f"a{num(rx)} {num(ry)} 0 1 0 {num(-2 * rx)} 0z")
    if tag == "line":
        return f"M{a['x1']} {a['y1']}L{a['x2']} {a['y2']}"
    if tag in ("polyline", "polygon"):
        pts = a["points"].replace(",", " ").split()
        pairs = [f"{pts[i]} {pts[i + 1]}" for i in range(0, len(pts), 2)]
        return "M" + "L".join(pairs) + ("z" if tag == "polygon" else "")
    if tag == "rect":
        x, y, w, h = (float(a[k]) for k in ("x", "y", "width", "height"))
        r = min(float(a.get("rx", a.get("ry", 0))), w / 2, h / 2)
        if r <= 0:
            return f"M{num(x)} {num(y)}h{num(w)}v{num(h)}h{num(-w)}z"
        return (f"M{num(x + r)} {num(y)}h{num(w - 2 * r)}a{num(r)} {num(r)} 0 0 1 {num(r)} {num(r)}"
                f"v{num(h - 2 * r)}a{num(r)} {num(r)} 0 0 1 {num(-r)} {num(r)}"
                f"h{num(-(w - 2 * r))}a{num(r)} {num(r)} 0 0 1 {num(-r)} {num(-r)}"
                f"v{num(-(h - 2 * r))}a{num(r)} {num(r)} 0 0 1 {num(r)} {num(-r)}z")
    raise SystemExit(f"不认识的标签 {tag}")


def camel(name):
    return "".join(part.capitalize() for part in name.split("-"))


def main():
    nodes = json.loads((Path(sys.argv[1]) / "icon-nodes.json").read_text())
    missing = [n for n in ICONS if n not in nodes]
    if missing:
        raise SystemExit(f"lucide-static 里没有：{missing}")
    names = sorted(ICONS)
    out = ['''//! Lucide 图标（ISC 许可，见同目录 LICENSE-lucide）：24 × 24 视口、描边风格的 path 数据。
//!
//! 本文件由 `tools/lucide-icons/gen.py` 生成，不要手改；要加图标改脚本里的清单后重新生成。

/// 面板用到的图标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {''']
    for n in names:
        out.append(f"    {camel(n)},")
    out.append("}\n\nimpl Icon {\n    /// 全部图标，测试里逐个验证能解析。\n    pub const ALL: &'static [Icon] = &[")
    for n in names:
        out.append(f"        Icon::{camel(n)},")
    out.append("    ];\n\n    /// Lucide 里的图标名。\n    pub fn name(self) -> &'static str {\n        match self {")
    for n in names:
        out.append(f'            Icon::{camel(n)} => "{n}",')
    out.append("        }\n    }\n\n    /// 组成这个图标的 path `d` 字符串，24 × 24 坐标系。\n    pub(super) fn paths(self) -> &'static [&'static str] {\n        match self {")
    for n in names:
        ds = [to_path(t, a) for t, a in nodes[n]]
        body = ",\n".join(f'                "{d}"' for d in ds)
        out.append(f"            Icon::{camel(n)} => &[\n{body},\n            ],")
    out.append("        }\n    }\n}")
    OUT.write_text("\n".join(out) + "\n")
    # 生成物要与 rustfmt 的输出一致，否则提交钩子会报格式差异
    subprocess.run(["rustfmt", "--edition", "2024", str(OUT)], check=True)
    print(f"写入 {OUT}（{len(names)} 个图标）")


if __name__ == "__main__":
    main()
