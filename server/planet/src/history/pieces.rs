//! The continents as rigid pieces: each cell of crust in its piece's own
//! lattice, placed by the piece's turn; the grid's view of them (`paint`,
//! `seats`) and who rides over whom (`precedence`).

use super::*;

/// A cell of continental crust in its piece, at a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Its cell of the piece's own lattice (the grid's, turned): the
    /// same for good.
    pub at: u32,
    /// Its name, for good.
    pub id: u32,
    /// Its plate (as in [`History::plate`]).
    pub plate: u8,
    /// Its own plate's id (plates held together share the one above).
    pub own: u8,
    /// Why it changed plate in the step (a `SWITCH_*`), 0 if it did not.
    pub switch: u8,
    /// [`CONTINENT`] or [`TERRANE`].
    pub class: u8,
    /// Whether it lies under another piece's crust (a collision).
    pub under: bool,
    /// The margin flags it has gathered.
    pub margins: u8,
    /// Its thickness, tenths of a km.
    pub thick: u16,
    /// The height it carries, m (as [`History::high_at`]).
    pub high: i16,
    /// The sediment lying on it, m of thickness.
    pub silt: u16,
    /// Where it was at the start, or was born: its relief's grain.
    pub home: u32,
    /// The area it stands for, km².
    pub area_km2: u32,
    /// What changed its height since the step before, m, by cause.
    pub why: [i16; CAUSES],
    /// Its precedence where pieces overlap (the higher shows).
    pub rank: (i64, i64, i64),
}

/// A rigid piece of continent at a step (version 10): cells of its own
/// lattice, and the turn that places the lattice on the planet. Moving
/// it changes the turn only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    /// Its frame's name: a piece cut in two gives two frames.
    pub frame: u32,
    /// The turn from its lattice to the planet.
    pub turn: [[i64; 3]; 3],
    /// Its cells.
    pub tiles: Vec<Tile>,
}

impl Piece {
    /// Where one of its cells is on the planet.
    #[must_use]
    pub fn place(&self, sphere: &Sphere, tile: usize) -> V3 {
        normalize(apply(
            &self.turn,
            sphere.centre[self.tiles[tile].at as usize],
        ))
    }
}

/// The continents at a step, read at any point of the planet: the cell
/// of a piece's lattice that holds the point, exactly (the grid's view
/// reads only its cells' centres).
pub struct Pieces {
    back: Vec<M3>,
    slot: Slots,
    near: Vec<Vec<u32>>,
}

impl Pieces {
    /// The index of a step's pieces.
    #[must_use]
    pub fn new(sphere: &Sphere, pieces: &[Piece]) -> Self {
        let mut slot = Slots::default();
        let mut near: Vec<Vec<u32>> = vec![Vec::new(); CELLS];
        for (k, piece) in pieces.iter().enumerate() {
            let k = k as u32;
            for (i, tile) in piece.tiles.iter().enumerate() {
                slot.insert(slot_key(k, tile.at), i as u32);
                let q = sphere.cell_of(piece.place(sphere, i));
                for m in std::iter::once(q).chain(sphere.links[q].iter().map(|&(m, _)| m as usize))
                {
                    if !near[m].contains(&k) {
                        near[m].push(k);
                    }
                }
            }
        }
        Self {
            back: pieces.iter().map(|p| back_of(&p.turn)).collect(),
            slot,
            near,
        }
    }

    /// The cell of a piece at a place of its lattice, if it holds one.
    #[must_use]
    pub fn tile(&self, piece: usize, at: u32) -> Option<usize> {
        self.slot
            .get(&slot_key(piece as u32, at))
            .map(|&i| i as usize)
    }

