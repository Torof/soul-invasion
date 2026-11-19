#!/usr/bin/env python3
"""Wave-crest slope turf, drawn by algorithm after the validated turf (v14).

Three values laid in order (etude/ETUDE-HERBE.md): a striated calm green, a
few soft dark masses, a dark-green bed of shadow under the upslope edge of
every cream band, soft cream flats with blurred sub-streaks, a penumbra
under every dark stroke, then the black itself (bellied flames in sawtooth
rows, lying fat fringes, lone ticks in the calm) and last the pale wedges
and the rare filaments.  Nothing is a filled blade or an outlined shape.
Everything is drawn at xSS on a torus (seamless by construction) and
downscaled with LANCZOS; the cream is capped at L `cream-cap`.  v12b, the
lead's words: cream patches are long (aspect >= 3), gently curved, fewer,
and not always under a tuft; black strokes thin but for 10-15 % fat ones;
only the cream is blurred, everything else is crisp (the downscale is the
only softening).

    slope_turf.py OUT [--seed 1 --angle 32 ...]  (see --help, README.md)
    slope_turf.py OUT --game GAME      also writes the tile divided by the
                                       screen tint (0.94, 1.02, 0.92)
"""
import argparse
import math
import random

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

# --- palette, on-screen values (the turf is shown through x(0.94, 1.02, 0.92))
GREEN = (100, 146, 62)         # calm, L 123
GREEN_DARK = (46, 92, 46)      # beds, penumbra, dark combing, L 73 (cold, 112 deg)
GREEN_PALE = (150, 180, 100)   # pale combing
CREAM = (190, 200, 130)        # flats, L 189 (capped after)
CREAM_SUB = (152, 172, 102)    # sub-streaks inside the flats, L 158
CREAM_FILET = (190, 200, 132)  # the rare bright filaments, L 189 (capped after)
WEDGE = (180, 193, 120)        # the lit edge of a blade, L 181
DARK = (26, 46, 30)            # black, L 38 (cold)
MASS = (44, 84, 38)            # dark masses, L 67
FUR_DARK = (30, 60, 27)
FUR_LIGHT = (96, 142, 60)
SCREEN_TINT = (0.94, 1.02, 0.92)


def unit(deg):
    """Screen-space unit vector for an angle in degrees, 0 = right, + = up."""
    a = math.radians(deg)
    return math.cos(a), -math.sin(a)


def slerp_deg(a, b, t):
    d = ((b - a + 180.0) % 360.0) - 180.0
    return a + d * t


