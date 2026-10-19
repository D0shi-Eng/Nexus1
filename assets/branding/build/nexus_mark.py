#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""NEXUS identity build - single source of truth for the NEXUS application mark.

Reproducible command (no network, no third-party raster, Pillow + numpy only):

    <PYTHON_INTERPRETER> <REPO_ROOT>\\assets\\branding\\build\\nexus_mark.py

The same procedural geometry drives BOTH the master SVG and every raster, so the
vector and the bitmaps cannot drift apart. Geometry is authored in a 256x256
design space and rendered with supersampled coverage masks, then downsampled in
premultiplied alpha (no dark fringe on the transparent edge).

Outputs
-------
assets/branding/nexus-icon.svg
assets/branding/png/nexus-icon-{16,24,32,48,64,128,256,512}.png
assets/branding/nexus-icon.ico                      multi-size 16..256
apps/desktop/src-tauri/icons/icon.ico               identical bytes to the above
apps/desktop/src-tauri/icons/{32x32,128x128,128x128@2x,icon}.png
apps/desktop/src-tauri/icons/Square*Logo.png, StoreLogo.png
assets/branding/preview/contact-{light,dark}.png
assets/branding/preview/legibility-<size>-<bg>.png
assets/branding/PREVIEW.md                          contact sheet + SHA-256
assets/branding/build-report.txt                    full manifest + metrics