    /// The crust at a point: its piece and its cell in the piece.
    #[must_use]
    pub fn at(&self, sphere: &Sphere, pieces: &[Piece], p: V3) -> Option<(usize, usize)> {
        let mut best: Option<(Rank, std::cmp::Reverse<u32>, (usize, usize))> = None;
        for &k in &self.near[sphere.cell_of(p)] {
            let at = sphere.cell_of(normalize(apply(&self.back[k as usize], p))) as u32;
            if let Some(&i) = self.slot.get(&slot_key(k, at)) {
                let piece = &pieces[k as usize];
                let key = (
                    piece.tiles[i as usize].rank,
                    std::cmp::Reverse(piece.frame),
                    (k as usize, i as usize),
                );
                if best.is_none_or(|b| key > b) {
                    best = Some(key);
                }
            }
        }
        best.map(|b| b.2)
    }
}

pub(super) type M3 = [[i64; 3]; 3];

pub(super) const IDENTITY: M3 = [[ONE, 0, 0], [0, ONE, 0], [0, 0, ONE]];

pub(super) fn apply(m: &M3, v: V3) -> V3 {
    let r = |row: &[i64; 3]| {
        ((0..3)
            .map(|k| i128::from(row[k]) * i128::from(v[k]))
            .sum::<i128>()
            / i128::from(ONE)) as i64
    };
    [r(&m[0]), r(&m[1]), r(&m[2])]
}

/// A turn of `angle` (millionths of a radian) about the unit `axis`
/// (Rodrigues): points move along `axis × p`.
pub(super) fn rotation(axis: V3, angle: i64) -> M3 {
    let (c, s) = (cos(angle), sin(angle));
    let t = ONE - c;
    let k = axis;
    let kx = [[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]];
    let mut m = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let outer = (i128::from(t) * i128::from(k[i]) * i128::from(k[j])
                / i128::from(ONE)
                / i128::from(ONE)) as i64;
            m[i][j] = if i == j { c } else { 0 } + s * kx[i][j] / ONE + outer;
        }
    }
    m
}

/// A point of a piece's lattice placed on the planet by the piece's turn
/// (not normalised).
#[must_use]
pub fn placed(turn: &[[i64; 3]; 3], p: V3) -> V3 {
    apply(turn, p)
}

/// Where a point carried by a plate turning by `omega` (µm a year, see
/// [`Motion::omega`]) is after a step of `step_my` million years.
#[must_use]
pub fn carried(omega: V3, step_my: i64, p: V3) -> V3 {
    let speed = norm(omega);
    if speed < 50 {
        return p;
    }
    let turn = rotation(normalize(omega), speed * step_my * ONE / (1000 * RADIUS_KM));
    normalize(apply(&turn, p))
}

/// A cell of continental crust (version 10, the lead's 27 November: « de
/// vraies pièces rigides »). It is for good one cell (`at`) of its
/// piece's own lattice (`frame`): moving the piece changes the frame's
/// turn and nothing here, and nothing here is ever resampled. Only named
/// events change it: a collision's front, a rift, an arc, a docking,
/// wear, silt.
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four plain facts about a cell of crust, not a state machine"
)]
pub(super) struct Crust {
    pub(super) frame: u32,
    pub(super) at: u32,
    pub(super) plate: u8,
    /// The area it stands for, km² (a collision shortens it).
    pub(super) area: u32,
    pub(super) rock: Rock,
    /// Its name, for good.
    pub(super) id: u32,
    /// What happened to it in the step (a [`Margin`]).
    pub(super) event: u8,
    /// Whether it lies under another piece's crust.
    pub(super) under: bool,
    /// Whether a rift had torn it before the step.
    pub(super) rifted: bool,
    /// Why it changed plate in the step (a `SWITCH_*`).
    pub(super) switch: u8,
    /// Whether the planet's grid drew it at the step before.
    pub(super) shown: bool,
    pub(super) dead: bool,
    /// At the edge of a continent above a trench: how far its margin has
    /// been squeezed and not yet folded, km (`trenches::margins_fold`).
    pub(super) squeezed: u16,
    /// Where it is on the planet now and the grid's cell there, read from
    /// its frame's turn at each step.
    pub(super) pos: V3,
    pub(super) cell: u32,
}

