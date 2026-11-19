//! The continent born once and kept (D28, 19 November 2026): a server
//! stores a world's continent the first time it is born and hands it
//! back at every later start, so the window is cut from it again with no
//! birth (6 to 13 s, 140 MB at its peak, for the vivarium's continent of
//! 1,024 cells a side).
//!
//! The simulation does no I/O: [`Continent::to_bytes`] and
//! [`Continent::from_bytes`] are what the server writes and reads, and
//! [`born_within`] runs a world's making with the continents the server
//! kept, and tells which it had to bear.

use std::cell::RefCell;

use super::climate::{Biome, Reach};
use super::relief::{Stroke, StrokeKind};
use super::{Continent, Grid};
use crate::geology::Rock;

/// What the bytes start with, and the version of their layout.
const MAGIC: &[u8; 8] = b"SICONT01";

/// The continents of a [`born_within`] scope: those given, those born.
#[derive(Default)]
struct Scope {
    known: Vec<Continent>,
    born: Vec<Continent>,
}

thread_local! {
    static SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

/// Runs `f` with the continents `known` born already: a window cut
/// meanwhile on this thread from the continent of one of their seeds and
/// sides reads it instead of bearing it again. Gives back what `f`
/// gives, and the continents it had to bear (to keep), each once.
pub fn born_within<R>(known: Vec<Continent>, f: impl FnOnce() -> R) -> (R, Vec<Continent>) {
    let outer = SCOPE.with_borrow_mut(|s| {
        s.replace(Scope {
            known,
            born: Vec::new(),
        })
    });
    let out = f();
    let scope = SCOPE.with_borrow_mut(|s| std::mem::replace(s, outer));
    (out, scope.expect("the scope set above").born)
}

/// `f` of the continent of `seed`, `side` cells a side: one the scope
/// knows or bore already, else born now (and kept by the scope, if any).
pub(crate) fn with_continent<R>(seed: u64, side: usize, f: impl FnOnce(&Continent) -> R) -> R {
    let matches = |c: &Continent| c.seed == seed && c.grid.side == side;
    // Taken out of the scope while `f` runs, so `f` may open its own.
    let taken = SCOPE.with_borrow_mut(|s| {
        let s = s.as_mut()?;
        if let Some(k) = s.known.iter().position(matches) {
            return Some((s.known.swap_remove(k), false));
        }
        let k = s.born.iter().position(matches)?;
        Some((s.born.swap_remove(k), true))
    });
    let (continent, born) = match taken {
        Some(found) => found,
        None => (Continent::with(seed, side, None), true),
    };
    let out = f(&continent);
    SCOPE.with_borrow_mut(|s| {
        if let Some(s) = s.as_mut() {
            if born {
                s.born.push(continent);
            } else {
                s.known.push(continent);
            }
        }
    });
    out
}

/// Little-endian writing.
struct Out(Vec<u8>);

impl Out {
    fn u8s(&mut self, v: impl IntoIterator<Item = u8>) {
        self.0.extend(v);
    }
    /// Each of `v` as `f` writes it.
    fn all<T: Copy, const N: usize>(&mut self, v: &[T], f: fn(T) -> [u8; N]) {
        for &x in v {
            self.0.extend_from_slice(&f(x));
        }
    }
}

/// Little-endian reading; `None` past the end.
struct In<'a>(&'a [u8]);

impl In<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }
    fn many<T, const N: usize>(&mut self, n: usize, f: fn([u8; N]) -> T) -> Option<Vec<T>> {
        let bytes = self.take(n.checked_mul(N)?)?;
        Some(bytes.as_chunks::<N>().0.iter().map(|b| f(*b)).collect())
    }
    fn one<T, const N: usize>(&mut self, f: fn([u8; N]) -> T) -> Option<T> {
        self.many(1, f)?.pop()
    }
    /// `n` bytes, each one of `all` by its rank.
    fn ranks<T: Copy>(&mut self, n: usize, all: &[T]) -> Option<Vec<T>> {
        self.take(n)?
            .iter()
            .map(|&b| all.get(usize::from(b)).copied())
            .collect()
    }
    fn bools(&mut self, n: usize) -> Option<Vec<bool>> {
        self.ranks(n, &[false, true])
    }
}

