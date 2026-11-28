//! Rifts as a process: where a plate is weak, troughs that begin, stretch
//! and thin the crust, grow cell by cell along the weakest crust, cut the
//! plate in two once they cross it, and unzip; or are abandoned;
//! boundaries that open; the sinking of a rifted margin.

use super::*;

/// A rift (version 11). Along a line across a plate, the forces on the
/// plate's two sides would part them at some speed if nothing held them
/// (the same balance that turns the plates). The lithosphere holds: it
/// lets through only a share of that speed, its weakness, ‰: little
/// where it is cold, old and whole, more over a warm mantle (up to
/// [`WEAK_WARM`] at [`WEAK_HOT`]), more along a suture or a failed
/// rift of the history, more in young crust than in an old core (up to
/// [`WEAK_YOUNG`] for crust just made, nothing at [`CORE_MY`]). The
/// supercontinent's own weakness is its mosaic's (version 13): its
/// belts are younger crust, weakest along their axes, and its cores are
/// old; no flag says so, the crust's age does. As the
/// crust thins the line weakens (necking): the share goes to the whole
/// as the square of the stretching done, cell by cell.
pub(super) const WEAK_COLD: i64 = 10;
pub(super) const WEAK_WARM: i64 = 50;
pub(super) const WEAK_HOT: i64 = 2_500;
pub(super) const WEAK_SEAM: i64 = 40;
pub(super) const WEAK_YOUNG: i64 = 45;
pub(super) const CORE_MY: i64 = 3_000;
/// The stretched zone's width at the start, km (a narrow rift), and the
/// crust's thickness at which it gives way, tenths of a km: the ocean
/// opens there.
pub(super) const RIFT_WIDTH_KM: i64 = 200;
pub(super) const BREAK_THICK: u16 = 150;
/// What a plate's section holds (version 15). The tension across a line
/// through a plate (`forces::parting`: how fast its two sides would part
/// if nothing held them, times the drag they share) is borne by the
/// whole section of the plate along that line. Each cell of the section
/// holds as its lithosphere is strong ([`holds`]): a continent's crust
/// by its thickness and the inverse of its weakness ([`weak`]: an old
/// cold core ten times a warm young belt), the ocean floor as much as
/// the strongest crust once it is old, and little while it is young and
/// thin. A trough begins, and goes on stretching, where the tension is
/// more than the section holds: [`Settings::crust_holds`] is the tension a
/// cell of the strongest crust holds, the one number for the strength of
/// the lithosphere.
/// A trough the tension has left for this many steps is abandoned (a
/// failed rift).
pub(super) const RIFT_IDLE: usize = 4;
/// Each tip of a trough grows as fast as the trough stretches: this
/// many km for each km of opening (the East African rift: ~4 mm a year
/// and ~60 km a million years; once the crust necks the stretching
/// races, and the tips with it). A tip steps to the cell ahead (within
/// 60° of the line across the pull) where the crust is weakest, looking
/// two cells ahead; a step off the line counts for less, as the pull
/// across it does.
pub(super) const RIFT_GROW: i64 = 10;
/// A tip feels the crust this far ahead of it, km (the pull gathers
/// ahead of a tear over a few thicknesses of the plate): it steps toward
/// the weakest way through it.
pub(super) const RIFT_AHEAD_KM: i64 = 368;
/// A trough's line is the line through its ends once it is this many
/// cells long (less before, by its length).
pub(super) const RIFT_LINE_CELLS: i64 = 20;
/// Where a plate may begin to tear: at the weakest of this many spots of
/// its continent, this far apart, km, along the best of twelve lines.
pub(super) const RIFT_SPOTS: usize = 6;
pub(super) const RIFT_SPOT_APART_KM: i64 = 800;
/// The grid's step, km (a walk along a line).
pub(super) const CELL_KM: i64 = 92;

/// A rift stretches and thins the crust this far from its line, km, most
/// at the line. Thinned crust floats lower: at the line it sinks by
/// [`STRETCH_M`] while it stretches, and as much again as it cools,
/// [`COOLING`] ‰ of what is left each step (together what the crust's
/// own buoyancy, [`ISOSTASY_M`], gives for the 20 km it has lost: since
/// version 18 the thinning itself lowers it, [`Rock::thinned`], and
/// half of that is held up by its heat at first). The
/// trough's flanks rise meanwhile, up to [`SHOULDER_M`] (the shoulders
/// of a rift valley), and sink back as they cool.
pub(super) const STRETCH_KM: u32 = 230;
pub(super) const STRETCH_M: i64 =
    ISOSTASY_M * (CONTINENT_THICK as i64 - BREAK_THICK as i64) / 10 / 2;
pub(super) const SHOULDER_M: i64 = STRETCH_M / 2;
pub(super) const COOLING: i64 = 100;

/// The opening at which the crust on a trough's line has given way, m.
pub(super) const BREAK_M: i64 =
    RIFT_WIDTH_KM * 1000 * (CONTINENT_THICK as i64 - BREAK_THICK as i64) / BREAK_THICK as i64;