/// `a` after `b`.
pub(super) fn times(a: &M3, b: &M3) -> M3 {
    let mut m = [[0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            m[i][j] = ((0..3)
                .map(|k| i128::from(a[i][k]) * i128::from(b[k][j]))
                .sum::<i128>()
                / i128::from(ONE)) as i64;
        }
    }
    m
}

/// The turn back.
pub(super) fn back_of(m: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| m[j][i]))
}

/// A turn made a turn again after a product: its rows of length one and
/// square to each other (the rounding of forty products does not add up).
pub(super) fn steady(m: &M3) -> M3 {
    let a = normalize(m[0]);
    let b = normalize(sub(m[1], scale(a, dot(m[1], a))));
    [a, b, normalize(cross(a, b))]
}

/// A plain mixing of a lattice cell's key (the std hasher is slow for
/// millions of small lookups; the map is only ever looked up, never
/// walked, so the order of its keys decides nothing).
#[derive(Default)]
pub(super) struct Mix(u64);

impl std::hash::Hasher for Mix {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }

    fn write_u64(&mut self, v: u64) {
        let x = (self.0 ^ v).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        self.0 = x ^ (x >> 32);
    }
}

pub(super) type Slots = std::collections::HashMap<u64, u32, std::hash::BuildHasherDefault<Mix>>;

pub(super) fn slot_key(frame: u32, at: u32) -> u64 {
    (u64::from(frame) << 32) | u64::from(at)
}

/// The precedence of a cell of crust where several lie over a point: its
/// class, its plate's weight.
pub(super) type Rank = (i64, i64, i64);

/// The continents (version 10): cells of crust in the frames of their
/// pieces. A frame is the grid's own lattice with a turn that places it
/// on the planet today; the cells of a frame on plates that turn alike
/// are one rigid piece, and a frame whose cells come to turn differently
/// (a rift, a block escaping) splits into frames that keep the same
/// lattice, so the two shores of the new ocean fit for good. Pieces
/// welded together keep their frames and turn together.
pub(super) struct Land {
    /// Each frame's turn, and the turn back.
    pub(super) rot: Vec<M3>,
    pub(super) back: Vec<M3>,
    pub(super) cells: Vec<Crust>,
    /// The cell of crust at each held place of each frame's lattice.
    pub(super) slot: Slots,
    /// For neighbourhood queries: the frames with crust in or around
    /// each cell of the planet's grid.
    pub(super) near: Vec<Vec<u32>>,
}

impl Land {
    pub(super) fn new() -> Self {
        Self {
            rot: Vec::new(),
            back: Vec::new(),
            cells: Vec::new(),
            slot: Slots::default(),
            near: vec![Vec::new(); CELLS],
        }
    }

    /// A new frame, turned by `rot`.
    pub(super) fn frame(&mut self, rot: M3) -> u32 {
        self.rot.push(rot);
        self.back.push(back_of(&rot));
        self.rot.len() as u32 - 1
    }

    /// The cell of crust at a place of a frame's lattice.
    pub(super) fn get(&self, frame: u32, at: u32) -> Option<usize> {
        self.slot.get(&slot_key(frame, at)).map(|&i| i as usize)
    }

    /// Where a cell of crust is on the planet now.
    pub(super) fn place(&mut self, sphere: &Sphere, i: usize) {
        let c = &mut self.cells[i];
        c.pos = normalize(apply(
            &self.rot[c.frame as usize],
            sphere.centre[c.at as usize],
        ));
        c.cell = sphere.cell_of(c.pos) as u32;
        let (frame, q) = (c.frame, c.cell as usize);
        if !self.near[q].contains(&frame) {
            self.near[q].push(frame);
        }
        for &(m, _) in &sphere.links[q] {
            let list = &mut self.near[m as usize];
            if !list.contains(&frame) {
                list.push(frame);
            }
        }
    }