The build is byte-deterministic: no timestamps are embedded anywhere.
"""

from __future__ import annotations

import hashlib
import math
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

# --------------------------------------------------------------------------
# paths
# --------------------------------------------------------------------------
ROOT = Path(__file__).resolve().parents[3]          # repo root (derived, never hardcoded)
BRAND = ROOT / "assets" / "branding"
PNG_DIR = BRAND / "png"
PREVIEW_DIR = BRAND / "preview"
TAURI_ICONS = ROOT / "apps" / "desktop" / "src-tauri" / "icons"

# --------------------------------------------------------------------------
# design parameters (256x256 design space)
# --------------------------------------------------------------------------
CANVAS = 256.0
CX = CY = 128.0

#: hexagonal governed container - "hexagonal architecture" / bounded cell
HEX_R = 112.0
HEX_CORNER = 24.0
#: the highlight band is specified in DEVICE pixels, not design units: a deep
#: navy plate cannot separate from the dark taskbar on its own, and a sub-pixel
#: band downscales into a broken dashed hairline. Every raster therefore
#: re-derives the band as ~1.4 px. This is the only size-variant geometry.
RIM_BAND_PX = 1.4
RIM_ALPHA = 0.90

#: the tri-link circuit: three connected nodes closed into one unbroken ring
RING_R = 76.0
RING_CORNER = 24.0
RING_W = 26.0
NODE_R = 23.0

#: palette reused from the running application (no invented brand colours)
NAVY_TOP = (41, 69, 104)      # == .navigation__item--active background #294568
NAVY_BOTTOM = (16, 29, 48)    # == .sidebar background #101d30
RIM_RGB = (168, 200, 232)     # highlight band; keeps the silhouette on dark
GOLD_TOP = (240, 197, 111)    # == .brand__mark colour #f0c56f
GOLD_BOTTOM = (224, 173, 78)  # == .navigation__item--active indicator #e4b65a

PNG_SIZES = (16, 24, 32, 48, 64, 128, 256, 512)
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
#: Tauri's standard Windows bundle assets, so any bundle.icon list resolves
SQUARE_LOGOS = {
    "Square30x30Logo.png": 30,
    "Square44x44Logo.png": 44,
    "Square71x71Logo.png": 71,
    "Square89x89Logo.png": 89,
    "Square107x107Logo.png": 107,
    "Square142x142Logo.png": 142,
    "Square150x150Logo.png": 150,
    "Square284x284Logo.png": 284,
    "Square310x310Logo.png": 310,
    "StoreLogo.png": 50,
}
TAURI_PNG = {
    "32x32.png": 32,
    "128x128.png": 128,
    "128x128@2x.png": 256,
    "icon.png": 512,
}

LIGHT_BG = (255, 255, 255)
DARK_BG = (31, 31, 31)        # Windows 11 default dark taskbar
VOID_BG = (0, 0, 0)           # harshest case
SHEET_SIZE = (16, 24, 32, 48, 64, 128, 256)

FONT_REGULAR = Path(r"C:\Windows\Fonts\segoeui.ttf")
FONT_BOLD = Path(r"C:\Windows\Fonts\segoeuib.ttf")


# ==========================================================================
# geometry
# ==========================================================================
def _norm(v):
    m = math.hypot(v[0], v[1])
    return (v[0] / m, v[1] / m) if m else (0.0, 0.0)


def _sub(a, b):
    return (a[0] - b[0], a[1] - b[1])


def _add(a, b):
    return (a[0] + b[0], a[1] + b[1])


def _mul(a, k):
    return (a[0] * k, a[1] * k)


def polygon_vertices(n, radius, rotation_deg, center=(CX, CY)):
    return [
        (
            center[0] + radius * math.cos(math.radians(rotation_deg + i * 360.0 / n)),
            center[1] + radius * math.sin(math.radians(rotation_deg + i * 360.0 / n)),
        )
        for i in range(n)
    ]


def _outward_normal(a, b, inside):
    """Unit normal of edge a->b pointing away from `inside`."""
    d = _norm(_sub(b, a))
    n = (-d[1], d[0])
    mid = ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0)
    if n[0] * (mid[0] - inside[0]) + n[1] * (mid[1] - inside[1]) < 0:
        n = (-n[0], -n[1])
    return n


def rounded_polygon(vertices, radius):
    """Corner spec for a polygon whose corners are circular arcs of `radius`.

    Each entry stores the arc centre, radius and the outward edge normals, so an
    offset outline at distance d is simply the same arc centres with radius
    r + d and the same tangent angles. That is exactly how SVG's round join
    strokes a path, which keeps the vector and the raster identical.
    """
    n = len(vertices)
    inside = (
        sum(v[0] for v in vertices) / n,
        sum(v[1] for v in vertices) / n,
    )
    spec = []
    for i, v in enumerate(vertices):
        prev = vertices[(i - 1) % n]
        nxt = vertices[(i + 1) % n]
        n_in = _outward_normal(prev, v, inside)
        n_out = _outward_normal(v, nxt, inside)
        u_in = _norm(_sub(v, prev))
        out_dir = _norm(_sub(nxt, v))
        cosang = max(-1.0, min(1.0, (-u_in[0]) * out_dir[0] + (-u_in[1]) * out_dir[1]))
        theta = math.acos(cosang)
        half = theta / 2.0
        t = radius / math.tan(half)
        bis = _norm(_add((-u_in[0], -u_in[1]), out_dir))
        center = _add(v, _mul(bis, radius / math.sin(half)))
        spec.append(
            {
                "vertex": v,
                "center": center,
                "r": radius,
                "n_in": n_in,
                "n_out": n_out,
                "t": t,
                "theta": theta,
                "edge": math.dist(v, nxt),
                "ray": _norm(_sub(v, inside)),
            }
        )
    return spec


def outline_points(spec, d, steps_per_rad=64):
    """Polygon point list for the boundary offset outward by d."""
    pts = []
    for s in spec:
        rr = s["r"] + d
        if rr <= 1e-6:
            rr = 1e-6
        a0 = math.atan2(s["n_in"][1], s["n_in"][0])
        a1 = math.atan2(s["n_out"][1], s["n_out"][0])
        da = (a1 - a0 + math.pi) % (2 * math.pi) - math.pi
        steps = max(3, int(abs(da) * steps_per_rad) + 1)
        for k in range(steps + 1):
            a = a0 + da * k / steps
            pts.append((s["center"][0] + rr * math.cos(a), s["center"][1] + rr * math.sin(a)))
    return pts


def _fmt(v):
    s = f"{v:.3f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def svg_subpath(spec, d):
    """Exact SVG path for the same boundary - straight edges + true arcs."""
    parts = []
    for i, s in enumerate(spec):
        rr = s["r"] + d
        a0 = math.atan2(s["n_in"][1], s["n_in"][0])
        a1 = math.atan2(s["n_out"][1], s["n_out"][0])
        da = (a1 - a0 + math.pi) % (2 * math.pi) - math.pi
        sx = s["center"][0] + rr * s["n_in"][0]
        sy = s["center"][1] + rr * s["n_in"][1]
        ex = s["center"][0] + rr * s["n_out"][0]
        ey = s["center"][1] + rr * s["n_out"][1]
        parts.append(("M" if i == 0 else "L") + f"{_fmt(sx)} {_fmt(sy)}")
        parts.append(f"A{_fmt(rr)} {_fmt(rr)} 0 0 {1 if da > 0 else 0} {_fmt(ex)} {_fmt(ey)}")
    parts.append("Z")
    return "".join(parts)


HEX_VERTS = polygon_vertices(6, HEX_R, -90.0)TRI_VERTS = polygon_vertices(3, RING_R, -90.0)
HEX_SPEC = rounded_polygon(HEX_VERTS, HEX_CORNER)
TRI_SPEC = rounded_polygon(TRI_VERTS, RING_CORNER)

#: centre of each gold node: on the ring centreline, along each vertex ray
NODE_CENTERS = [
    _add((CX, CY), _mul(s["ray"], RING_R - RING_CORNER)) for s in TRI_SPEC
]


def geometry_report():
    """Analytic legibility numbers in design units and at each output size."""
    rows = []
    hex_h = 2 * HEX_R * math.cos(math.radians(30))
    node_outer = (RING_R - RING_CORNER) + NODE_R
    for px in sorted(set(PNG_SIZES) | set(ICO_SIZES)):
        k = px / CANVAS
        rows.append(
            {
                "px": px,
                "hex_w": hex_h * k,
                "hex_h": 2 * HEX_R * k,
                "ring_stroke": RING_W * k,
                "node_dia": 2 * NODE_R * k,
                "node_outer_dia": 2 * node_outer * k,
                "ring_outer_dia": 2 * (RING_R - 2 * RING_CORNER + RING_CORNER + RING_W / 2) * k,
                "rim_band": RIM_INSET * k,
            }
        )
    return rows


# ==========================================================================
# rasterisation
# ==========================================================================
def _gradient(size, c0, c1, y0, y1):
    """Vertical linear gradient, drawn in design space then scaled."""
    yy = np.arange(size, dtype=np.float64)
    t = np.clip((yy - y0) / max(1e-9, (y1 - y0)), 0.0, 1.0)[:, None]
    rgb = np.array(c0, dtype=np.float64)[None, :] * (1 - t) + np.array(c1, dtype=np.float64)[None, :] * t
    arr = np.repeat(rgb[:, None, :], size, axis=1).astype(np.uint8)
    return Image.fromarray(arr, "RGB")


def render_mark(px, ss=None):
    """Render the NEXUS mark at `px` device pixels with `ss`x supersampling."""
    if ss is None:
        ss = 8 if px <= 64 else 4
    size = int(round(px * ss))
    k = size / CANVAS

    # design-space coordinates are multiplied by k after outline generation, so
    # the offset distance d stays in design units for every output size
    def mask_of(spec, d):
        m = Image.new("L", (size, size), 0)
        ImageDraw.Draw(m).polygon([(x * k, y * k) for x, y in outline_points(spec, d)], fill=255)
        return m

    def ring_of(spec, outer, inner):
        mo = mask_of(spec, outer)
        mi = mask_of(spec, inner)
        arr = np.clip(
            np.asarray(mo, dtype=np.int16) - np.asarray(mi, dtype=np.int16), 0, 255
        ).astype(np.uint8)
        return Image.fromarray(arr, "L")

    # --- masks -----------------------------------------------------------
    plate_mask = mask_of(HEX_SPEC, 0.0)
    rim_mask = ring_of(HEX_SPEC, 0.0, -RIM_INSET)

    ring_mask = ring_of(TRI_SPEC, RING_W / 2.0, -RING_W / 2.0)
    node_mask = Image.new("L", (size, size), 0)
    nd = ImageDraw.Draw(node_mask)
    for c in NODE_CENTERS:
        nd.ellipse(
            [
                (c[0] - NODE_R) * k,
                (c[1] - NODE_R) * k,
                (c[0] + NODE_R) * k,
                (c[1] + NODE_R) * k,
            ],
            fill=255,
        )
    gold_mask = Image.fromarray(
        np.maximum(np.asarray(ring_mask, dtype=np.uint8), np.asarray(node_mask, dtype=np.uint8)),
        "L",
    )

    # --- colour layers ---------------------------------------------------
    y_hex0 = (CY - HEX_R) * k
    y_hex1 = (CY + HEX_R) * k
    plate = _gradient(size, NAVY_TOP, NAVY_BOTTOM, y_hex0, y_hex1)

    y_g0 = (CY - (RING_R - 2 * RING_CORNER + RING_CORNER + RING_W / 2)) * k
    y_g1 = (CY + (RING_R - 2 * RING_CORNER + RING_CORNER + RING_W / 2)) * k
    gold = _gradient(size, GOLD_TOP, GOLD_BOTTOM, y_g0, y_g1)

    rim = Image.new("RGB", (size, size), RIM_RGB)
    rim_alpha = rim_mask.point(lambda v: int(round(v * RIM_ALPHA)))

    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    out.paste(plate, (0, 0), plate_mask)
    out.paste(rim, (0, 0), rim_alpha)
    out.paste(gold, (0, 0), gold_mask)

    return downscale_rgba(out, (px, px))


def downscale_rgba(im, size):
    """LANCZOS downscale in premultiplied alpha - prevents a dark fringe."""
    if im.size == size:
        return im
    arr = np.asarray(im, dtype=np.float64)
    a = arr[..., 3:4] / 255.0
    pre = arr.copy()
    pre[..., :3] *= a
    small = np.asarray(
        Image.fromarray(np.clip(pre, 0, 255).astype(np.uint8), "RGBA").resize(
            size, Image.Resampling.LANCZOS
        ),
        dtype=np.float64,
    )
    a2 = small[..., 3:4] / 255.0
    rgb = np.divide(small[..., :3], a2, out=np.zeros_like(small[..., :3]), where=a2 > 1e-6)
    out = np.concatenate([np.clip(rgb, 0, 255), small[..., 3:4]], axis=-1)
    out[small[..., 3] < 0.5] = 0
    return Image.fromarray(np.clip(out, 0, 255).astype(np.uint8), "RGBA")


# ==========================================================================
# SVG master (same geometry as the rasters)
# ==========================================================================
def build_svg():
    hex_outer = svg_subpath(HEX_SPEC, 0.0)
    hex_inner = svg_subpath(HEX_SPEC, -RIM_INSET)
    tri_outer = svg_subpath(TRI_SPEC, RING_W / 2.0)
    tri_inner = svg_subpath(TRI_SPEC, -RING_W / 2.0)

    def rgb(c):
        return "#%02X%02X%02X" % c

    nodes = "\n".join(
        '    <circle cx="%s" cy="%s" r="%s"/>' % (_fmt(c[0]), _fmt(c[1]), _fmt(NODE_R))
        for c in NODE_CENTERS
    )
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!--
  NEXUS application mark - original geometry, hand-specified in a 256x256 space.
  Concept: a hexagonal governed container (bounded, auditable cell) holding one
  unbroken ring that links three nodes (connected intelligence under one
  circuit) around a deliberately empty core where human approval sits.
  No third-party mark is copied, traced or approximated; no raster is embedded.
  Generated deterministically by assets/branding/build/nexus_mark.py so that the
  vector and every approved raster share one geometry source.
-->
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256"
     role="img" aria-label="NEXUS">
  <defs>
    <linearGradient id="nxPlate" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{rgb(NAVY_TOP)}"/>
      <stop offset="1" stop-color="{rgb(NAVY_BOTTOM)}"/>
    </linearGradient>
    <linearGradient id="nxGold" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{rgb(GOLD_TOP)}"/>
      <stop offset="1" stop-color="{rgb(GOLD_BOTTOM)}"/>
    </linearGradient>
  </defs>
  <path d="{hex_outer}" fill="url(#nxPlate)"/>
  <path d="{hex_outer} {hex_inner}" fill="{rgb(RIM_RGB)}" fill-opacity="{RIM_ALPHA:.2f}" fill-rule="evenodd"/>
  <g fill="url(#nxGold)">
    <path d="{tri_outer} {tri_inner}" fill-rule="evenodd"/>
{nodes}
  </g>
</svg>
"""