/// How weak the lithosphere is under a cell of crust, ‰ (see
/// [`WEAK_COLD`]).
pub(super) fn weak(heat: i32, r: &Rock, now_my: u16) -> i64 {
    let warm = i64::from(heat).clamp(0, WEAK_HOT) * WEAK_WARM / WEAK_HOT;
    if r.class == OCEAN {
        return WEAK_COLD + warm;
    }
    let seam = if r.margins & (SUTURE | FAILED_RIFT) != 0 {
        WEAK_SEAM
    } else {
        0
    };
    let age = i64::from(r.born_my.saturating_sub(now_my)).min(CORE_MY);
    WEAK_COLD + warm + seam + WEAK_YOUNG * (CORE_MY - age) / CORE_MY
}

/// What a cell of a plate's section holds of the tension across it, per
/// hundred thousand of what a cell of the strongest crust holds (see
/// [`CRUST_HOLDS`]): the inverse of its weakness, by its thickness for a
/// continent's crust; for the ocean floor by the thickness of its
/// lithosphere, which grows as the square root of its age up to `full_my`.
pub(super) fn holds(heat: i32, r: &Rock, now_my: u16, full_my: i64) -> i64 {
    let strong = 1_000_000 * WEAK_COLD / weak(heat, r, now_my);
    if r.class == OCEAN {
        let age = i64::from(r.born_my.saturating_sub(now_my)).min(full_my);
        strong * (age * 1_000_000 / full_my.max(1)).isqrt() / 1000
    } else {
        strong * i64::from(r.thick) / i64::from(CONTINENT_THICK)
    }
}

/// The tension across a line, ‰ of what its section holds: `pull` (the
/// speed its two sides would part at, µm a year), the drag they share,
/// the section's hold (the sum of [`holds`] along it) and what a cell of
/// the strongest crust holds ([`Settings::crust_holds`]).
pub(super) fn tension(pull: i64, shared: i64, held: i64, strongest: i64) -> i64 {
    if pull <= 0 {
        return 0;
    }
    (i128::from(pull) * i128::from(shared) * 100
        / (i128::from(strongest.max(1)) * i128::from(held.max(1)))) as i64
}

/// The most a section may hold for the tension across it to reach
/// `stress` ‰ of that ([`tension`], turned round).
pub(super) fn held_at_most(pull: i64, shared: i64, strongest: i64, stress: i64) -> i64 {
    (i128::from(pull.max(0)) * i128::from(shared) * 100
        / (i128::from(strongest.max(1)) * i128::from(stress.max(1)))) as i64
}

/// A stretched cell's weakness: its own, and the whole as the square of
/// the stretching done (`ext`, m).
pub(super) fn necked(w0: i64, ext: i64) -> i64 {
    let done = (ext * 1000 / BREAK_M).clamp(0, 1000);
    w0 + (1000 - w0) * done / 1000 * done / 1000
}

/// Pairs of plates whose continents touch, have not parted, and now move
/// apart: their boundary opens.
pub(super) fn opening(
    sphere: &Sphere,
    plate: &[u8],
    rock: &[Rock],
    parted: &[[bool; MAXP]; MAXP],
    group: &[u8; MAXP],
    broken: &mut [[bool; MAXP]; MAXP],
) -> Vec<(u8, u8)> {
    let mut out = Vec::new();
    for c in 0..CELLS {
        if rock[c].class != CONTINENT {
            continue;
        }
        let a = plate[c];
        for &(n, _) in &sphere.links[c] {
            let n = n as usize;
            let b = plate[n];
            let (i, j) = (a as usize, b as usize);
            if b != a
                && rock[n].class == CONTINENT
                && !parted[i][j]
                && group[i] != group[j]
                && !broken[i][j]
            {
                broken[i][j] = true;
                broken[j][i] = true;
                out.push((a.min(b), a.max(b)));
            }
        }
    }
    out
}

/// A rift in progress: a trough along cells of crust of one plate. It is
/// carried with its plate; each step the pull across it stretches and
/// thins the crust along it, each cell from the day the trough reached
/// it, and its tips grow. While it has not crossed its continent the
/// plate is one plate with a rift in it. Once it has crossed and its
/// crust has given way somewhere, the plate is cut along it, for good
/// (version 12): the two plates part where the crust has given way and
/// are still held where it has not, so the ocean unzips. A trough the
/// pull has left is abandoned: it stays.
pub(super) struct Trough {
    /// Its record, an index into the rifts.
    pub(super) record: usize,
    pub(super) plate: u8,
    /// The cells of crust along it, in order from one tip to the other.
    pub(super) path: Vec<u32>,
    /// How far each has opened, m.
    pub(super) ext: Vec<i64>,
    /// Once the plate is cut: the cell of crust each faced across the
    /// cut ([`NO_BIT`]: none).
    pub(super) twin: Vec<u32>,
    /// The pole of the great circle through its ends.
    pub(super) normal: V3,
    /// Whether each tip has reached its continent's edge.
    pub(super) done: [bool; 2],
    /// Steps without stretching.
    pub(super) idle: usize,
    pub(super) failed: bool,
    /// The plate is cut along it; it lasts until all its crust has
    /// given way.
    pub(super) cut: bool,
}

impl Trough {
    /// Keeps the cells of its path that `keep`.
    pub(super) fn keep(&mut self, mut keep: impl FnMut(u32) -> bool) {
        let mut k = 0;
        for i in 0..self.path.len() {
            if keep(self.path[i]) {
                self.path[k] = self.path[i];
                self.ext[k] = self.ext[i];
                self.twin[k] = self.twin[i];
                k += 1;
            }
        }
        self.path.truncate(k);
        self.ext.truncate(k);
        self.twin.truncate(k);
    }
}