    /// New crust at a free place of a frame's lattice.
    pub(super) fn add(
        &mut self,
        sphere: &Sphere,
        (frame, at): (u32, u32),
        plate: u8,
        rock: Rock,
        id: u32,
    ) -> usize {
        let i = self.cells.len();
        self.cells.push(Crust {
            frame,
            at,
            plate,
            area: sphere.area_km2[at as usize],
            rock,
            id,
            event: Margin::Inland as u8,
            under: false,
            rifted: false,
            switch: 0,
            shown: false,
            dead: false,
            squeezed: 0,
            pos: [0; 3],
            cell: 0,
        });
        self.slot.insert(slot_key(frame, at), i as u32);
        self.place(sphere, i);
        i
    }

    /// A cell of crust gone (run under another and piled on it).
    pub(super) fn kill(&mut self, i: usize) {
        let c = &mut self.cells[i];
        c.dead = true;
        self.slot.remove(&slot_key(c.frame, c.at));
    }

    /// The neighbours of a cell of crust in its own lattice, and the
    /// distance to them, km: the crust it has always touched.
    pub(super) fn kin<'a>(
        &'a self,
        sphere: &'a Sphere,
        i: usize,
    ) -> impl Iterator<Item = (usize, u32)> + 'a {
        let c = &self.cells[i];
        sphere.links[c.at as usize]
            .iter()
            .filter_map(move |&(n, km)| self.get(c.frame, n).map(|j| (j, km)))
    }

    /// The crust over the point `y` of the grid's cell `q`, the first in
    /// precedence, leaving out the frame `but`: the lattice cell of each
    /// frame around that holds the point, exactly.
    pub(super) fn cover(
        &self,
        sphere: &Sphere,
        (y, q): (V3, usize),
        rank: &dyn Fn(u8, u8) -> Rank,
        but: u32,
    ) -> Option<usize> {
        let mut best: Option<(Rank, std::cmp::Reverse<u32>, usize)> = None;
        for &f in &self.near[q] {
            if f == but {
                continue;
            }
            let at = sphere.cell_of(normalize(apply(&self.back[f as usize], y)));
            if let Some(j) = self.get(f, at as u32) {
                let c = &self.cells[j];
                let k = (rank(c.plate, c.rock.class), std::cmp::Reverse(f), j);
                if best.is_none_or(|b| k > b) {
                    best = Some(k);
                }
            }
        }
        best.map(|b| b.2)
    }

    /// The plates turn, each frame with its cells' plate. A frame whose
    /// cells turn differently splits: each part keeps the lattice and
    /// the turn so far, and takes its own turn from now on. No cell
    /// changes.
    pub(super) fn turn(&mut self, sphere: &Sphere, turn: &[M3]) {
        let same: Vec<usize> = (0..turn.len())
            .map(|p| (0..=p).find(|&q| turn[q] == turn[p]).expect("itself"))
            .collect();
        let old = self.rot.len();
        let mut keeps = vec![usize::MAX; old];
        let mut forks: Vec<(u32, usize, u32)> = Vec::new();
        for i in 0..self.cells.len() {
            let c = self.cells[i];
            let (f, k) = (c.frame as usize, same[c.plate as usize]);
            if keeps[f] == usize::MAX {
                keeps[f] = k;
            }
            if keeps[f] == k {
                continue;
            }
            let to = if let Some(x) = forks.iter().find(|x| x.0 == c.frame && x.1 == k) {
                x.2
            } else {
                let to = self.frame(self.rot[f]);
                forks.push((c.frame, k, to));
                to
            };
            self.slot.remove(&slot_key(c.frame, c.at));
            self.slot.insert(slot_key(to, c.at), i as u32);
            self.cells[i].frame = to;
        }
        let turned = (0..old)
            .map(|f| (f, keeps[f]))
            .chain(forks.iter().map(|x| (x.2 as usize, x.1)));
        for (f, k) in turned {
            if k != usize::MAX && turn[k] != IDENTITY {
                self.rot[f] = steady(&times(&turn[k], &self.rot[f]));
                self.back[f] = back_of(&self.rot[f]);
            }
        }
        for list in &mut self.near {
            list.clear();
        }
        for i in 0..self.cells.len() {
            self.place(sphere, i);
        }
    }

    /// The cells gone leave the record: each cell's new index.
    pub(super) fn compact(&mut self) -> Vec<u32> {
        let mut new_ix = vec![NO_BIT; self.cells.len()];
        let mut kept = Vec::with_capacity(self.cells.len());
        for (i, c) in self.cells.iter().enumerate() {
            if !c.dead {
                new_ix[i] = kept.len() as u32;
                kept.push(*c);
            }
        }
        self.cells = kept;
        self.slot.clear();
        for (i, c) in self.cells.iter().enumerate() {
            self.slot.insert(slot_key(c.frame, c.at), i as u32);
        }
        new_ix
    }

    /// A many-source shortest walk through the crust, from cell to cell
    /// of a piece's own lattice, up to `reach` km, only into cells that
    /// `pass`: each cell's distance, km.
    pub(super) fn spread(
        &self,
        sphere: &Sphere,
        sources: &[usize],
        pass: impl Fn(usize) -> bool,
        reach: u32,
    ) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.cells.len()];
        let mut heap = std::collections::BinaryHeap::new();
        for &i in sources {
            if dist[i] == u32::MAX {
                dist[i] = 0;
                heap.push(std::cmp::Reverse((0_u32, i as u32)));
            }
        }
        while let Some(std::cmp::Reverse((d, i))) = heap.pop() {
            let i = i as usize;
            if d > dist[i] {
                continue;
            }
            for (j, km) in self.kin(sphere, i) {
                let nd = d + km;
                if pass(j) && nd <= reach && nd < dist[j] {
                    dist[j] = nd;
                    heap.push(std::cmp::Reverse((nd, j as u32)));
                }
            }
        }
        dist
    }

    /// The same walk from every source with no cell barred: each cell's
    /// distance, km, and the source it is nearest to (its index in
    /// `sources`).
    pub(super) fn spread_from(
        &self,
        sphere: &Sphere,
        sources: &[usize],
        reach: u32,
    ) -> (Vec<u32>, Vec<u32>) {
        let mut dist = vec![u32::MAX; self.cells.len()];
        let mut origin = vec![u32::MAX; self.cells.len()];
        let mut heap = std::collections::BinaryHeap::new();
        for (k, &i) in sources.iter().enumerate() {
            if dist[i] == u32::MAX {
                dist[i] = 0;
                origin[i] = k as u32;
                heap.push(std::cmp::Reverse((0_u32, i as u32)));
            }
        }
        while let Some(std::cmp::Reverse((d, i))) = heap.pop() {
            let i = i as usize;
            if d > dist[i] {
                continue;
            }
            for (j, km) in self.kin(sphere, i) {
                let nd = d + km;
                if nd <= reach && nd < dist[j] {
                    dist[j] = nd;
                    origin[j] = origin[i];
                    heap.push(std::cmp::Reverse((nd, j as u32)));
                }
            }
        }
        (dist, origin)
    }
    /// The other plates whose crust a cell touches: its neighbours in its
    /// own lattice, and the other pieces the grid draws around it (a
    /// neighbourhood query: who touches whom).
    pub(super) fn facing(&self, sphere: &Sphere, top: &[u32], i: usize) -> Vec<u8> {
        let c = &self.cells[i];
        let mut out: Vec<u8> = Vec::new();
        for (j, _) in self.kin(sphere, i) {
            let p = self.cells[j].plate;
            if p != c.plate && !out.contains(&p) {
                out.push(p);
            }
        }
        let q = c.cell as usize;
        for m in std::iter::once(q).chain(sphere.links[q].iter().map(|&(m, _)| m as usize)) {
            if top[m] == NO_BIT {
                continue;
            }
            let o = &self.cells[top[m] as usize];
            if o.frame != c.frame && o.plate != c.plate && !out.contains(&o.plate) {
                out.push(o.plate);
            }
        }
        out
    }

    /// The pieces as the history keeps them at a step (`floor`: each
    /// plate's mean floor birth, for the precedence).
    pub(super) fn pieces(&self, group: &[u8; MAXP], floor: &[i64; MAXP]) -> Vec<Piece> {
        let weight = self.weight();
        let mut index = vec![usize::MAX; self.rot.len()];
        let mut out: Vec<Piece> = Vec::new();
        for c in &self.cells {
            let f = c.frame as usize;
            if index[f] == usize::MAX {
                index[f] = out.len();
                out.push(Piece {
                    frame: c.frame,
                    turn: self.rot[f],
                    tiles: Vec::new(),
                });
            }
            out[index[f]].tiles.push(Tile {
                at: c.at,
                id: c.id,
                plate: group[c.plate as usize],
                own: c.plate,
                switch: c.switch,
                class: c.rock.class,
                under: c.under,
                margins: c.rock.margins,
                thick: c.rock.thick,
                high: c.rock.high(),
                silt: c.rock.silt,
                home: c.rock.home,
                area_km2: c.area,
                why: c.rock.why,
                rank: precedence(&weight, floor, c.plate, c.rock.class),
            });
        }
        out
    }

    /// The crust of other pieces lying against the cell of crust `i`:
    /// their cells at its place and around it (a docked terrane, a
    /// welded continent, the other side of a collision's front).
    pub(super) fn beside(&self, sphere: &Sphere, i: usize) -> Vec<usize> {
        let c = &self.cells[i];
        let around = c.cell as usize;
        let mut out = Vec::new();
        for &frame in &self.near[around] {
            if frame == c.frame {
                continue;
            }
            for m in
                std::iter::once(around).chain(sphere.links[around].iter().map(|l| l.0 as usize))
            {
                let at = sphere.cell_of(normalize(apply(
                    &self.back[frame as usize],
                    sphere.centre[m],
                )));
                if let Some(k) = self.get(frame, at as u32)
                    && !out.contains(&k)
                {
                    out.push(k);
                }
            }
        }
        out
    }

    /// Each plate's continental crust, km² (what no other piece covers).
    pub(super) fn weight(&self) -> [i64; MAXP] {
        let mut out = [0_i64; MAXP];
        for c in &self.cells {
            if !c.dead && !c.under {
                out[c.plate as usize] += i64::from(c.area);
            }
        }
        out
    }
}