# ==========================================================================
# ICO container (written by hand so the embedded pixels are exactly ours)
# ==========================================================================
def _bmp_payload(img):
    w, h = img.size
    px = img.load()
    xor = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            r, g, b, a = px[x, y]
            xor += bytes((b, g, r, a))
    stride = ((w + 31) // 32) * 4
    and_mask = bytearray()
    for y in range(h - 1, -1, -1):
        row = bytearray(stride)
        for x in range(w):
            if px[x, y][3] < 128:
                row[x >> 3] |= 0x80 >> (x & 7)
        and_mask += row
    header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, len(xor) + len(and_mask), 0, 0, 0, 0)
    return bytes(header + xor + and_mask)


def _png_bytes(img):
    import io

    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def build_ico(images, path):
    payloads = []
    for img in images:
        w = img.size[0]
        payloads.append(_png_bytes(img) if w >= 256 else _bmp_payload(img))
    count = len(images)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    entries, body = b"", b""
    for img, payload in zip(images, payloads):
        w, h = img.size
        entries += struct.pack(
            "<BBBBHHII", 0 if w >= 256 else w, 0 if h >= 256 else h, 0, 0, 1, 32, len(payload), offset
        )
        offset += len(payload)
        body += payload
    path.write_bytes(header + entries + body)


