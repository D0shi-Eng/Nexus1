#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Read-only audit of apps/desktop/src/styles/global.css (path resolved from this script).

Prints the census that drives the token extraction:
  * which custom properties are consumed but never defined (and whether a
    fallback saves the declaration),
  * the raw hex/rgba palette so tokens reuse existing values,
  * physical-property (RTL-unsafe) usage counts.
"""
from __future__ import annotations

import pathlib
import re
from collections import Counter

CSS = (
    pathlib.Path(__file__).resolve().parents[3]
    / "apps"
    / "desktop"
    / "src"
    / "styles"
    / "global.css"
)


def main() -> int:
    src = CSS.read_text(encoding="utf-8")
    print(f"file={CSS} bytes={CSS.stat().st_size} lines={src.count(chr(10)) + 1}")

    used: dict[str, dict[str, int]] = {}
    for m in re.finditer(r"var\(\s*(--[a-zA-Z0-9-]+)\s*(,)?", src):
        rec = used.setdefault(m.group(1), {"n": 0, "fallback": 0})
        rec["n"] += 1
        if m.group(2):
            rec["fallback"] += 1
    defined = set(re.findall(r"(--[a-zA-Z0-9-]+)\s*:", src))

    print("\n== custom property consumption ==")
    print(f"{'token':24}{'uses':>6}{'w/fallback':>12}  defined")
    broken: list[str] = []
    for name, rec in sorted(used.items()):
        is_def = name in defined
        print(f"{name:24}{rec['n']:>6}{rec['fallback']:>12}  {'yes' if is_def else 'NO'}")
        if not is_def and rec["fallback"] == 0:
            broken.append(name)
    print("\nUNDEFINED + NO FALLBACK (invalid at computed-value time):", ", ".join(broken) or "none")
    print("count:", len(broken))

    print("\n== hex palette census ==")
    hexes = [h.lower() for h in re.findall(r"#[0-9a-fA-F]{3,8}\b", src)]
    print("distinct:", len(set(hexes)), "occurrences:", len(hexes))
    for colour, n in Counter(hexes).most_common():
        print(f"  {colour:9} x{n}")

    print("\n== rgba() census ==")
    rgbas = re.findall(r"rgba?\([^)]*\)", src)
    for value, n in Counter(rgbas).most_common():
        print(f"  {value:44} x{n}")

    print("\n== rem/px font-size census ==")
    sizes = Counter(re.findall(r"font-size:\s*([^;}\n]+)", src))
    for value, n in sorted(sizes.items(), key=lambda kv: -kv[1]):
        print(f"  {value.strip():44} x{n}")

    print("\n== border-radius census ==")
    radii = Counter(re.findall(r"border-radius:\s*([^;}\n]+)", src))
    for value, n in sorted(radii.items(), key=lambda kv: -kv[1]):
        print(f"  {value.strip():44} x{n}")

    print("\n== physical vs logical direction properties ==")
    physical = {
        "padding-left": len(re.findall(r"padding-left\s*:", src)),
        "padding-right": len(re.findall(r"padding-right\s*:", src)),
        "margin-left": len(re.findall(r"margin-left\s*:", src)),
        "margin-right": len(re.findall(r"margin-right\s*:", src)),
        "left:": len(re.findall(r"(?<![a-z-])left\s*:", src)),
        "right:": len(re.findall(r"(?<![a-z-])right\s*:", src)),
        "text-align: right/left": len(re.findall(r"text-align:\s*(?:right|left)", src)),
        "border-left": len(re.findall(r"border-left", src)),
        "border-right": len(re.findall(r"border-right", src)),
    }
    logical = {
        "margin-inline-start": len(re.findall(r"margin-inline-start", src)),
        "margin-inline-end": len(re.findall(r"margin-inline-end", src)),
        "padding-inline": len(re.findall(r"padding-inline", src)),
        "inset-inline": len(re.findall(r"inset-inline", src)),
        "inset-block": len(re.findall(r"inset-block", src)),
        "border-inline": len(re.findall(r"border-inline", src)),
        "border-block": len(re.findall(r"border-block", src)),
        "text-align: start/end": len(re.findall(r"text-align:\s*(?:start|end)", src)),
        "padding-block": len(re.findall(r"padding-block", src)),
        "margin-block": len(re.findall(r"margin-block", src)),
    }
    for k, v in physical.items():
        print(f"  PHYSICAL {k:26} {v}")
    for k, v in logical.items():
        print(f"  logical  {k:26} {v}")

    print("\n== transition/animation census ==")
    for value, n in Counter(re.findall(r"transition:\s*([^;}\n]+)", src)).most_common():
        print(f"  {value.strip():50} x{n}")

    print("\n== box-shadow census ==")
    for value, n in Counter(re.findall(r"box-shadow:\s*([^;}\n]+?)\s*;", src)).most_common():
        print(f"  {value.strip():56} x{n}")

    print("\n== min-height (control height) census ==")
    for value, n in Counter(re.findall(r"min-height:\s*([^;}\n]+)", src)).most_common():
        print(f"  {value.strip():20} x{n}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