/// The grid's view of the continents: each cell shows the crust over its
/// centre. A view only: every change is made in the pieces.
pub(super) fn paint(land: &Land, top: &[u32], plate: &mut [u8], rock: &mut [Rock]) {
    for q in 0..CELLS {
        if top[q] != NO_BIT {
            let c = &land.cells[top[q] as usize];
            plate[q] = c.plate;
            rock[q] = c.rock;
        }
    }
}

/// Where the plates' bookkeeping finds each cell of crust on the grid: a
/// cell of the grid that draws it, else the cell it lies in.
pub(super) fn seats(land: &Land, top: &[u32]) -> Vec<u32> {
    let mut seat: Vec<u32> = land.cells.iter().map(|c| c.cell).collect();
    for q in (0..CELLS).rev() {
        if top[q] != NO_BIT {
            seat[top[q] as usize] = q as u32;
        }
    }
    seat
}

/// The precedence of crust of a class on a plate: continental crust never
/// sinks, the heavier continent rides; of two ocean floors the younger.
pub(super) fn precedence(cont_of: &[i64; MAXP], floor: &[i64; MAXP], p: u8, class: u8) -> Rank {
    let q = i64::from(p);
    match class {
        CONTINENT => (3, cont_of[p as usize], -q),
        TERRANE => (2, cont_of[p as usize], -q),
        _ => (1, -floor[p as usize], -q),
    }
}