/// What a line across a plate reads on the grid.
#[derive(Clone, Copy)]
struct Sheet<'a> {
    sphere: &'a Sphere,
    plate: &'a [u8],
    rock: &'a [Rock],
}

impl Sheet<'_> {
    /// The grid's cell `k` steps from `from` along `dir`.
    fn cell(&self, from: V3, dir: V3, k: i64) -> usize {
        let angle = k * CELL_KM * ONE / RADIUS_KM;
        self.sphere.cell_of(normalize(add(
            scale(from, cos(angle)),
            scale(dir, sin(angle)),
        )))
    }

    /// What the plate's section along the line through `x` heading `dir`
    /// holds: the sum of [`holds`] over its cells, both ways from `x`
    /// until the plate ends (three cells of another plate). None once it
    /// holds more than `most` (the walk stops there: such a section
    /// cannot tear, see [`held_at_most`]).
    fn section(
        &self,
        pl: u8,
        (x, dir): (V3, V3),
        (heat, now_my, full_my): (&[i32], u16, i64),
        most: i64,
    ) -> Option<i64> {
        let around = TAU / 2 * RADIUS_KM / ONE / CELL_KM;
        let at = self.sphere.cell_of(x);
        let hold = |c: usize| holds(heat[c], &self.rock[c], now_my, full_my);
        let mut sum = hold(at);
        for way in [dir, dir.map(|v| -v)] {
            let (mut last, mut off) = (at, 0);
            for k in 1..=around {
                let c = self.cell(x, way, k);
                if c == last {
                    continue;
                }
                last = c;
                if self.plate[c] == pl {
                    off = 0;
                    sum += hold(c);
                    if sum > most {
                        return None;
                    }
                } else {
                    off += 1;
                    if off == 3 {
                        break;
                    }
                }
            }
        }
        Some(sum)
    }

    /// Whether the plate's continent ends ahead: three cells of sea, or
    /// of another plate (a lake of a cell or two is no shore).
    fn shore(&self, pl: u8, from: V3, dir: V3) -> bool {
        (1..=3).all(|k| {
            let c = self.cell(from, dir, k);
            self.plate[c] != pl || self.rock[c].class == OCEAN
        })
    }
}

/// The cells a tip at the cell of crust `from` may step to, heading
/// `dir`: its neighbours in its own lattice and the crust of other
/// pieces of its plate the grid draws around it, within the `cone`
/// about `dir`. Each with what it is worth, its weakness times how far
/// along `dir` the step goes, and its distance, km.
fn forward(
    (sphere, land, top): (&Sphere, &Land, &[u32]),
    (heat, now_my): (&[i32], u16),
    on_path: &[bool],
    from: usize,
    (dir, cone): (V3, i64),
) -> Vec<(usize, i64, i64)> {
    let me = land.cells[from];
    let mut around: Vec<usize> = land.kin(sphere, from).map(|(j, _)| j).collect();
    let q = me.cell as usize;
    for m in std::iter::once(q).chain(sphere.links[q].iter().map(|&(m, _)| m as usize)) {
        if top[m] != NO_BIT && land.cells[top[m] as usize].frame != me.frame {
            around.push(top[m] as usize);
        }
    }
    let mut out = Vec::new();
    for j in around {
        let o = &land.cells[j];
        if o.dead || o.under || o.plate != me.plate || on_path.get(j).is_some_and(|&b| b) {
            continue;
        }
        let km = km_between(o.pos, me.pos);
        if km == 0 {
            continue;
        }
        let along = dot(normalize(tangent(me.pos, sub(o.pos, me.pos))), dir);
        if along >= cone {
            let worth = weak(heat[o.cell as usize], &o.rock, now_my) * along / ONE;
            out.push((j, worth, km));
        }
    }
    out
}

/// The most a tip standing on the cell of crust `from` may gather over
/// its next `depth` steps: the worth of each step ([`forward`]), along
/// the best way ahead. Where the crust ends, the rest counts as the last
/// cell did (a shore neither draws a tip nor turns it away).
fn ahead(
    view: (&Sphere, &Land, &[u32]),
    warm: (&[i32], u16),
    on_path: &[bool],
    seen: &mut std::collections::HashMap<(usize, i64), i64>,
    from: usize,
    heading: &dyn Fn(V3) -> V3,
    depth: i64,
) -> i64 {
    if depth == 0 {
        return 0;
    }
    if let Some(&v) = seen.get(&(from, depth)) {
        return v;
    }
    let me = &view.1.cells[from];
    let steps = forward(view, warm, on_path, from, (heading(me.pos), ONE / 2));
    let v = steps
        .iter()
        .map(|&(j, worth, _)| worth + ahead(view, warm, on_path, seen, j, heading, depth - 1))
        .max()
        .unwrap_or_else(|| depth * weak(warm.0[me.cell as usize], &me.rock, warm.1));
    seen.insert((from, depth), v);
    v
}