def poisson_torus(n, side, rmin, rng, accept=None):
    """Up to n points on the torus [0, side)^2, no two closer than rmin;
    accept(x, y) may veto a candidate."""
    cell = rmin / math.sqrt(2.0)
    ncell = max(1, int(side // cell))
    cell = side / ncell
    grid = {}
    pts = []
    tries = 0
    while len(pts) < n and tries < n * 60:
        tries += 1
        p = (rng.random() * side, rng.random() * side)
        if accept is not None and not accept(p[0], p[1]):
            continue
        gx, gy = int(p[0] // cell), int(p[1] // cell)
        ok = True
        for dx in range(-2, 3):
            for dy in range(-2, 3):
                for q in grid.get(((gx + dx) % ncell, (gy + dy) % ncell), ()):
                    ddx = abs(p[0] - q[0]); ddy = abs(p[1] - q[1])
                    ddx = min(ddx, side - ddx); ddy = min(ddy, side - ddy)
                    if ddx * ddx + ddy * ddy < rmin * rmin:
                        ok = False; break
                if not ok: break
            if not ok: break
        if ok:
            pts.append(p)
            grid.setdefault((gx, gy), []).append(p)
    return pts


def belly(s):
    """Width profile of a flame, fraction of w0 along its height: 0.7 at the
    base, 1.3 at 45 %, 1.0 at 80 %, 0 at the tip (etude 1.2)."""
    knots = ((0.0, 0.8), (0.45, 1.2), (0.8, 0.9), (1.0, 0.0))
    for (s0, w0), (s1, w1) in zip(knots, knots[1:]):
        if s <= s1:
            t = (s - s0) / (s1 - s0)
            t = t * t * (3 - 2 * t)
            return w0 + (w1 - w0) * t
    return 0.0


def comet(s):
    """Width profile of a cream band: pointed ends, full in the middle."""
    if s < 0.25:
        t = s / 0.25; return 0.15 + 0.85 * t * t * (3 - 2 * t)
    if s > 0.65:
        t = (1 - s) / 0.35; return 0.1 + 0.9 * t * t * (3 - 2 * t)
    return 1.0


def stroke_poly(p, a0, a1, length, w0, w1, bend=0.0, steps=10, prof=1.0, offs=(0.0, 0.0), wfun=None, srange=(0.0, 1.0)):
    """The polygon of a thin tapered line stroke from p: direction turns from
    a0 to a1 along its length, width from w0 to w1 (or w0 * wfun(s)); bend
    offsets the middle sideways (fraction of length); offs shifts it along
    its normal from offs[0] at the base to offs[1] at the tip; srange keeps
    only that portion.  Returns [] when nothing is left."""
    x, y = p
    left, right = [], []
    step = length / steps
    for i in range(steps + 1):
        s = i / steps
        ang = slerp_deg(a0, a1, s)
        dx, dy = unit(ang)
        nx, ny = -dy, dx
        if srange[0] <= s <= srange[1] + 1e-9:
            off = bend * length * math.sin(math.pi * s) + offs[0] + (offs[1] - offs[0]) * s
            cx, cy = x + nx * off, y + ny * off
            if wfun is None:
                w = (w0 + (w1 - w0) * s ** prof) * 0.5 * min(1.0, s / 0.12) ** 0.5  # rounded base
            else:
                w = w0 * wfun(s) * 0.5
            left.append((cx + nx * w, cy + ny * w))
            right.append((cx - nx * w, cy - ny * w))
        x += dx * step; y += dy * step
    return left + right[::-1] if len(left) >= 2 else []


class Torus:
    """A greyscale mask canvas; every polygon that crosses an edge is drawn
    again on the other side, so the tile wraps."""

    def __init__(self, side):
        self.side = side
        self.img = Image.new('L', (side, side), 0)
        self.draw = ImageDraw.Draw(self.img)

    def offsets(self, pts):
        xs = [p[0] for p in pts]; ys = [p[1] for p in pts]; s = self.side
        ox = [0]; oy = [0]
        if min(xs) < 0: ox.append(s)
        if max(xs) >= s: ox.append(-s)
        if min(ys) < 0: oy.append(s)
        if max(ys) >= s: oy.append(-s)
        return [(a, b) for a in ox for b in oy]

    def polygon(self, pts, fill=255):
        for ox, oy in self.offsets(pts):
            self.draw.polygon([(x + ox, y + oy) for x, y in pts], fill=fill)

    def stroke(self, p, a0, a1, length, w0, w1, bend=0.0, fill=255, steps=10, prof=1.0, offs=(0.0, 0.0), wfun=None, srange=(0.0, 1.0)):
        pts = stroke_poly(p, a0, a1, length, w0, w1, bend, steps, prof, offs, wfun, srange)
        if pts:
            self.polygon(pts, fill)

    def array(self, blur=0.0):
        return blur_wrap(np.asarray(self.img, dtype=np.float32) / 255.0, blur)


class Recorder:
    """Strokes are recorded as elements (a blade with its wedge, a row, a
    patch...) with their footprint, so a tile family can keep, for every
    tile, exactly the elements that touch the margin band (D: v13)."""

    def __init__(self):
        self.elements = []
        self.cur = None

    def begin(self):
        self.cur = []

    def end(self):
        if self.cur:
            self.elements.append(self.cur)
        self.cur = None

    def add(self, name, pts):
        if self.cur is None:
            self.elements.append([(name, pts)])
        else:
            self.cur.append((name, pts))


class Layer:
    """A named mask that only records strokes (drawn later by rasterize)."""

    def __init__(self, rec, name):
        self.rec, self.name = rec, name

    def stroke(self, p, a0, a1, length, w0, w1, bend=0.0, fill=255, steps=10, prof=1.0, offs=(0.0, 0.0), wfun=None, srange=(0.0, 1.0)):
        pts = stroke_poly(p, a0, a1, length, w0, w1, bend, steps, prof, offs, wfun, srange)
        if pts:
            self.rec.add(self.name, pts)


def element_bbox(el):
    xs = [x for (_, pts) in el for (x, _) in pts]
    ys = [y for (_, pts) in el for (_, y) in pts]
    return min(xs), min(ys), max(xs), max(ys)


def touches_band(el, S, M):
    """Does the element's footprint reach within M of an edge (torus)?"""
    x0, y0, x1, y1 = element_bbox(el)
    return x0 < M or y0 < M or x1 > S - M or y1 > S - M


def rasterize(elements, S, names):
    canvases = {n: Torus(S) for n in names}
    for el in elements:
        for (name, pts) in el:
            if name in canvases:
                canvases[name].polygon(pts)
    return canvases


def blur_wrap(arr, radius):
    """Gaussian blur of a float mask that wraps at the edges (torus)."""
    if radius <= 0:
        return arr
    pad = int(math.ceil(radius * 3)) + 1
    big = np.pad(arr, pad, mode='wrap')
    im = Image.fromarray(np.clip(big * 255, 0, 255).astype(np.uint8), 'L').filter(ImageFilter.GaussianBlur(radius))
    return np.asarray(im, dtype=np.float32)[pad:-pad, pad:-pad] / 255.0


def periodic_noise(side, rng, kmax, elong_dir=None, elong=0.0, terms=60, kmin=0.0):
    """Sum of sinusoids with integer wave vectors: periodic on the tile.
    elong>0 damps wave vectors along elong_dir, so blobs stretch along it."""
    ys, xs = np.mgrid[0:side, 0:side].astype(np.float32) / side
    out = np.zeros((side, side), np.float32)
    for _ in range(terms):
        kx = rng.randint(-kmax, kmax); ky = rng.randint(-kmax, kmax)
        if kx == 0 and ky == 0: continue
        k = math.hypot(kx, ky)
        if k < kmin: continue
        amp = 1.0 / k
        if elong_dir is not None:
            dx, dy = elong_dir
            proj = (kx * dx + ky * dy) / k
            amp *= math.exp(-elong * proj * proj)
        out += amp * np.cos(2 * math.pi * (kx * xs + ky * ys) + rng.random() * 2 * math.pi)
    out -= out.mean(); out /= out.std() + 1e-6
    return out


def upsample(f, S):
    """A field computed at 1024 brought to the drawing size (bicubic)."""
    if f.shape[0] == S:
        return f
    return np.asarray(Image.fromarray(f.astype(np.float32), 'F').resize((S, S), Image.Resampling.BICUBIC), dtype=np.float32)


def margin_weight(side, M, B):
    """1 inside the margin band (distance to an edge <= M), falling smoothly
    to 0 at M + B: how much a tile's own field yields to the family's."""
    yy, xx = np.mgrid[0:side, 0:side]
    d = np.minimum(np.minimum(yy, side - 1 - yy), np.minimum(xx, side - 1 - xx)).astype(np.float32)
    t = np.clip((d - M) / B, 0, 1)
    return 1 - t * t * (3 - 2 * t)


def tile_fields(a, rng):
    """Per-seed low-frequency fields at 1024 (v14): the calm mottles, a crest
    density (0.05-1.8) and an angle drift (degrees) at scale `field-scale`."""
    side = a.side
    dx, dy = unit(a.angle)
    kmin, kmax = side / a.field_scale / 1.3, side / a.field_scale * 1.3
    return dict(
        mot=periodic_noise(side, rng, 7, elong_dir=(dx, dy), elong=6.0),
        mot2=periodic_noise(side, rng, 24, elong_dir=(dx, dy), elong=10.0, terms=120),
        dens=np.clip(0.85 + 0.8 * periodic_noise(side, rng, int(math.ceil(kmax)), terms=30, kmin=kmin), 0.05, 1.8),
        angf=a.band_drift * periodic_noise(side, rng, int(math.ceil(kmax)), terms=30, kmin=kmin))


def blend_fields(fam, own, w):
    return {k: w * fam[k] + (1 - w) * own[k] for k in fam}


def composite(rgb, mask, color, alpha=1.0):
    m = np.clip(mask * alpha, 0, 1)[..., None]
    return rgb * (1 - m) + np.array(color, np.float32) * m


def cap_luminance(arr, cap):
    """Scale every pixel brighter than L=cap down to it (etude point 5)."""
    L = 0.299 * arr[..., 0] + 0.587 * arr[..., 1] + 0.114 * arr[..., 2]
    f = np.where(L > cap, cap / np.maximum(L, 1e-3), 1.0)[..., None]
    return arr * f


LAYERS = ('sm_d', 'sm_p', 'cd', 'cp', 'flat', 'tail', 'sub', 'streak', 'bed', 'fd', 'shade', 'wedge', 'filet')


def generate(a, rng, S, ss, geom, fields):
    """Every stroke element of a tile from one RNG, recorded with its
    footprint, in a fixed order: combing, then per tuft its cream patch, beds,
    fringes and sawtooth rows (with their filets), then lone rows.  Ticks
    are generated apart (they need the marks).  Returns the Recorder."""
    k2 = (a.side / 1024) ** 2
    ang, dx, dy, ux, uy = geom
    rec = Recorder()
    L_ = {n: Layer(rec, n) for n in LAYERS}
    dens, angf = fields['dens'], fields['angf']
    at = lambda f, x, y: float(f[int(y / ss) % a.side, int(x / ss) % a.side])
    sm_d, sm_p, cd, cp = L_['sm_d'], L_['sm_p'], L_['cd'], L_['cp']
    flat, tail, sub, streak, bed = L_['flat'], L_['tail'], L_['sub'], L_['streak'], L_['bed']
    fd, shade, wedge, filet = L_['fd'], L_['shade'], L_['wedge'], L_['filet']

    # 1a. wide smudges, darker and paler, stretched along the climb (low alpha)
    for i in range(int(a.comb * 0.2 * k2)):
        x, y = rng.random() * S, rng.random() * S
        L = rng.uniform(40, 120) * ss
        a0 = ang + at(angf, x, y) + rng.gauss(0, 5)
        w = rng.uniform(5, 14) * ss
        (sm_d if i % 2 else sm_p).stroke((x, y), a0, a0 + rng.gauss(0, 5), L, w * 0.6, w * 0.5, rng.uniform(-0.04, 0.04), wfun=comet)
    # 1b. thin dry strokes, 2-4 px, 25-80 px, at the climb angle +- 6 (etude 1.4)
    for i in range(int(a.comb * k2)):
        x, y = rng.random() * S, rng.random() * S
        L = rng.uniform(25, 80) * ss
        a0 = ang + at(angf, x, y) + rng.gauss(0, 6); a1 = a0 + rng.gauss(3, 5)
        w = rng.uniform(1.5, 3.5) * ss
        (cd if i % 3 else cp).stroke((x, y), a0, a1, L, w, 0.4 * ss, rng.uniform(-0.04, 0.04), prof=1.6)

    # 3. crest lines: wavy lines at the climb angle, cut into tuft segments ----
    tufts = []                # (cx, cy, L, angle): where a row of blades stands
    placed = 0
    while placed < a.tufts:
        x, y = rng.random() * S, rng.random() * S
        la = ang + rng.uniform(-a.band_jitter, a.band_jitter)
        phase = rng.random() * 2 * math.pi
        lam = rng.uniform(250, 500) * ss
        amp = rng.uniform(6, 16) * ss
        t = 0.0
        total = rng.uniform(0.5, 1.2) * S
        while t < total and placed < a.tufts:
            # the line turns with the angle field; segments are short with
            # log-normal gaps; the density field keeps or drops each one
            L = min(200, max(25, rng.lognormvariate(math.log(a.band_len), 0.5)))
            gap = min(200, max(5, a.crest_gap * math.exp(rng.gauss(0, 0.8))))
            off = amp * math.sin(2 * math.pi * t / lam + phase) + rng.gauss(0, 5) * ss
            lt = la + at(angf, x, y)
            ldx, ldy = unit(lt)
            cx = (x + ldx * L * ss / 2 + ux * off) % S
            cy = (y + ldy * L * ss / 2 + uy * off) % S
            if rng.random() < at(dens, cx, cy):
                tufts.append((cx, cy, L * ss, lt + rng.gauss(0, 3)))
                placed += 1
            step = (L + gap) * ss
            x += ldx * step; y += ldy * step
            t += step

    def flame(bx, by, lean, h, w0, wedge_p=0.85):
        """One blade by its two edges: the dark upper edge is a bellied flame
        with a rounded base; the pale lower edge is a wedge over 50-80 % of
        the height, glued to the downslope flank; a dark-green shade a little
        wider than the black sits under it."""
        a0 = lean + rng.gauss(-24, 6)
        a1 = lean + rng.gauss(10, 5)
        bend = rng.uniform(-0.06, 0.0)
        gap = rng.uniform(1.6, 3.0) * ss
        kw = dict(bend=bend, steps=16, offs=(-gap * 0.5, 0.0), wfun=belly)
        fd.stroke((bx, by), a0, a1, h, w0, 0.0, **kw)
        if a.shade_a > 0:
            shade.stroke((bx, by), a0, a1, h * 1.05, w0 + a.shade_w * ss, 0.0, **kw)
        if rng.random() < wedge_p:
            hh = h * rng.uniform(0.5, 0.8)
            wedge.stroke((bx, by), a0, slerp_deg(a0, a1, hh / h), hh, rng.uniform(1.4, 2.4) * ss * a.thin, 0.0, bend,
                         prof=1.2, steps=12, offs=(gap * 0.5, gap * 0.15))

    def flame_w0():
        if rng.random() < a.fat:
            return rng.uniform(3.0, 4.5) * ss
        return rng.uniform(1.2, 2.0) * ss * a.thin * a.dark_w

    def flame_lean(row_lean):
        r = rng.random()
        if r < a.left:
            return rng.uniform(95, 135)
        if r < a.left + 0.08:
            return rng.uniform(80, 92)
        return row_lean + rng.gauss(0, a.lean_sd * 0.6)

    def height():
        if rng.random() < 0.08:
            return rng.uniform(28, 38) * ss
        h = rng.lognormvariate(math.log(a.spike), 0.25)
        return min(45, max(8, h)) * ss

    def sawtooth(px, py, row_lean, n, hmax=32 * ss):
        """4-8 short flames touching at the base, heights alternating +-40 %,
        one or two long ones sticking out; sometimes a filet at its foot.
        One element."""
        rec.begin()
        t = 0.0
        hbase = height()
        for k in range(n):
            bx = px + dx * t + ux * rng.uniform(-1.5, 1.5) * ss
            by = py + dy * t + uy * rng.uniform(-1.5, 1.5) * ss
            h = hbase * (1.0 + (0.4 if k % 2 else -0.4) * rng.uniform(0.5, 1.0)) * math.exp(rng.gauss(0, 0.15))
            if rng.random() < 0.08:
                h = rng.uniform(28, 38) * ss
            h = min(hmax, max(8 * ss, h))
            flame(bx, by, flame_lean(row_lean), h, flame_w0())
            t += rng.uniform(3.0, 5.5) * ss
        if rng.random() < a.filets:
            fl = rng.uniform(15, 40) * ss
            fa = ang + rng.gauss(0, 4)
            x, y = px + dx * t * 0.5, py + dy * t * 0.5
            filet.stroke((x - dx * fl * 0.3, y - dy * fl * 0.3), fa, fa + rng.gauss(2, 3), fl, rng.uniform(1.0, 2.0) * ss, 0.0, rng.uniform(-0.03, 0.03), steps=12)
        rec.end()
        return t

    for (cx, cy, L, ba) in tufts:
        bdx, bdy = unit(ba)
        # 4. cream patch: long (aspect >= 4), gently curved, feathered; under the
        #    tuft, offset from it, or absent (the lead's shares); one element
        r = rng.random()
        if r >= a.cream_none and a.cream_a > 0:
            pl = min(200, max(60, rng.lognormvariate(math.log(a.cream_len), 0.35))) * ss
            pw = rng.uniform(12, 25) * ss
            pw = min(pw, pl / 4.0)
            if r < a.cream_none + a.cream_below:
                so = rng.uniform(-0.2, 0.2) * L
                no = -(pw * 0.5 + rng.uniform(1, 4) * ss)
            else:
                so = rng.choice([-1, 1]) * rng.uniform(0.3, 0.8) * L
                no = -(pw * 0.5 + rng.uniform(6, 25) * ss)
            px, py = cx + bdx * so + ux * no, cy + bdy * so + uy * no
            pa, bend = ba + rng.gauss(0, 3), rng.uniform(0.02, 0.07) * rng.choice([-1, 1])
            pdx, pdy = unit(pa)
            x0 = px - pdx * pl / 2; y0 = py - pdy * pl / 2
            rec.begin()
            flat.stroke((x0, y0), pa + rng.gauss(0, 3), pa + rng.gauss(0, 3), pl, pw, 0.0, bend, wfun=comet, steps=16)
            tail.stroke((x0 - ux * pw * 0.3, y0 - uy * pw * 0.3), pa, pa, pl, pw * 0.9, 0.0, bend, wfun=comet, steps=16)
            for k in range(1 + int(pl / (40 * ss))):
                fl = pl * rng.uniform(0.25, 0.6)
                so = rng.uniform(-0.45, 0.45) * pl
                no = rng.uniform(-0.4, 0.4) * pw
                sub.stroke((px + pdx * (so - fl / 2) + ux * no, py + pdy * (so - fl / 2) + uy * no), pa + rng.gauss(0, 2), pa + rng.gauss(0, 3), fl, rng.uniform(2, 5) * ss, rng.uniform(2, 6) * ss, bend)
            for k in range(1 + int(pl / (50 * ss))):
                fl = rng.uniform(20, 60) * ss
                so = rng.uniform(-0.5, 0.5) * pl
                no = rng.uniform(-0.45, 0.45) * pw
                streak.stroke((px + pdx * (so - fl / 2) + ux * no, py + pdy * (so - fl / 2) + uy * no), pa + rng.gauss(0, 2), pa + rng.gauss(0, 2), fl, rng.uniform(1.5, 3.0) * ss, 0.0, bend, steps=12)
            rec.end()
        # 5. beds of shadow: crisp dark-green strokes above the tuft row
        if a.bed_a > 0:
            for k in range(2 + int(L / (60 * ss))):
                bw = rng.uniform(0.5, 1.2) * a.bed_w * ss
                bl = L * rng.uniform(0.5, 1.0)
                so = rng.uniform(-0.5, 0.5) * (L - bl) if L > bl else 0.0
                no = bw * 0.45 + rng.uniform(0, 6) * ss
                bed.stroke((cx + bdx * (so - bl / 2) + ux * no, cy + bdy * (so - bl / 2) + uy * no), ba + rng.gauss(0, 3), ba + rng.gauss(0, 4), bl, bw, bw * rng.uniform(0.3, 0.8), rng.uniform(-0.03, 0.03), wfun=comet)
        # 6. lying fat fringes along the tuft line, intermittent (the fat population)
        t = -L / 2
        while t < L / 2:
            fl = rng.uniform(30, 90) * ss
            if rng.random() < a.fringes:
                ex = cx + bdx * t + ux * rng.uniform(-2, 2) * ss
                ey = cy + bdy * t + uy * rng.uniform(-2, 2) * ss
                fa0, fa1, fb = ba + rng.gauss(0, 4), ba + rng.gauss(12, 6), rng.uniform(-0.04, 0.02)
                g = rng.uniform(3.0, 5.0) * ss
                w0 = rng.uniform(4.0, 6.0) * ss * a.thin * a.dark_w
                kw = dict(bend=fb, steps=14, offs=(-g * 0.5, 0.0), wfun=belly)
                rec.begin()
                fd.stroke((ex, ey), fa0, fa1, fl, w0, 0.0, **kw)
                if a.shade_a > 0:
                    shade.stroke((ex, ey), fa0, fa1, fl, w0 + a.shade_w * ss, 0.0, **kw)
                rec.end()
            t += fl + rng.uniform(20, 60) * ss
        # sawtooth rows along the tuft line, never evenly spaced
        t = -L / 2 + rng.uniform(0, 20) * ss
        while t < L / 2:
            ex = cx + bdx * t + ux * rng.uniform(-2, 2) * ss
            ey = cy + bdy * t + uy * rng.uniform(-2, 2) * ss
            n = rng.choice([4, 5, 5, 6, 7, 8, 9])
            used = sawtooth(ex, ey, a.lean + rng.gauss(0, 10), n)
            t += used + rng.uniform(1, 14) * ss / a.foam
    # lone short rows in the calm (a few)
    for (x, y) in poisson_torus(int(a.lone * k2), S, 40 * ss, rng):
        sawtooth(x, y, a.lean + rng.gauss(0, 10), rng.choice([2, 3, 3, 4]), hmax=22 * ss)
    def ticks(near):
        """Lone ticks of 8-14 px wherever nothing is within 20 px (contrast
        rests); `near` is that mask at 1024.  Returns the new elements."""
        start = len(rec.elements)
        free = lambda x, y: not near[int(y / ss) % a.side, int(x / ss) % a.side]
        for (x, y) in poisson_torus(int(a.ticks * k2), S, 25 * ss, rng, accept=free):
            h = rng.uniform(8, 14) * ss
            lean = flame_lean(a.lean + rng.gauss(0, 10))
            rec.begin()
            flame(x, y, lean, h, rng.uniform(1.2, 1.6) * ss, wedge_p=0.15)
            rec.end()
        return rec.elements[start:]
    rec.ticks = ticks
    return rec


def near_marks(elements, S, ss, side):
    """Mask at 1024 of what lies within 20 px of a dark stroke or a cream flat."""
    cv = rasterize(elements, S, ('fd', 'flat'))
    marks = np.maximum(cv['fd'].array(), cv['flat'].array())
    small = np.asarray(Image.fromarray((marks * 255).astype(np.uint8)).reduce(ss))
    return np.asarray(Image.fromarray(small).filter(ImageFilter.MaxFilter(41))) > 40


def build(a):
    ss = a.ss
    S = a.side * ss
    k2 = (a.side / 1024) ** 2
    ang = a.angle
    dx, dy = unit(ang)            # along the climb (screen)
    ux, uy = -dy, dx              # the normal, made to point toward the top of the screen
    if uy > 0:
        ux, uy = -ux, -uy
    geom = (ang, dx, dy, ux, uy)
    family = a.family is not None
    frng = random.Random(a.family) if family else None
    trng = random.Random(a.seed)
    rng0 = frng if family else trng     # the background and the masses: shared by a family
    M = a.margin * ss

    # Only the cream is blurred (v12b, the lead's rule): everything else is
    # drawn crisp at xSS and softened only by the final downscale.

    # 1. calm green: soft mottle; in a family the tile's own fields (mottle,
    #    crest density, angle drift) yield to the family's inside the margin
    #    band and over `blend` px beyond it (v14) ------------------------------
    if family:
        f_fam = tile_fields(a, frng)
        f_own = tile_fields(a, trng)
        fields = blend_fields(f_fam, f_own, margin_weight(a.side, a.margin, a.blend))
    else:
        f_fam = None
        fields = f_own = tile_fields(a, trng)
    mot = upsample(fields['mot'], S)
    mot2 = upsample(fields['mot2'], S)
    base = np.array(GREEN, np.float32)[None, None, :] * np.ones((S, S, 1), np.float32)
    lum = 0.08 * mot + 0.05 * mot2
    rgb = base * (1.0 + lum[..., None])
    rgb[..., 0] += 6 * mot2          # a touch of hue drift
    del base, lum, mot

    # 2. masses: 2-4 dark zones, elongated along the climb, hard torn edges
    #    (shared by a family: a pixel field, not an element) -------------------
    mass = None
    if a.masses > 0:
        rng = rng0
        ys_, xs_ = np.mgrid[0:S, 0:S].astype(np.float32)
        mm = np.zeros((S, S), np.float32)
        for (bx, by) in poisson_torus(int(a.masses), S, 260 * ss, rng):
            r = rng.uniform(0.7, 1.0) * a.mass_size * ss * 0.5
            e = rng.uniform(0.5, 0.8)
            ddx = (xs_ - bx + S / 2) % S - S / 2
            ddy = (ys_ - by + S / 2) % S - S / 2
            along = ddx * dx + ddy * dy
            across = ddx * ux + ddy * uy
            d2 = (along / r) ** 2 + (across / (r * e)) ** 2
            mm = np.maximum(mm, (d2 < 1.0 + 0.35 * mot2).astype(np.float32))
        del xs_, ys_
        mass = mm
        del mm
        tear = Torus(S)
        rim = np.asarray(Image.fromarray((mass * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(9))) > 127
        rim = rim & ~(np.asarray(Image.fromarray((mass * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(9))) > 127)
        ry, rx = np.nonzero(rim[::4 * ss, ::4 * ss])
        for i in rng.sample(range(len(ry)), min(len(ry), int(40 * a.masses))):
            x, y = rx[i] * 4 * ss + rng.random() * 4 * ss, ry[i] * 4 * ss + rng.random() * 4 * ss
            L = rng.uniform(20, 60) * ss
            a0 = ang + rng.gauss(0, 10)
            tear.stroke((x, y), a0, a0 + rng.gauss(5, 8), L, rng.uniform(3, 7) * ss, 0.5 * ss, rng.uniform(-0.05, 0.05), prof=1.3)
        rgb = composite(rgb, mass, MASS, a.mass_a)
        rgb = composite(rgb, tear.array(), MASS, a.mass_a * 0.8)
        del tear, rim
        if a.fur > 0:
            hd, hl = Torus(S), Torus(S)
            want, tries = int(a.fur * a.masses * 2200 * k2), 0
            while want > 0 and tries < 400000:
                tries += 1
                x, y = rng.random() * S, rng.random() * S
                if mass[int(y) % S, int(x) % S] < 0.5: continue
                want -= 1
                L = rng.uniform(14, 30) * ss
                a0 = rng.uniform(25, 75); a1 = a0 + rng.uniform(10, 40)
                hd.stroke((x, y), a0, a1, L, 1.8 * ss, 0.0, rng.uniform(-0.12, 0.0), prof=1.0, steps=12)
                if rng.random() < 0.7:
                    hl.stroke((x + 1.5 * ss, y + 1.0 * ss), a0, a1, L, 1.3 * ss, 0.0, rng.uniform(-0.12, 0.0), steps=12, srange=(0.2, 1.0))
            rgb = composite(rgb, hd.array(), FUR_DARK, 0.8)
            rgb = composite(rgb, hl.array(), FUR_LIGHT, 0.85)
            del hd, hl
    del mot2

    # 3-6. the elements: in a family, those touching the margin band come
    #      from the family RNG (identical in every tile), the others from the
    #      tile's own seed and never reach the band ----------------------------
    if family:
        fam = generate(a, frng, S, ss, geom, f_fam)
        own = generate(a, trng, S, ss, geom, fields)
        kept = [e for e in fam.elements if touches_band(e, S, M)]
        if a.ticks > 0:
            kept += [e for e in fam.ticks(near_marks(kept, S, ss, a.side)) if touches_band(e, S, M)]
        own_kept = [e for e in own.elements if not touches_band(e, S, M)]
        if a.ticks > 0:
            own_kept += [e for e in own.ticks(near_marks(kept + own_kept, S, ss, a.side)) if not touches_band(e, S, M)]
        n_fam, n_own = len(kept), len(own_kept)
        kept += own_kept
        a.family_counts = (n_fam, n_own)
    else:
        own = generate(a, trng, S, ss, geom, fields)
        kept = list(own.elements)
        if a.ticks > 0:
            kept += own.ticks(near_marks(kept, S, ss, a.side))
    cv = rasterize(kept, S, LAYERS)
    del kept

    def weaken(m):
        return m if mass is None else m * (1 - 0.6 * mass)

    rgb = composite(rgb, cv['sm_d'].array(), GREEN_DARK, 0.25 * a.comb_a)
    rgb = composite(rgb, cv['sm_p'].array(), GREEN_PALE, 0.18 * a.comb_a)
    rgb = composite(rgb, cv['cd'].array(), GREEN_DARK, 0.3 * a.comb_a)
    rgb = composite(rgb, cv['cp'].array(), GREEN_PALE, 0.28 * a.comb_a)
    if a.cream_a > 0:
        core = weaken(np.clip(cv['flat'].array(2.5 * ss) * 1.2, 0, 1))
        rgb = composite(rgb, weaken(cv['tail'].array(7 * ss)), CREAM, 0.35)
        rgb = composite(rgb, core, CREAM, a.cream_a)
        rgb = composite(rgb, cv['sub'].array(3.0 * ss) * core, CREAM_SUB, 0.5)
        rgb = composite(rgb, cv['streak'].array(1.0 * ss) * np.clip(core * 2, 0, 1), CREAM_FILET, 0.55)
        del core
    if a.bed_a > 0:
        rgb = composite(rgb, cv['bed'].array(), GREEN_DARK, a.bed_a)
    if a.shade_a > 0:
        rgb = composite(rgb, cv['shade'].array(), GREEN_DARK, a.shade_a)
    rgb = composite(rgb, cv['fd'].array(), DARK, 1.0)
    rgb = composite(rgb, cv['wedge'].array(), WEDGE, 0.95)
    rgb = composite(rgb, cv['filet'].array(), CREAM_FILET, 0.9)
    del cv

    out = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8))
    del rgb
    final = a.out_side if a.out_side else a.side
    out = out.resize((final, final), Image.Resampling.LANCZOS)
    arr = cap_luminance(np.asarray(out, dtype=np.float32), a.cream_cap)
    return Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))


def for_game(img):
    """The file version: the on-screen tile divided by the screen tint."""
    arr = np.asarray(img.convert('RGB'), dtype=np.float32) / np.array(SCREEN_TINT, np.float32)
    return Image.fromarray(np.clip(arr, 0, 255).astype(np.uint8))


def seam_measure(img):
    arr = np.asarray(img, dtype=np.float32)
    edge = 0.5 * (np.abs(arr[:, 0] - arr[:, -1]).mean() + np.abs(arr[0] - arr[-1]).mean())
    inner = 0.5 * (np.abs(arr[:, 1:] - arr[:, :-1]).mean() + np.abs(arr[1:] - arr[:-1]).mean())
    return edge, inner


if __name__ == '__main__':
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('out')
    ap.add_argument('--game', default=None, help='also write the tile divided by the screen tint here')
    ap.add_argument('--side', type=int, default=1024)
    ap.add_argument('--ss', type=int, default=3, help='supersampling factor')
    ap.add_argument('--angle', type=float, default=30.0, help='climb angle, degrees above the horizontal toward the right')
    ap.add_argument('--tufts', type=int, default=120, help='tuft rows (segments of crest lines) per tile')
    ap.add_argument('--cream-none', type=float, default=0.25, help='share of tufts with no cream patch')
    ap.add_argument('--cream-below', type=float, default=0.35, help='share of tufts with the patch right below (the rest: offset)')
    ap.add_argument('--cream-len', type=float, default=100.0, help='median patch length, px (60-200, width 12-25, aspect >= 3)')
    ap.add_argument('--crest-gap', type=float, default=50.0, help='mean gap between segments on a crest line, px')
    ap.add_argument('--foam', type=float, default=2.0, help='density of sawtooth rows on the edges (gaps / foam)')
    ap.add_argument('--spike', type=float, default=13.0, help='median flame height, px at 1024')
    ap.add_argument('--fringes', type=float, default=0.3, help='probability of a lying fat fringe per 30-90 px of band edge')
    ap.add_argument('--out-side', dest='out_side', type=int, default=0, help='final image size, px (0 = --side); 2048 keeps the drawing at 1024-scale but delivers twice the pixels for the max zoom')
    ap.add_argument('--thin', type=float, default=1.0, help='scale on every black stroke width (1 = v12b; 0.6 = thin, crisp blades)')
    ap.add_argument('--dark-w', dest='dark_w', type=float, default=1.0, help='extra scale on the dark edge width only (the lit edge keeps --thin)')
    ap.add_argument('--wedge-l', dest='wedge_l', type=float, default=1.0, help='lightness scale of the lit edge of blades (0.85 = a little darker green)')
    ap.add_argument('--fat', type=float, default=0.12, help='share of fat flames (w0 3-4.5 px; the rest 1.2-2 px)')
    ap.add_argument('--shade-w', type=float, default=1.5, help='extra width of the crisp dark-green shade under each black stroke, px')
    ap.add_argument('--shade-a', type=float, default=0.55, help='shade alpha (GREEN_DARK)')
    ap.add_argument('--bed-w', type=float, default=10.0, help='width of the crisp dark-green bed strokes above a tuft row, px')
    ap.add_argument('--bed-a', type=float, default=0.6, help='bed alpha (GREEN_DARK)')
    ap.add_argument('--masses', type=int, default=3, help='number of soft dark masses per tile (0 = none)')
    ap.add_argument('--mass-size', type=float, default=360.0, help='mass length, px (120-300)')
    ap.add_argument('--mass-a', type=float, default=0.85, help='mass alpha (MASS)')
    ap.add_argument('--fur', type=float, default=1.0, help='fur density inside the masses (0 = none, 1 = the painter\'s)')
    ap.add_argument('--comb', type=int, default=2400, help='combing strokes per 1024^2')
    ap.add_argument('--comb-a', type=float, default=2.2, help='combing alpha factor (1 = v11)')
    ap.add_argument('--ticks', type=int, default=60, help='lone ticks seeded where nothing is within 20 px, per 1024^2')
    ap.add_argument('--lone', type=int, default=50, help='lone short rows in the calm per 1024^2')
    ap.add_argument('--filets', type=float, default=0.2, help='probability of a bright filament per tuft')
    ap.add_argument('--lean', type=float, default=58.0, help='flame tip direction, degrees (90 = straight up)')
    ap.add_argument('--lean-sd', type=float, default=12.0, help='sd of the lean, degrees')
    ap.add_argument('--left', type=float, default=0.08, help='share of flames leaning left (95-135 deg)')
    ap.add_argument('--cream-a', type=float, default=0.88, help='alpha of the cream flats')
    ap.add_argument('--cream-cap', type=float, default=186.0, help='max luminance of the cream')
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--band-jitter', type=float, default=12.0, help='per crest line angle jitter, +- degrees (v14)')
    ap.add_argument('--band-drift', type=float, default=10.0, help='sd of the angle drift field, degrees: the lines curve with it (v14)')
    ap.add_argument('--band-len', type=float, default=70.0, help='median crest segment length, px (log-normal, max 200)')
    ap.add_argument('--field-scale', type=float, default=420.0, help='scale of the density and angle fields, px (300-600)')
    ap.add_argument('--blend', type=float, default=96.0, help='beyond the margin, px over which the tile fields take over from the family fields')
    ap.add_argument('--family', type=int, default=None, help='family seed: elements touching the margin band are drawn from it, identical in every tile of the family (v13)')
    ap.add_argument('--margin', type=float, default=32.0, help='margin band M along the edges, px at 1024 (>= 3 sigma of the widest blur + the downscale)')
    args = ap.parse_args()
    WEDGE = tuple(int(round(c * args.wedge_l)) for c in WEDGE)   # module scope: build() reads the global
    im = build(args)
    im.save(args.out)
    e, i = seam_measure(im)
    print(f'-> {args.out}  seam: edge {e:.2f} vs interior {i:.2f}')
    if getattr(args, 'family_counts', None):
        print(f'   family elements {args.family_counts[0]}, tile elements {args.family_counts[1]}')
    if args.game:
        lossless = {"lossless": True, "quality": 100, "method": 6, "exact": True}
        for_game(im).save(args.game, **(lossless if args.game.endswith(".webp") else {}))
        print(f'-> {args.game}')