def read_ico_frames(path):
    """Re-read the ICO and return {size: (rgba bytes hash, opaque pixel count)}."""
    with Image.open(path) as im:
        raw = im.info.get("sizes", set())
        sizes = sorted(int(s[0]) if isinstance(s, (tuple, list)) else int(s) for s in raw)
        out = {}
        for s in sizes:
            im.size = (s, s) if isinstance(im.size, tuple) and len(im.size) == 2 else s
            frame = im.convert("RGBA")
            arr = np.asarray(frame)
            out[int(frame.size[0])] = (
                hashlib.sha256(arr.tobytes()).hexdigest(),
                int((arr[..., 3] > 0).sum()),
            )
    return out


# ==========================================================================
# measurement
# ==========================================================================
def _srgb_lin(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def rel_lum(rgb):
    r, g, b = (_srgb_lin(float(v)) for v in rgb[:3])
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(fg, bg):
    l1, l2 = rel_lum(fg), rel_lum(bg)
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


def flatten(img, bg):
    layer = Image.new("RGBA", img.size, tuple(bg) + (255,))
    return Image.alpha_composite(layer, img).convert("RGB")


def _is_gold(rgb):
    r, g, b = rgb
    return r > 150 and (int(r) - int(b)) > 40 and g > 110


def measure(img, bg):
    """Contrast + structural metrics for one rendered size on one background."""
    flat = np.asarray(flatten(img, bg), dtype=np.int16)
    rgba = np.asarray(img, dtype=np.uint8)
    n = flat.shape[0]

    mark = np.abs(flat - np.array(bg, dtype=np.int16)).sum(axis=2) > 12
    plate = mark & (~np.array([[_is_gold(tuple(p)) for p in row] for row in flat]))
    gold = np.array([[_is_gold(tuple(p)) for p in row] for row in flat]) & mark

    # boundary ring of the mark: mark pixels adjacent to a non-mark pixel
    er = mark.copy()
    er[1:, :] &= mark[:-1, :]
    er[:-1, :] &= mark[1:, :]
    er[:, 1:] &= mark[:, :-1]
    er[:, :-1] &= mark[:, 1:]
    edge = mark & ~er

    def med(sel):
        if not sel.any():
            return None
        return tuple(int(v) for v in np.median(flat[sel], axis=0))

    plate_c = med(plate)
    gold_c = med(gold)
    edge_c = med(edge)

    m = {
        "size": n,
        "mark_px": int(mark.sum()),
        "mark_ratio": round(float(mark.sum()) / (n * n), 4),
        "gold_px": int(gold.sum()),
        "edge_vs_bg": round(contrast(edge_c, bg), 2) if edge_c else None,
        "plate_vs_bg": round(contrast(plate_c, bg), 2) if plate_c else None,
        "gold_vs_plate": round(contrast(gold_c, plate_c), 2) if (gold_c and plate_c) else None,
        "min_alpha": int(rgba[..., 3].min()),
        "max_alpha": int(rgba[..., 3].max()),
    }

    # structural test: is the gold circuit a CLOSED loop, i.e. is the core
    # aperture sealed off from the outside? Flood fill the non-gold region from
    # the centre; if the fill reaches the transparent outside, the ring broke.
    if n <= 64:
        sealed, aperture_px = _aperture_sealed(flat, gold, bg, n)
        m["aperture_sealed"] = sealed
        m["aperture_px"] = aperture_px
    return m


def _aperture_sealed(flat, gold, bg, n):
    free = ~gold
    cy = cx = n // 2
    if gold[cy, cx]:
        # centre sits on glyph ink; step to the nearest free pixel
        found = None
        for r in range(1, n):
            for dy in range(-r, r + 1):
                for dx in range(-r, r + 1):
                    y, x = cy + dy, cx + dx
                    if 0 <= y < n and 0 <= x < n and free[y, x]:
                        found = (y, x)
                        break
                if found:
                    break
            if found:
                break
        if not found:
            return False, 0
        cy, cx = found

    seen = np.zeros((n, n), dtype=bool)
    stack = [(cy, cx)]
    seen[cy, cx] = True
    count = 0
    escaped = False
    while stack:
        y, x = stack.pop()
        count += 1
        if y in (0, n - 1) or x in (0, n - 1):
            escaped = True
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < n and 0 <= nx < n and free[ny, nx] and not seen[ny, nx]:
                seen[ny, nx] = True
                stack.append((ny, nx))
    return (not escaped), count


# ==========================================================================
# evidence rendering
# ==========================================================================
def _font(path, size):
    try:
        return ImageFont.truetype(str(path), size)
    except Exception:  # pragma: no cover - font is present on this machine
        return ImageFont.load_default()


def build_contact_sheet(images, bg, dest, title, subtitle):
    f_title = _font(FONT_BOLD, 22)
    f_label = _font(FONT_REGULAR, 16)
    f_small = _font(FONT_REGULAR, 13)

    left = 210
    cell_w = 300
    row_h = 120
    pad = 26
    width = left + cell_w * 2 + pad
    height = pad + 66 + row_h * len(SHEET_SIZE) + pad

    sheet = Image.new("RGB", (width, height), tuple(bg))
    d = ImageDraw.Draw(sheet)
    fg = (24, 24, 27) if sum(bg) > 380 else (238, 240, 244)
    dim = (120, 122, 128) if sum(bg) > 380 else (150, 154, 162)

    d.text((pad, pad - 4), title, font=f_title, fill=fg)
    d.text((pad, pad + 26), subtitle, font=f_small, fill=dim)
    d.line([(pad, pad + 54), (width - pad, pad + 54)], fill=dim, width=1)

    d.text((pad, pad + 66), "actual size", font=f_label, fill=fg)
    d.text((pad + left, pad + 66), "magnified (nearest neighbour)", font=f_label, fill=fg)

    for i, size in enumerate(SHEET_SIZE):
        img = images[size]
        top = pad + 96 + i * row_h
        d.text((pad, top + 34), f"{size} x {size} px", font=f_label, fill=fg)
        mag = 4 if size <= 32 else (2 if size <= 64 else 1)
        d.text((pad, top + 56), f"x{mag} shown" if mag > 1 else "1:1 only", font=f_small, fill=dim)

        cell = Image.new("RGBA", (cell_w - 24, row_h - 20), tuple(bg) + (255,))
        cell.alpha_composite(img, ((cell.width - size) // 2, (cell.height - size) // 2))
        sheet.paste(cell.convert("RGB"), (pad + left - 24, top))

        big = img.resize((size * mag, size * mag), Image.Resampling.NEAREST)
        if big.width > cell_w - 40 or big.height > row_h - 20:
            big.thumbnail((cell_w - 40, row_h - 20), Image.Resampling.NEAREST)
        cell2 = Image.new("RGBA", (cell_w - 24, row_h - 20), tuple(bg) + (255,))
        cell2.alpha_composite(big, ((cell2.width - big.width) // 2, (cell2.height - big.height) // 2))
        sheet.paste(cell2.convert("RGB"), (pad + left + cell_w, top))

    sheet.save(dest, optimize=True)
    return dest


def build_single_evidence(img, bg, dest, size):
    pad = 30
    mag = 4 if size <= 32 else (2 if size <= 64 else 1)
    big = img.resize((size * mag, size * mag), Image.Resampling.NEAREST)
    w = pad * 2 + size + 24 + big.width
    h = pad * 2 + max(size, big.height)
    sheet = Image.new("RGB", (w, h), tuple(bg))
    sheet.paste(img, (pad, (h - size) // 2), img)
    sheet.paste(big, (pad + size + 24, (h - big.height) // 2), big)
    sheet.save(dest, optimize=True)
    return dest


# ==========================================================================
# main
# ==========================================================================
def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    for d in (BRAND, PNG_DIR, PREVIEW_DIR, TAURI_ICONS):
        d.mkdir(parents=True, exist_ok=True)

    produced: list[tuple[str, int, str]] = []

    def record(path):
        p = Path(path)
        produced.append((str(p.relative_to(ROOT)).replace("\\", "/"), p.stat().st_size, sha256(p)))
        return p

    # ---- master vector -------------------------------------------------
    svg = BRAND / "nexus-icon.svg"
    svg.write_text(build_svg(), encoding="utf-8", newline="\n")
    record(svg)

    # ---- rasters -------------------------------------------------------
    images = {s: render_mark(s) for s in PNG_SIZES}
    for s in PNG_SIZES:
        p = PNG_DIR / f"nexus-icon-{s}.png"
        images[s].save(p, optimize=True)
        record(p)

    # ---- ICO (identical bytes in both locations) -----------------------
    ico = BRAND / "nexus-icon.ico"
    build_ico([images[s] for s in ICO_SIZES], ico)
    record(ico)
    tauri_ico = TAURI_ICONS / "icon.ico"
    tauri_ico.write_bytes(ico.read_bytes())
    record(tauri_ico)

    # ---- Tauri png assets ---------------------------------------------
    for name, s in TAURI_PNG.items():
        p = TAURI_ICONS / name
        images[s].save(p, optimize=True)
        record(p)
    for name, s in SQUARE_LOGOS.items():
        images.setdefault(s, render_mark(s)).save(TAURI_ICONS / name, optimize=True)
        record(TAURI_ICONS / name)

    # ---- measurements --------------------------------------------------
    geo = {row["px"]: row for row in geometry_report()}
    metrics = []
    for size in sorted(set(SHEET_SIZE) | {512}):
        for label, bg in (("light", LIGHT_BG), ("dark", DARK_BG), ("void", VOID_BG)):
            m = measure(images[size], bg)
            m["bg"] = label
            m["geo"] = geo.get(size, {})
            metrics.append(m)

    # ---- evidence images ----------------------------------------------
    contact_light = PREVIEW_DIR / "contact-light.png"
    build_contact_sheet(
        images,
        LIGHT_BG,
        contact_light,
        "NEXUS mark - light background",
        "Windows 11 / light surfaces. Transparent PNG rasters rendered by nexus_mark.py; no OS shell involved.",
    )
    record(contact_light)
    contact_dark = PREVIEW_DIR / "contact-dark.png"
    build_contact_sheet(
        images,
        DARK_BG,
        contact_dark,
        "NEXUS mark - dark background (Windows 11 taskbar 1f1f1f)",
        "Every leg must stay separable on the dark taskbar; the rim band carries the silhouette at 16 px.",
    )
    record(contact_dark)

    legibility = {}
    for size in SHEET_SIZE:
        for label, bg in (("light", LIGHT_BG), ("dark", DARK_BG)):
            out = PREVIEW_DIR / f"legibility-{size}-{label}.png"
            build_single_evidence(images[size], bg, out, size)
            record(out)
            legibility[(size, label)] = out

    # ---- ICO structural verification ----------------------------------
    frames = read_ico_frames(ico)
    ico_ok = sorted(frames) == sorted(ICO_SIZES)

    # ---- report --------------------------------------------------------
    lines = []
    lines.append("NEXUS identity build report")
    lines.append("=" * 72)
    lines.append("")
    lines.append("build command:")
    # لا يُدوَّن مسار أداة شخصي في تقرير البناء: يُذكر المفسّر الفعلي فقط
    lines.append(f'  "{sys.executable}" {Path(__file__)}')
    lines.append("")
    lines.append("geometry (256x256 design space)")
    lines.append("-" * 72)
    lines.append(f"  hexagon circumradius      {HEX_R}  corner radius {HEX_CORNER}")
    lines.append(f"  rim band                  inset {RIM_INSET} at alpha {RIM_ALPHA}")
    lines.append(f"  ring centreline radius    {RING_R}  corner radius {RING_CORNER}  stroke {RING_W}")
    lines.append(f"  node radius               {NODE_R}")
    lines.append("")
    lines.append("analytic legibility (px)")
    lines.append("-" * 72)
    lines.append(f"  {'size':>5}{'hex w':>9}{'hex h':>9}{'stroke':>9}{'node dia':>10}{'rim':>7}")
    for row in geometry_report():
        lines.append(
            f"  {row['px']:>5}{row['hex_w']:>9.2f}{row['hex_h']:>9.2f}"
            f"{row['ring_stroke']:>9.2f}{row['node_dia']:>10.2f}{row['rim_band']:>7.2f}"
        )
    lines.append("")
    lines.append("measured contrast and structure")
    lines.append("-" * 72)
    lines.append(
        f"  {'size':>5} {'bg':>6} {'mark%':>7} {'edge/bg':>8} {'plate/bg':>9} {'gold/plate':>11} {'sealed':>7}"
    )
    for m in metrics:
        sealed = m.get("aperture_sealed")
        lines.append(
            f"  {m['size']:>5} {m['bg']:>6} {m['mark_ratio'] * 100:>6.1f}%"
            f" {str(m['edge_vs_bg']):>8} {str(m['plate_vs_bg']):>9} {str(m['gold_vs_plate']):>11}"
            f" {('yes' if sealed else 'no') if sealed is not None else '-':>7}"
        )
    lines.append("")
    lines.append(f"ICO sizes embedded   : {sorted(frames)}")
    lines.append(f"ICO size set complete: {ico_ok}")
    lines.append("")
    lines.append("ICO frame verification (independent re-read of the container)")
    lines.append("-" * 72)
    for s, (h, opaque) in sorted(frames.items()):
        same = "matches standalone render" if h == hashlib.sha256(
            np.asarray(images[s]).tobytes()
        ).hexdigest() else "DIFFERS from standalone render"
        lines.append(f"  {s:>3} px  opaque={opaque:<6} rgba-sha256={h[:16]}...  {same}")
    lines.append("")
    lines.append("artifacts (sha256)")
    lines.append("-" * 72)
    for rel, size, digest in produced:
        lines.append(f"  {digest}  {size:>8}  {rel}")
    lines.append("")
    lines.append(f"artifact count: {len(produced)}")
    lines.append(f"total bytes   : {sum(p[1] for p in produced)}")
    report_path = BRAND / "build-report.txt"
    report_path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")

    # ---- PREVIEW.md ----------------------------------------------------
    preview = build_preview_md(produced, metrics, frames, geo, ico_ok)
    (BRAND / "PREVIEW.md").write_text(preview, encoding="utf-8", newline="\n")

    print("\n".join(lines))
    print()
    print(f"PREVIEW.md written ({len(preview)} bytes)")
    if not ico_ok:
        raise SystemExit("ICO size set incomplete")
    return 0


def build_preview_md(produced, metrics, frames, geo, ico_ok):
    by_name = {rel: (size, digest) for rel, size, digest in produced}

    def row(rel):
        size, digest = by_name[rel]
        return f"| `{rel}` | {size:,} | `{digest}` |"

    core = [rel for rel, _, _ in produced if rel.startswith("assets/branding/nexus-icon")
            or rel.startswith("assets/branding/png/")
            or rel.startswith("apps/desktop/src-tauri/icons/icon.ico")
            or rel in {f"apps/desktop/src-tauri/icons/{n}" for n in TAURI_PNG}
            or rel == "apps/desktop/src-tauri/icons/StoreLogo.png"]
    extras = [rel for rel, _, _ in produced if rel not in core and rel.startswith("apps/desktop/src-tauri/icons/")]
    sheets = [rel for rel, _, _ in produced if rel.startswith("assets/branding/preview/contact")]

    light_rows = "\n".join(
        f"| {s} px | {geo[s]['hex_w']:.1f} x {geo[s]['hex_h']:.1f} | {geo[s]['ring_stroke']:.2f} | "
        f"{geo[s]['node_dia']:.2f} | {m['edge_vs_bg']} | {m['gold_vs_plate']} | "
        f"{'yes' if m.get('aperture_sealed') else ('no' if m.get('aperture_sealed') is False else 'n/a')} | "
        f"[`legibility-{s}-light.png`](preview/legibility-{s}-light.png) |"
        for s in SHEET_SIZE
        for m in [next(x for x in metrics if x["size"] == s and x["bg"] == "light")]
    )
    dark_rows = "\n".join(
        f"| {s} px | {geo[s]['hex_w']:.1f} x {geo[s]['hex_h']:.1f} | {geo[s]['ring_stroke']:.2f} | "
        f"{geo[s]['node_dia']:.2f} | {m['edge_vs_bg']} | {m['gold_vs_plate']} | "
        f"{'yes' if m.get('aperture_sealed') else ('no' if m.get('aperture_sealed') is False else 'n/a')} | "
        f"[`legibility-{s}-dark.png`](preview/legibility-{s}-dark.png) |"
        for s in SHEET_SIZE
        for m in [next(x for x in metrics if x["size"] == s and x["bg"] == "dark")]
    )
    frame_rows = "\n".join(
        f"| {s} px | {'PNG' if s >= 256 else '32-bit BMP + AND mask'} | {opaque:,} | `{h[:16]}...` |"
        for s, (h, opaque) in sorted(frames.items())
    )

    return f"""# NEXUS application mark - preview and verification

**Task:** `task-3` (design-system-icon) - original NEXUS identity.
**Status:** software-rendered and measured on this machine. See
[NOT_VERIFIED](#not_verified) before claiming anything about real Windows surfaces.

Rebuild everything (byte-reproducible, no timestamps, no network):

```
"<PYTHON_INTERPRETER>" <REPO_ROOT>\\assets\\branding\\build\\nexus_mark.py
```

## The idea

An **original** mark expressing Governance + Connected Intelligence + Security + Control
without any forbidden cliché (no robot, no brain, no lone padlock, no third-party mark):

* **Hexagonal container** - a bounded, auditable cell. It is literally the *hexagonal
  architecture* shape of this codebase (ADR-001 local-first modular monolith), and a
  closed perimeter is the security claim: nothing crosses it silently.
* **One unbroken gold ring** - three links closed into a single circuit: no gap means no
  bypass. Three links because AI governance here has three recurring roles (policy,
  oversight, audit) and the truth layer requires a human counter-party.
* **Three nodes on the ring** - the connected parts; each node is a seat of intelligence
  joined to the same circuit rather than roaming free.
* **Empty bounded core** - the aperture stays deliberately hollow: that is where human
  approval sits (ADR-005). The mark does not claim the system is autonomous.

All four colours are taken from the running application, so the icon and the in-app brand
mark agree: navy `#294568` -> `#101d30`, rim `#537ba3`, gold `#f0c56f` -> `#e4b65a`.

## Contact sheet

| light surfaces | dark surfaces (Windows 11 taskbar `#1f1f1f`) |
| --- | --- |
| ![light](preview/contact-light.png) | ![dark](preview/contact-dark.png) |

Files: {' , '.join('`' + s + '`' for s in sheets)}

## Legibility - light background (`#ffffff`)

`hex` = rendered plate size, `stroke` = gold circuit stroke width in device pixels,
`node` = node diameter, `sealed` = the flood-fill test proving the gold circuit is a
closed loop at that size (the core aperture does not leak to the outside).

| size | hex (px) | stroke (px) | node (px) | mark edge vs bg | gold vs plate | sealed | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
{light_rows}

## Legibility - dark background (`#1f1f1f`)

| size | hex (px) | stroke (px) | node (px) | mark edge vs bg | gold vs plate | sealed | evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
{dark_rows}

Contrast figures are WCAG 2.x relative-luminance ratios measured from the actual
rendered pixels (median of the edge band / plate / gold cluster), not from the source
colours.

## Produced artifacts (SHA-256)

| artifact | bytes | sha256 |
| --- | --- | --- |
{chr(10).join(row(rel) for rel in core)}

Evidence images:

| artifact | bytes | sha256 |
| --- | --- | --- |
{chr(10).join(row(rel) for rel in sheets)}
{chr(10).join(row(rel) for rel in sorted(rel for rel, _, _ in produced if rel.startswith('assets/branding/preview/legibility-'))[:0])}
Full manifest for every file (including all additional Tauri `Square*Logo.png` /
`StoreLogo.png` assets): `build-report.txt`.

Additional Tauri bundle assets:

| artifact | bytes | sha256 |
| --- | --- | --- |
{chr(10).join(row(rel) for rel in extras)}

## ICO container - independently re-read

The ICO is authored by hand (32-bit BMP + AND mask below 256 px, PNG at 256 px) so the
embedded pixels are exactly the approved renders instead of a second scaler's guess.
Re-reading the file and extracting every frame:

| size | stored as | opaque pixels | frame rgba sha256 |
| --- | --- | --- | --- |
{frame_rows}

Size set complete (16/24/32/48/64/128/256): **{'yes' if ico_ok else 'NO'}**

Each frame's rgba hash is compared against the standalone PNG render of the same size in
`build-report.txt`; a mismatch would be reported as `DIFFERS`.

## NOT_VERIFIED

These were **not** performed and must not be claimed:

* Real Windows taskbar / Start menu / Alt-Tab rendering, DPI scaling by the shell, or
  clear-type/theme recolouring of the icon.
* Installer (`msi`/`nsis`) icon rendering, shortcut (`.lnk`) icons, or Explorer thumbnails.
* Any macOS/`.icns` output - this is a Windows-only target, so no `.icns` is produced.
* Downstream packaging: `tauri.conf.json` `bundle.icon` wiring is owned by the Lead; this
  build only guarantees the files and names exist.
* Perceptual testing with real users.
"""


if __name__ == "__main__":
    sys.exit(main())