/// The pull stretches and thins the crust about a trough: each source
/// (a cell of crust on the line, how far it opens in the step, m) thins
/// the crust around it, most at the line. Thinned crust sinks, and will
/// sink as much again as it cools; the flanks rise, and will sink back.
/// The crust's volume is kept: a thinned cell stands for a wider crust.
fn stretch(sphere: &Sphere, land: &mut Land, sources: &[(usize, i64)], record: usize) {
    if sources.is_empty() {
        return;
    }
    let from: Vec<usize> = sources.iter().map(|x| x.0).collect();
    let (d, origin) = land.spread_from(sphere, &from, STRETCH_KM);
    let (whole, least_thick) = (i64::from(CONTINENT_THICK), i64::from(BREAK_THICK));
    for (i, c) in land.cells.iter_mut().enumerate() {
        if d[i] == u32::MAX || c.dead {
            continue;
        }
        let more = sources[origin[i] as usize].1;
        // Each cell thins from where it is, down to the thickness at
        // which the crust gives way on the line, less away from it.
        let reach = bump(i64::from(d[i]), i64::from(STRETCH_KM));
        let share = reach * reach / 1000;
        let least = least_thick + (whole - least_thick) * (1000 - share) / 1000;
        let r = &mut c.rock;
        r.rift = (record + 1).min(255) as u8;
        let flank = 4 * reach * (1000 - reach) / 1000;
        let up = SHOULDER_M * flank / 1000 * more / BREAK_M;
        if up > 0 {
            r.heat(up, BY_STRETCH);
        }
        let was = i64::from(r.thick);
        let thin =
            (more * was / (RIFT_WIDTH_KM * 1000) * was / whole * share / 1000).min(was - least);
        if thin <= 0 {
            continue;
        }
        c.area = (i64::from(c.area) * was / (was - thin)) as u32;
        r.thinned(thin, was);
    }
}