/// Every biome by its rank.
const BIOMES: [Biome; 13] = [
    Biome::Sea,
    Biome::Lake,
    Biome::Desert,
    Biome::Steppe,
    Biome::WoodedSteppe,
    Biome::Grassland,
    Biome::Broadleaf,
    Biome::Jungle,
    Biome::MixedForest,
    Biome::Conifer,
    Biome::Marsh,
    Biome::Tundra,
    Biome::Alpine,
];

/// Every reach by its rank.
const REACHES: [Reach; 4] = [Reach::Single, Reach::Bar, Reach::Braided, Reach::Sill];

/// Every stroke kind by its rank.
const STROKES: [StrokeKind; 3] = [StrokeKind::Range, StrokeKind::Plateau, StrokeKind::Basin];

/// The rank of `v` in `all`.
fn rank<T: PartialEq>(all: &[T], v: &T) -> u8 {
    u8::try_from(all.iter().position(|a| a == v).expect("listed")).expect("few")
}

/// A checksum of `bytes` (FNV-1a), so a damaged copy is refused.
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl Continent {
    /// Everything the continent holds, as bytes: [`Continent::from_bytes`]
    /// gives it back whole. About 57 bytes a cell: 60 MB for the
    /// vivarium's continent of 1,024 cells a side.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Out(Vec::with_capacity(self.grid.len() * 57 + 64));
        out.u8s(*MAGIC);
        out.all(&[self.seed, self.grid.side as u64], u64::to_le_bytes);
        out.all(&[self.north_latitude], i32::to_le_bytes);
        out.u8s(self.sea.iter().map(|&b| u8::from(b)));
        out.all(&self.height, i32::to_le_bytes);
        out.all(&self.surface, i32::to_le_bytes);
        out.u8s(self.lake.iter().map(|&b| u8::from(b)));
        out.all(&self.receiver, u32::to_le_bytes);
        out.all(&self.order, u32::to_le_bytes);
        out.all(&self.uplift, i32::to_le_bytes);
        out.all(&self.exhumed, i32::to_le_bytes);
        out.u8s(self.rock.iter().map(|r| rank(&Rock::ALL, r)));
        out.u8s(self.alluvium.iter().map(|&b| u8::from(b)));
        out.all(&self.water_table, i32::to_le_bytes);
        out.u8s(self.spring.iter().map(|&b| u8::from(b)));
        out.u8s(self.reach.iter().map(|r| rank(&REACHES, r)));
        out.all(&self.spread, u16::to_le_bytes);
        out.all(&self.low_discharge, u64::to_le_bytes);
        out.all(&self.temperature, i16::to_le_bytes);
        out.all(&self.rain, u16::to_le_bytes);
        out.all(&self.discharge, u64::to_le_bytes);
        out.u8s(self.biome.iter().map(|b| rank(&BIOMES, b)));
        out.all(
            &[u32::try_from(self.strokes.len()).expect("few strokes")],
            u32::to_le_bytes,
        );
        for s in &self.strokes {
            out.u8s([rank(&STROKES, &s.kind)]);
            out.all(&[s.half_width, s.strength], i32::to_le_bytes);
            out.all(
                &[u32::try_from(s.points.len()).expect("few points")],
                u32::to_le_bytes,
            );
            for &(x, y) in &s.points {
                out.all(&[x, y], i32::to_le_bytes);
            }
        }
        let sum = checksum(&out.0);
        out.all(&[sum], u64::to_le_bytes);
        out.0
    }

    /// The continent [`Continent::to_bytes`] wrote; `None` when the bytes
    /// are not one whole and undamaged.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let (body, sum) = bytes.split_at_checked(bytes.len().checked_sub(8)?)?;
        if checksum(body) != u64::from_le_bytes(sum.try_into().ok()?) {
            return None;
        }
        let mut r = In(body);
        if r.take(MAGIC.len())? != MAGIC {
            return None;
        }
        let seed = r.one(u64::from_le_bytes)?;
        let side = usize::try_from(r.one(u64::from_le_bytes)?).ok()?;
        if !(64..=8192).contains(&side) {
            return None;
        }
        let grid = Grid { side };
        let n = grid.len();
        let north_latitude = r.one(i32::from_le_bytes)?;
        let sea = r.bools(n)?;
        let height = r.many(n, i32::from_le_bytes)?;
        let surface = r.many(n, i32::from_le_bytes)?;
        let lake = r.bools(n)?;
        let receiver = r.many(n, u32::from_le_bytes)?;
        let order = r.many(n, u32::from_le_bytes)?;
        let uplift = r.many(n, i32::from_le_bytes)?;
        let exhumed = r.many(n, i32::from_le_bytes)?;
        let rock = r.ranks(n, &Rock::ALL)?;
        let alluvium = r.bools(n)?;
        let water_table = r.many(n, i32::from_le_bytes)?;
        let spring = r.bools(n)?;
        let reach = r.ranks(n, &REACHES)?;
        let spread = r.many(n, u16::from_le_bytes)?;
        let low_discharge = r.many(n, u64::from_le_bytes)?;
        let temperature = r.many(n, i16::from_le_bytes)?;
        let rain = r.many(n, u16::from_le_bytes)?;
        let discharge = r.many(n, u64::from_le_bytes)?;
        let biome = r.ranks(n, &BIOMES)?;
        let mut strokes = Vec::new();
        for _ in 0..r.one(u32::from_le_bytes)? {
            let kind = *STROKES.get(usize::from(r.take(1)?[0]))?;
            let half_width = r.one(i32::from_le_bytes)?;
            let strength = r.one(i32::from_le_bytes)?;
            let points = usize::try_from(r.one(u32::from_le_bytes)?).ok()?;
            let flat = r.many(points.checked_mul(2)?, i32::from_le_bytes)?;
            strokes.push(Stroke {
                kind,
                points: flat
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| (p[0], p[1]))
                    .collect(),
                half_width,
                strength,
            });
        }
        let in_range = receiver.iter().chain(&order).all(|&i| (i as usize) < n);
        (r.0.is_empty() && in_range).then_some(Self {
            seed,
            grid,
            north_latitude,
            sea,
            height,
            surface,
            lake,
            receiver,
            order,
            uplift,
            exhumed,
            rock,
            alluvium,
            water_table,
            spring,
            reach,
            spread,
            low_discharge,
            temperature,
            rain,
            discharge,
            biome,
            strokes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_continent_kept_as_bytes_comes_back_whole() {
        let born = Continent::with(5, 128, None);
        let bytes = born.to_bytes();
        let back = Continent::from_bytes(&bytes).expect("whole bytes");
        assert_eq!(back.fingerprint(), born.fingerprint());
        assert_eq!(back.to_bytes(), bytes, "every field, strokes included");
        assert_ne!(born.strokes.len(), 0);
        // Damaged, cut short or lengthened: refused.
        let mut damaged = bytes.clone();
        damaged[bytes.len() / 2] ^= 1;
        assert!(Continent::from_bytes(&damaged).is_none());
        assert!(Continent::from_bytes(&bytes[..bytes.len() - 1]).is_none());
        assert!(Continent::from_bytes(&bytes[..10]).is_none());
        assert!(Continent::from_bytes(&[]).is_none());
        let mut longer = bytes[..bytes.len() - 8].to_vec();
        longer.push(0);
        let sum = checksum(&longer);
        longer.extend_from_slice(&sum.to_le_bytes());
        assert!(Continent::from_bytes(&longer).is_none());
    }

    #[test]
    fn a_scope_bears_each_continent_once_and_reads_those_it_knows() {
        let side = 64;
        // No scope: born each time, nothing kept.
        let print = with_continent(3, side, Continent::fingerprint);
        assert_eq!(print, Continent::with(3, side, None).fingerprint());
        // A scope bears a continent once, however often it is read.
        let (prints, born) = born_within(Vec::new(), || {
            [3, 3, 4].map(|seed| with_continent(seed, side, Continent::fingerprint))
        });
        assert_eq!(prints[0], print);
        assert_eq!(prints[0], prints[1]);
        let mut seeds: Vec<u64> = born.iter().map(|c| c.seed).collect();
        seeds.sort_unstable();
        assert_eq!(seeds, [3, 4]);
        // A continent known (here, a copy read back from its bytes) is
        // read, not born again.
        let known = Continent::from_bytes(&born[0].to_bytes()).expect("whole");
        let seed = known.seed;
        let (again, born) = born_within(vec![known], || {
            with_continent(seed, side, Continent::fingerprint)
        });
        assert!(born.is_empty());
        assert_eq!(
            again,
            Continent::with(seed, side, None).fingerprint(),
            "the same continent"
        );
        // Out of the scope, nothing is left behind.
        assert!(SCOPE.with_borrow(Option::is_none));
    }
}