/// Step 3b. The rifts: troughs begin, stretch the crust, grow, cut
/// their plate in two and unzip.
#[allow(
    clippy::too_many_lines,
    reason = "a trough's step, in the order things happen to it"
)]
pub(super) fn troughs(world: &mut World) {
    let journal = &mut world.journal;
    let s = world.s;
    let sphere = world.sphere;
    let heat = &world.heat;
    let belt = &world.belt;
    let plumes = &world.plumes;
    let spent = &mut world.spent;
    let parted = &mut world.parted;
    let broken = &mut world.broken;
    let alias = &world.alias;
    let born_t = &mut world.born_t;
    let next_id = &mut world.next_id;
    let free = &mut world.free;
    let unnamed = &mut world.unnamed;
    let rifts = &mut world.rifts;
    let lips = &mut world.lips;
    let troughs = &mut world.troughs;
    let edges_last = &world.edges_last;
    let land = &mut world.land;
    let bits = &mut world.bits;
    let disp = &world.disp;
    let top = &world.top;
    let plate = &mut world.plate;
    let rock = &world.rock;
    let omega = &mut world.omega;
    let group = &mut world.group;
    let t = world.step.t;
    let now_my = world.step.now_my;
    let cont_of = &mut world.step.cont_of;
    let (cell_tq, edge_tq) = torques(
        sphere,
        s,
        rock,
        heat,
        Edges {
            slab: &edges_last.0,
            suction: &edges_last.1,
            ridge: &edges_last.2,
            piles: &[],
            bridges: &[],
        },
    );
    let mut loads: Vec<Vec<Load>> = vec![Vec::new(); MAXP];
    for c in 0..CELLS {
        let p = plate[c] as usize;
        if cont_of[p] > 0 {
            let land_here = rock[c].class != OCEAN;
            loads[p].push((
                sphere.centre[c],
                cell_tq[c],
                1000 + if land_here { s.continent_drag } else { 0 },
                rock[c].class == CONTINENT,
            ));
        }
    }
    for &(c, p, tq) in &edge_tq {
        if cont_of[p as usize] > 0 {
            loads[p as usize].push((sphere.centre[c as usize], tq, 0, false));
        }
    }
    let speeds = (s.mantle_drag_mm * 1000, s.speed_mm.1 * 1000);
    let mut busy = [false; MAXP];
    let mut on_path = vec![false; land.cells.len()];
    for tr in &*troughs {
        for &i in &tr.path {
            on_path[i as usize] = true;
        }
    }
    let mut cuts: Vec<usize> = Vec::new();
    let mut over = vec![false; troughs.len()];
    for k in 0..troughs.len() {
        if troughs[k].failed {
            continue;
        }
        let record = troughs[k].record;
        if troughs[k].cut {
            // The plate is cut along it: where its crust has not given
            // way it still joins the two plates, and stretches as
            // they part there (the turns of this step).
            let tr = &mut troughs[k];
            tr.keep(|i| !land.cells[i as usize].dead);
            let mut sources: Vec<(usize, i64)> = Vec::new();
            let mut open = 0;
            for j in 0..tr.path.len() {
                if tr.ext[j] >= BREAK_M {
                    continue;
                }
                let c = land.cells[tr.path[j] as usize];
                let across = (tr.twin[j] != NO_BIT)
                    .then(|| land.cells[tr.twin[j] as usize])
                    .filter(|o| !o.dead);
                if let Some(o) = across {
                    if o.plate == c.plate || o.under || c.under {
                        // The two plates are one again, or one's crust
                        // has run under the other's: nothing to open.
                        tr.ext[j] = BREAK_M;
                        continue;
                    }
                    let dir = normalize(tangent(c.pos, sub(o.pos, c.pos)));
                    let rel = sub(
                        cross(omega[o.plate as usize], c.pos),
                        cross(omega[c.plate as usize], c.pos),
                    );
                    open = dot(rel, dir) * s.step_my;
                }
                let more = open.clamp(0, BREAK_M - tr.ext[j]);
                if more == 0 {
                    continue;
                }
                tr.ext[j] += more;
                sources.push((tr.path[j] as usize, more));
                if across.is_some() {
                    sources.push((tr.twin[j] as usize, more));
                }
            }
            stretch(sphere, &mut *land, &sources, record);
            over[k] = tr.ext.iter().all(|&e| e >= BREAK_M);
            continue;
        }
        // It goes with its crust: on the plate that holds most of it.
        let mut on = [0_u32; MAXP];
        for &i in &troughs[k].path {
            let c = &land.cells[i as usize];
            if !c.dead {
                on[c.plate as usize] += 1;
            }
        }
        let pl = (0..MAXP)
            .max_by_key(|&p| (on[p], std::cmp::Reverse(p)))
            .filter(|&p| on[p] > 0)
            .map_or_else(|| alias_of(alias, troughs[k].plate), |p| p as u8);
        troughs[k].keep(|i| {
            let c = &land.cells[i as usize];
            !c.dead && c.plate == pl
        });
        let give_up = |land: &mut Land, rifts: &mut Vec<Rift>| {
            rifts[record].aborted = true;
            rifts[record].failed_my = now_my;
            for c in &mut land.cells {
                if usize::from(c.rock.rift) == record + 1 {
                    c.rock.margins |= FAILED_RIFT;
                }
            }
        };
        if troughs[k].path.is_empty() {
            // Its crust is gone (run under another piece).
            troughs[k].failed = true;
            give_up(&mut *land, &mut *rifts);
            journal.push(Event::rift(
                What::RiftAbandoned,
                pl,
                rifts[record].centre,
                record,
            ));
            continue;
        }
        busy[pl as usize] = true;
        troughs[k].plate = pl;
        let mut path = std::mem::take(&mut troughs[k].path);
        let mut ext = std::mem::take(&mut troughs[k].ext);
        let at = |i: u32| land.cells[i as usize].pos;
        let mid = at(path[path.len() / 2]);
        // The line it follows: the line it began on, turning to the
        // line through its two ends as it grows (a short path's ends
        // say little).
        let mut n = normalize(tangent(mid, troughs[k].normal));
        if path.len() >= 2 {
            let mut ends = normalize(cross(mid, sub(at(path[path.len() - 1]), at(path[0]))));
            if dot(ends, n) < 0 {
                ends = ends.map(|v| -v);
            }
            let trust = (path.len() as i64 * ONE / RIFT_LINE_CELLS).min(ONE);
            n = normalize(add(n, scale(ends, trust)));
        }
        troughs[k].normal = n;
        let heading = |e: usize, x: V3| {
            let line = normalize(cross(n, x));
            if e == 0 { line.map(|v| -v) } else { line }
        };
        let sheet = Sheet {
            sphere,
            plate: &*plate,
            rock,
        };
        // The share of the pull each cell of the line lets through, as
        // far as it has necked; the trough's rate is their mean. It
        // stretches while the tension is more than its section holds
        // (the thinned crust of its own line holds less: a trough that
        // has begun goes on under a weaker pull).
        let (pull, shared) = parting(&loads[pl as usize], mid, n, speeds).unwrap_or((0, 0));
        let stress = sheet
            .section(
                pl,
                (mid, normalize(cross(n, mid))),
                (heat, now_my, s.slab_age_my),
                held_at_most(pull, shared, s.crust_holds, 1000),
            )
            .map_or(0, |held| tension(pull, shared, held, s.crust_holds));
        let share: Vec<i64> = (0..path.len())
            .map(|j| {
                let c = &land.cells[path[j] as usize];
                necked(weak(heat[c.cell as usize], &c.rock, now_my), ext[j])
            })
            .collect();
        let rate = pull * (share.iter().sum::<i64>() / share.len() as i64) / 1000;
        let put_back = |tr: &mut Trough, path: Vec<u32>, ext: Vec<i64>| {
            tr.twin = vec![NO_BIT; path.len()];
            tr.path = path;
            tr.ext = ext;
        };
        if stress < 1000 || rate <= 0 {
            troughs[k].idle += 1;
            if troughs[k].idle >= RIFT_IDLE {
                troughs[k].failed = true;
                give_up(&mut *land, &mut *rifts);
                journal.push(Event::rift(
                    What::RiftAbandoned,
                    pl,
                    rifts[record].centre,
                    record,
                ));
            }
            put_back(&mut troughs[k], path, ext);
            continue;
        }
        troughs[k].idle = 0;
        // Its tips grow, cell by cell, along the weakest crust ahead.
        for e in 0..2 {
            if troughs[k].done[e] {
                continue;
            }
            let mut budget = rate * s.step_my * RIFT_GROW / 1000;
            while budget > 0 {
                let i = if e == 0 {
                    path[0]
                } else {
                    path[path.len() - 1]
                } as usize;
                let dir = heading(e, land.cells[i].pos);
                if sheet.shore(pl, land.cells[i].pos, dir) {
                    troughs[k].done[e] = true;
                    break;
                }
                let view = (sphere, &*land, top.as_slice());
                let mut seen = std::collections::HashMap::new();
                let mut best: Option<(i64, std::cmp::Reverse<usize>, i64)> = None;
                for cone in [ONE / 2, ONE / 10] {
                    for (j, worth, km) in forward(view, (heat, now_my), &on_path, i, (dir, cone)) {
                        let next = ahead(
                            view,
                            (heat, now_my),
                            &on_path,
                            &mut seen,
                            j,
                            &|x| heading(e, x),
                            RIFT_AHEAD_KM / CELL_KM - 1,
                        );
                        let key = (worth + next, std::cmp::Reverse(j), km);
                        if best.is_none_or(|b| key > b) {
                            best = Some(key);
                        }
                    }
                    if best.is_some() {
                        break;
                    }
                }
                let Some((_, std::cmp::Reverse(j), km)) = best else {
                    troughs[k].done[e] = true;
                    break;
                };
                if j >= on_path.len() {
                    on_path.resize(j + 1, false);
                }
                on_path[j] = true;
                if e == 0 {
                    path.insert(0, j as u32);
                    ext.insert(0, 0);
                } else {
                    path.push(j as u32);
                    ext.push(0);
                }
                budget -= km;
            }
        }
        // The pull stretches the crust along it: each cell from the
        // day the trough reached it, so the oldest gives way first.
        let mut sources: Vec<(usize, i64)> = Vec::new();
        for j in 0..path.len() {
            let c = &land.cells[path[j] as usize];
            let w = necked(weak(heat[c.cell as usize], &c.rock, now_my), ext[j]);
            let m = (pull * w / 1000 * s.step_my).min(BREAK_M - ext[j]);
            if m > 0 {
                ext[j] += m;
                sources.push((path[j] as usize, m));
            }
        }
        stretch(sphere, &mut *land, &sources, record);
        for &i in &path {
            let c = &mut land.cells[i as usize];
            if c.event != Margin::Suture as u8 {
                c.event = Margin::Rift as u8;
            }
        }
        rifts[record].cells = rifts[record].cells.max(path.len() as u32);
        if troughs[k].done == [true; 2] && ext.iter().any(|&e| e >= BREAK_M) {
            cuts.push(k);
        }
        put_back(&mut troughs[k], path, ext);
    }
    // A trough has crossed its continent and its crust has given way
    // somewhere: the plate is cut in two along it, and on through its
    // floor from both ends. Each side keeps its lattice: the two shores
    // fit for good. The new plate is a plate from now on.
    for &k in &cuts {
        let old = troughs[k].plate;
        let path = &troughs[k].path;
        let seat = seats(&*land, top);
        let mut on = vec![false; CELLS];
        for &i in path {
            on[seat[i as usize] as usize] = true;
        }
        let mut sources: Vec<(usize, u32)> = Vec::new();
        for (j, &i) in path.iter().enumerate() {
            let x = land.cells[i as usize].pos;
            let a = land.cells[path[j.saturating_sub(1)] as usize].pos;
            let b = land.cells[path[(j + 1).min(path.len() - 1)] as usize].pos;
            let left = cross(x, sub(b, a));
            let q = seat[i as usize] as usize;
            for &(m, _) in &sphere.links[q] {
                let m = m as usize;
                if plate[m] == old && !on[m] {
                    let side = dot(sub(sphere.centre[m], sphere.centre[q]), left) > 0;
                    sources.push((m, u32::from(side)));
                }
            }
        }
        let (_, label) = sphere.spread(&sources, |_, b| plate[b] == old && !on[b], 30_000);
        let mut count = [0_usize; 2];
        for c in 0..CELLS {
            if plate[c] == old && label[c] < 2 {
                count[label[c] as usize] += 1;
            }
        }
        if count[0] == 0 || count[1] == 0 {
            // It parts nothing from nothing (a neck of a cell or two):
            // no plate to cut, the trough is left as it is.
            let record = troughs[k].record;
            troughs[k].failed = true;
            rifts[record].aborted = true;
            rifts[record].failed_my = now_my;
            for c in &mut land.cells {
                if usize::from(c.rock.rift) == record + 1 {
                    c.rock.margins |= FAILED_RIFT;
                }
            }
            journal.push(Event::rift(
                What::RiftAbandoned,
                old,
                rifts[record].centre,
                record,
            ));
            continue;
        }
        let away = u32::from(count[1] <= count[0]);
        // (With no name left for it the plate is not cut at this step:
        // counted, and tried again at the next.)
        let Some(new) = name(&mut *next_id, &mut *free, &mut *unnamed) else {
            continue;
        };
        let side_new: Vec<bool> = (0..CELLS)
            .map(|c| plate[c] == old && label[c] == away)
            .collect();
        for c in 0..CELLS {
            if side_new[c] {
                plate[c] = new;
                if disp[c] != NO_BIT {
                    bits[disp[c] as usize].plate = new;
                }
            }
        }
        for (i, c) in land.cells.iter_mut().enumerate() {
            if c.plate == old && side_new[seat[i] as usize] {
                c.plate = new;
                c.switch = SWITCH_RIFT;
            }
        }
        // The crust each cell of the line faces across the cut: the
        // neighbour in its lattice that left, the one most across.
        let twin: Vec<u32> = (0..path.len())
            .map(|j| {
                let i = path[j] as usize;
                let x = land.cells[i].pos;
                let a = land.cells[path[j.saturating_sub(1)] as usize].pos;
                let b = land.cells[path[(j + 1).min(path.len() - 1)] as usize].pos;
                let left = normalize(cross(x, sub(b, a)));
                land.kin(sphere, i)
                    .filter(|&(m, _)| land.cells[m].plate == new && !land.cells[m].dead)
                    .max_by_key(|&(m, _)| {
                        let to = normalize(tangent(x, sub(land.cells[m].pos, x)));
                        (dot(to, left).abs(), std::cmp::Reverse(m))
                    })
                    .map_or(NO_BIT, |(m, _)| m as u32)
            })
            .collect();
        for (at, km2) in flood(sphere, &mut *land, top, (old, new), plumes, &mut *spent) {
            lips.push(Lip {
                my: now_my,
                at,
                km2,
            });
        }
        let (o, w) = (old as usize, new as usize);
        // The new plate is a part of the old one: what has parted from
        // the old one, or opened against it, has from the new one too.
        for p in 0..MAXP {
            parted[w][p] = parted[o][p];
            parted[p][w] = parted[p][o];
            broken[w][p] = broken[o][p];
            broken[p][w] = broken[p][o];
        }
        parted[o][w] = false;
        parted[w][o] = false;
        broken[o][w] = true;
        broken[w][o] = true;
        born_t[w] = t;
        group[w] = new;
        omega[w] = omega[o];
        let r = &mut rifts[troughs[k].record];
        r.new = new;
        r.cut_my = now_my;
        r.along_km = path
            .windows(2)
            .map(|x| km_between(land.cells[x[0] as usize].pos, land.cells[x[1] as usize].pos))
            .sum::<i64>() as u32;
        r.straight_km = km_between(
            land.cells[path[0] as usize].pos,
            land.cells[path[path.len() - 1] as usize].pos,
        ) as u32;
        let chord = normalize(cross(
            land.cells[path[0] as usize].pos,
            land.cells[path[path.len() - 1] as usize].pos,
        ));
        r.bow_km = path
            .iter()
            .map(|&i| dot(land.cells[i as usize].pos, chord).abs() * RADIUS_KM / ONE)
            .max()
            .unwrap_or(0) as u32;
        r.cut_cells = path.len() as u32;
        r.belt_cells = path
            .iter()
            .filter(|&&i| {
                let r = &land.cells[i as usize].rock;
                r.class == CONTINENT && belt[r.home as usize] > 0
            })
            .count() as u32;
        journal.push(Event {
            other: old,
            ..Event::rift(
                What::BornOfRift,
                new,
                land.cells[path[path.len() / 2] as usize].pos,
                troughs[k].record,
            )
        });
        troughs[k].twin = twin;
        troughs[k].cut = true;
    }
    if troughs.iter().any(|tr| tr.cut) {
        *cont_of = land.weight();
    }
    let mut k = 0;
    troughs.retain(|_| {
        k += 1;
        !over[k - 1]
    });
    // A plate with no trough begins to tear where the tension across
    // it is more than its section holds there, and was at the step
    // before (a pull of one step tears nothing), along a line with
    // crust to tear ahead: at the weakest spots of its continent, along
    // the line where the tension is the most of what the section holds.
    let sheet = Sheet {
        sphere,
        plate: &*plate,
        rock,
    };
    let pulled = &mut world.pulled;
    for p in 0..MAXP {
        if busy[p] || cont_of[p] == 0 || alias[p] != p as u8 || loads[p].is_empty() {
            pulled[p] = 0;
            continue;
        }
        let mut spots: Vec<(i64, u32, usize)> = land
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                !c.dead && !c.under && c.plate as usize == p && c.rock.class == CONTINENT
            })
            .map(|(i, c)| (weak(heat[c.cell as usize], &c.rock, now_my), c.id, i))
            .collect();
        spots.sort_unstable_by_key(|&(w, id, _)| (std::cmp::Reverse(w), id));
        let mut picked: Vec<(i64, usize)> = Vec::new();
        for &(w, _, i) in &spots {
            if picked.len() == RIFT_SPOTS {
                break;
            }
            let x = land.cells[i].pos;
            if picked
                .iter()
                .all(|&(_, j)| km_between(x, land.cells[j].pos) >= RIFT_SPOT_APART_KM)
            {
                picked.push((w, i));
            }
        }
        let mut best: Option<(i64, std::cmp::Reverse<usize>, usize, V3)> = None;
        for (order, &(_, i)) in picked.iter().enumerate() {
            let x = land.cells[i].pos;
            let (e, nn) = (east_at(x), north_at(x, east_at(x)));
            for a in 0..12 {
                let angle = a * TAU / 24;
                let n = normalize(add(scale(e, cos(angle)), scale(nn, sin(angle))));
                let Some((pull, shared)) = parting(&loads[p], x, n, speeds) else {
                    continue;
                };
                let line = normalize(cross(n, x));
                if sheet.shore(p as u8, x, line) && sheet.shore(p as u8, x, line.map(|v| -v)) {
                    continue;
                }
                if pull <= 0 {
                    continue;
                }
                // Only a line that tears, and more than the best so far.
                let least = best.map_or(1000, |b| (b.0 + 1).max(1000));
                let Some(held) = sheet.section(
                    p as u8,
                    (x, line),
                    (heat, now_my, s.slab_age_my),
                    held_at_most(pull, shared, s.crust_holds, least),
                ) else {
                    continue;
                };
                let key = (
                    tension(pull, shared, held, s.crust_holds),
                    std::cmp::Reverse(order * 12 + a as usize),
                    i,
                    n,
                );
                if best.is_none_or(|b| (key.0, key.1) > (b.0, b.1)) {
                    best = Some(key);
                }
            }
        }
        let before = std::mem::replace(&mut pulled[p], best.map_or(0, |b| b.0));
        if let Some((stress, _, i, n)) = best
            && stress.min(before) >= 1000
        {
            troughs.push(Trough {
                record: rifts.len(),
                plate: p as u8,
                path: vec![i as u32],
                ext: vec![0],
                twin: vec![NO_BIT],
                normal: n,
                done: [false; 2],
                idle: 0,
                failed: false,
                cut: false,
            });
            rifts.push(Rift {
                my: now_my,
                plate: p as u8,
                new: NONE,
                centre: land.cells[i].pos,
                area_km2: cont_of[p],
                aborted: false,
                cut_my: NEVER,
                parted_my: NEVER,
                failed_my: NEVER,
                cells: 1,
                along_km: 0,
                straight_km: 0,
                bow_km: 0,
                cut_cells: 0,
                belt_cells: 0,
                pull: stress.min(i64::from(u32::MAX)) as u32,
            });
            journal.push(Event::rift(
                What::RiftBegun,
                p as u8,
                land.cells[i].pos,
                rifts.len() - 1,
            ));
        }
    }
}

/// Step 11 (the rifts). The crust beside a new ocean floor is a rift's
/// margin: it sinks.
pub(super) fn margins_sink(world: &mut World) {
    let sphere = world.sphere;
    let land = &mut world.land;
    // A rift stretches and thins the crust along it: it sinks at
    // once, and as much again while it cools.
    let torn: Vec<usize> = (0..land.cells.len())
        .filter(|&i| {
            let c = &land.cells[i];
            c.rock.margins & RIFT != 0 && !c.rifted
        })
        .collect();
    if !torn.is_empty() {
        let d = land.spread(sphere, &torn, |_| true, STRETCH_KM);
        for (i, c) in land.cells.iter_mut().enumerate() {
            if d[i] == u32::MAX {
                continue;
            }
            // The crust beside the tear is stretched crust: thinned
            // as a trough's is at that distance from its line, if it
            // is not yet (its volume kept: it stands for a wider one).
            let (whole, least) = (i64::from(CONTINENT_THICK), i64::from(BREAK_THICK));
            let reach = bump(i64::from(d[i]), i64::from(STRETCH_KM));
            let due = least + (whole - least) * (1000 - reach * reach / 1000) / 1000;
            let was = i64::from(c.rock.thick);
            let thin = was.min(whole) - due;
            if thin > 0 && !c.dead {
                c.area = (i64::from(c.area) * was / (was - thin)) as u32;
                c.rock.thinned(thin, was);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rock(class: u8, age: u16, thick: u16) -> Rock {
        Rock {
            class,
            piece: 0,
            margins: 0,
            rift: 0,
            born_my: age,
            orogeny: NEVER,
            home: 0,
            thick,
            fine: 0,
            warm: 0,
            pressed: 0,
            silt: 0,
            laid: NEVER,
            why: [0; CAUSES],
        }
    }

    #[test]
    fn old_cold_crust_holds_more_than_a_young_belt_and_old_floor_more_than_young() {
        let core = holds(0, &rock(CONTINENT, 3_000, CONTINENT_THICK), 0, 80);
        let belt = holds(0, &rock(CONTINENT, 300, CONTINENT_THICK), 0, 80);
        let thinned = holds(0, &rock(CONTINENT, 300, BREAK_THICK), 0, 80);
        let warm = holds(
            WEAK_HOT as i32,
            &rock(CONTINENT, 300, CONTINENT_THICK),
            0,
            80,
        );
        assert!(core > 4 * belt, "{core} {belt}");
        assert!(belt > 2 * thinned && belt > warm, "{belt} {thinned} {warm}");
        let floor = |age| holds(0, &rock(OCEAN, age, OCEAN_THICK), 0, 80);
        assert_eq!(floor(0), 0);
        assert_eq!(floor(80), core);
        assert_eq!(floor(200), core);
        assert_eq!(2 * floor(20), floor(80));
    }

    #[test]
    fn the_tension_across_a_crumb_is_small_however_fast_its_sides_would_part() {
        // The same parting speed across the same section: a side that
        // drags little is carried along for little.
        let section = 20 * 1_000_000;
        let wide = tension(30_000, 5_000_000, section, 300);
        let crumb = tension(30_000, 50_000, section, 300);
        assert!(wide >= 1000 && crumb < 1000, "{wide} {crumb}");
        assert_eq!(tension(-5_000, 5_000_000, section, 300), 0);
        // A weaker section tears under the same pull.
        assert!(tension(30_000, 50_000, section / 200, 300) >= 1000);
    }
}
