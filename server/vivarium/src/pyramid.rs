//! The start with an age pyramid (the life lot, approved by the project
//! lead on 25 November 2026: "C'est très très bien ce qu'ils ont suggéré.
//! Je suis partant, on y va.", on the audit of life and population). The
//! founders' start (lot 3 C) bore grown people of 18 to 40 alone, all wed
//! the first evening, every reserve full: some fifty births in the first
//! cycle, fourteen of them on day 8, then no fertile woman between the
//! founders' fortieth year and their daughters' sixteenth. A band of
//! foragers is not born so. Here a camp is born as one: children and
//! infants with their mothers, adults of every age, a few old, couples
//! close in age, some unwed, reserves unequal, a few women with child.
//!
//! **The reference band** ([`ROSTER`]) is a band of 25 shaped on the
//! forager censuses (orders of magnitude, from the literature: Howell 1979
//! and Lee 1979 for the Dobe !Kung, Blurton Jones 2016 and Marlowe 2010 for
//! the Hadza): some 40 to 45 % under fifteen, some 8 to 10 % past sixty,
//! the rest spread between, women and men in near equal numbers. A camp of
//! another size takes the roster's members at even quantiles (a camp of 50
//! takes each twice, a camp of 5 one in five), so the shape holds; under
//! ten or so the band is too thin to keep it.
//!
//! **Kinship**: a woman from twenty weds the free grown man nearest three
//! years older than her, ten years either side at most (forager husbands
//! are a few years older than their wives); the youngest grown stay unwed
//! and court by the daily rule (D49). Each child is given a mother whose
//! age at its birth was in the fertile years, whose other children are
//! three years or more from it (a birth every three to four years among
//! the !Kung and the Hadza), the wed first, then the one with the fewest
//! children, then the one nearest twenty-six at its birth: her spouse is
//! its father. An infant is carried and nursed by its mother (D47); one
//! with no possible mother is not born. The grown are given no parents:
//! the band remembers none.
//!
//! **Reserves** are drawn evenly between [`RESERVE_PERCENT`] of full for
//! each body but the infants (who are nursed): the band is born at the
//! start of spring, after a winter. **With child**: of the wed women of the
//! fertile years who nurse no infant, three in eight (rounded) are with
//! child, some days into it (a birth every four years, nursing two of them
//! and nine months with child, leaves a woman who nurses nobody with child
//! some three times in eight).
//!
//! Every draw is from the run's seed, the tribe and the member (D24).

use sim::Sex;
use sim::content::Content;

/// The reference band of 25, youngest first: its ages in tenths of an
/// age-year, and sexes. 11 under sixteen (44 %), two of them infants;
/// 12 from sixteen to fifty-nine; 2 past sixty (8 %); 13 women, 12 men.
pub const ROSTER: [(u32, Sex); 25] = [
    (5, Sex::Female),
    (15, Sex::Male),
    (30, Sex::Female),
    (40, Sex::Male),
    (60, Sex::Female),
    (80, Sex::Male),
    (90, Sex::Female),
    (110, Sex::Male),
    (120, Sex::Female),
    (130, Sex::Male),
    (150, Sex::Female),
    (180, Sex::Female),
    (200, Sex::Male),
    (230, Sex::Female),
    (260, Sex::Male),
    (280, Sex::Female),
    (310, Sex::Male),
    (330, Sex::Female),
    (360, Sex::Male),
    (390, Sex::Female),
    (430, Sex::Male),
    (470, Sex::Female),
    (540, Sex::Male),
    (630, Sex::Female),
    (670, Sex::Male),
];

/// The share of a full reserve each body is born with, drawn evenly
/// between the two, in per cent.
pub const RESERVE_PERCENT: (u32, u32) = (45, 90);
/// A woman weds from this age, in tenths of an age-year.
const WED_FROM_TENTHS: u32 = 200;
/// Her husband is nearest this many tenths older than her…
const HUSBAND_OLDER_TENTHS: u32 = 30;
/// …and at most this many tenths from that.
const COUPLE_WITHIN_TENTHS: u32 = 100;
/// Grown, in tenths: who may be wed or be a mother.
const GROWN_TENTHS: u32 = 160;
/// Two children of one mother are at least this many tenths apart.
const SPACING_TENTHS: u32 = 30;
/// The mother's age at a child's birth looked for first, in tenths.
const MOTHER_AGE_TENTHS: u32 = 260;
/// Of the wed fertile women nursing no infant, this many eighths are with
/// child (rounded).
const WITH_CHILD_EIGHTHS: usize = 3;

/// The salts of the draws, one a kind.
const SALT_RESERVE: u64 = 0x7e5e_47e5;
const SALT_WITH_CHILD: u64 = 0xc417_d000;
const SALT_DAYS: u64 = 0xda75_0001;

/// One member of a camp's band at its start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Its age, in tenths of an age-year.
    pub age_tenths: u32,
    /// Its sex.
    pub sex: Sex,
    /// Its spouse, by index in the band.
    pub spouse: Option<usize>,
    /// Its mother, by index in the band (its father is her spouse).
    pub mother: Option<usize>,
    /// Its fat reserve at the start, in per cent of full.
    pub reserve_percent: u32,
    /// With child for that many days already, for a woman with child.
    pub with_child_days: Option<u32>,
}

impl Member {
    /// Whether it is an infant, carried and nursed (D47).
    #[must_use]
    pub fn infant(&self, content: &Content) -> bool {
        content
            .needs
            .ages
            .is_infant(u64::from(self.age_tenths) * 100)
    }
}

/// A draw from the seed, the tribe, a kind and a member: a mix of the
/// four (`SplitMix64`'s finaliser), integers only (D24).
fn draw(seed: u64, tribe: u8, salt: u64, k: usize) -> u64 {
    let mut z = seed
        ^ salt.rotate_left(17)
        ^ (u64::from(tribe) + 1).wrapping_mul(0xd1b5_4a32_d192_ed03)
        ^ (k as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The band of `people` born at tribe `tribe`'s camp, youngest first,
/// with its couples, mothers, reserves and women with child (the module's
/// rules), from the run's seed. `days_per_age_year` sets how long a
/// pregnancy is, in days.
#[must_use]
pub fn plan(
    people: u32,
    seed: u64,
    tribe: u8,
    content: &Content,
    days_per_age_year: u32,
) -> Vec<Member> {
    let n = people as usize;
    let life = &content.needs.life;
    let mut band: Vec<Member> = (0..n)
        .map(|k| {
            let (age_tenths, sex) = ROSTER[((2 * k + 1) * ROSTER.len() / (2 * n)).min(24)];
            Member {
                age_tenths,
                sex,
                spouse: None,
                mother: None,
                reserve_percent: 100,
                with_child_days: None,
            }
        })
        .collect();
    // The couples: each woman from twenty, the youngest first, takes the
    // free grown man nearest three years older than her.
    for w in 0..n {
        if band[w].sex != Sex::Female || band[w].age_tenths < WED_FROM_TENTHS {
            continue;
        }
        let target = band[w].age_tenths + HUSBAND_OLDER_TENTHS;
        let man = (0..n)
            .filter(|&m| {
                band[m].sex == Sex::Male
                    && band[m].spouse.is_none()
                    && band[m].age_tenths >= GROWN_TENTHS
                    && band[m].age_tenths.abs_diff(target) <= COUPLE_WITHIN_TENTHS
            })
            .min_by_key(|&m| (band[m].age_tenths.abs_diff(target), m));
        if let Some(m) = man {
            band[w].spouse = Some(m);
            band[m].spouse = Some(w);
        }
    }
    // The mothers: the infants first, the youngest first (each nursed by
    // its own), then the children, the eldest first.
    let infant_tenths = content.needs.ages.weaned_years * 10;
    let mut young: Vec<usize> = (0..n)
        .filter(|&c| band[c].age_tenths < infant_tenths)
        .collect();
    young.extend(
        (0..n)
            .rev()
            .filter(|&c| (infant_tenths..GROWN_TENTHS).contains(&band[c].age_tenths)),
    );
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for c in young {
        let age = band[c].age_tenths;
        let mother = (0..n)
            .filter(|&w| {
                band[w].sex == Sex::Female
                    && band[w].age_tenths >= GROWN_TENTHS
                    && band[w].age_tenths > age
                    && life.is_fertile(u64::from(band[w].age_tenths - age) * 100)
                    && children[w]
                        .iter()
                        .all(|&o| band[o].age_tenths.abs_diff(age) >= SPACING_TENTHS)
            })
            .min_by_key(|&w| {
                (
                    band[w].spouse.is_none(),
                    children[w].len(),
                    (band[w].age_tenths - age).abs_diff(MOTHER_AGE_TENTHS),
                    w,
                )
            });
        if let Some(w) = mother {
            band[c].mother = Some(w);
            children[w].push(c);
        }
    }
    // The reserves, unequal; the infants nursed, full.
    let (lo, hi) = RESERVE_PERCENT;
    for (k, member) in band.iter_mut().enumerate() {
        if !member.infant(content) {
            let span = u64::from(hi - lo + 1);
            member.reserve_percent = lo
                + u32::try_from(draw(seed, tribe, SALT_RESERVE, k) % span)
                    .expect("under a hundred");
        }
    }
    // The women with child: of the wed fertile women nursing no infant,
    // three in eight, drawn; each some days into it.
    let mut open: Vec<usize> = (0..n)
        .filter(|&w| {
            band[w].sex == Sex::Female
                && band[w].spouse.is_some()
                && life.is_fertile(u64::from(band[w].age_tenths) * 100)
                && !children[w].iter().any(|&c| band[c].infant(content))
        })
        .collect();
    open.sort_by_key(|&w| (draw(seed, tribe, SALT_WITH_CHILD, w), w));
    let days = u64::from(life.pregnancy_quarters) * u64::from(days_per_age_year) / 4;
    let count = (open.len() * WITH_CHILD_EIGHTHS + 4) / 8;
    for &w in open.iter().take(count) {
        let into = draw(seed, tribe, SALT_DAYS, w) % days.max(1);
        band[w].with_child_days = Some(u32::try_from(into).expect("a few days"));
    }
    band
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn content() -> Content {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
        crate::load_content(&dir).expect("the repository's content")
    }

    fn years(m: &Member) -> u32 {
        m.age_tenths / 10
    }

    #[test]
    fn a_band_of_25_is_the_reference_pyramid() {
        let content = content();
        let band = plan(25, 1, 0, &content, 8);
        let ages: Vec<u32> = band.iter().map(|m| m.age_tenths).collect();
        let roster: Vec<u32> = ROSTER.iter().map(|r| r.0).collect();
        assert_eq!(ages, roster);
        let under_16 = band.iter().filter(|m| years(m) < 16).count();
        let old = band.iter().filter(|m| years(m) >= 60).count();
        let infants = band.iter().filter(|m| m.infant(&content)).count();
        assert_eq!((under_16, infants, old), (11, 2, 2));
        let women = band.iter().filter(|m| m.sex == Sex::Female).count();
        assert_eq!(women, 13);
    }

    #[test]
    fn couples_are_close_in_age_and_the_youngest_grown_unwed() {
        let content = content();
        let band = plan(25, 1, 0, &content, 8);
        let mut couples: Vec<(u32, u32)> = band
            .iter()
            .filter(|m| m.sex == Sex::Female)
            .filter_map(|m| Some((years(m), years(&band[m.spouse?]))))
            .collect();
        couples.sort_unstable();
        assert_eq!(
            couples,
            vec![(23, 26), (28, 31), (33, 36), (39, 43), (47, 54), (63, 67)]
        );
        for m in &band {
            if let Some(s) = m.spouse {
                assert_eq!(
                    band[s].spouse,
                    Some(band.iter().position(|x| x == m).unwrap())
                );
            }
        }
        let unwed: Vec<u32> = band
            .iter()
            .filter(|m| years(m) >= 16 && m.spouse.is_none())
            .map(years)
            .collect();
        assert_eq!(unwed, vec![18, 20], "the youngest woman and man");
    }

    #[test]
    fn every_child_has_a_plausible_mother_and_her_husband_as_father() {
        let content = content();
        let life = &content.needs.life;
        let band = plan(25, 1, 0, &content, 8);
        for (c, child) in band.iter().enumerate().filter(|(_, m)| years(m) < 16) {
            let w = child.mother.expect("every child of the band has a mother");
            let mother = &band[w];
            assert_eq!(mother.sex, Sex::Female);
            assert!(mother.spouse.is_some(), "a wed mother, a father");
            let at_birth = mother.age_tenths - child.age_tenths;
            assert!(life.is_fertile(u64::from(at_birth) * 100), "{at_birth}");
            for (o, other) in band.iter().enumerate() {
                if o != c && other.mother == Some(w) {
                    assert!(other.age_tenths.abs_diff(child.age_tenths) >= SPACING_TENTHS);
                }
            }
        }
        let infants: Vec<usize> = (0..band.len())
            .filter(|&c| band[c].infant(&content))
            .collect();
        let mothers: Vec<usize> = infants.iter().map(|&c| band[c].mother.unwrap()).collect();
        assert_eq!(mothers.len(), 2);
        assert_ne!(mothers[0], mothers[1], "one infant in a woman's arms");
        assert!(
            band.iter()
                .filter(|m| years(m) >= 16)
                .all(|m| m.mother.is_none())
        );
    }

    #[test]
    fn reserves_are_unequal_few_women_are_with_child_and_the_seed_sets_both() {
        let content = content();
        let band = plan(25, 1, 0, &content, 8);
        let (lo, hi) = RESERVE_PERCENT;
        let grown: Vec<u32> = band
            .iter()
            .filter(|m| !m.infant(&content))
            .map(|m| m.reserve_percent)
            .collect();
        assert!(grown.iter().all(|r| (lo..=hi).contains(r)));
        assert!(grown.iter().min() < grown.iter().max(), "unequal");
        assert!(
            band.iter()
                .filter(|m| m.infant(&content))
                .all(|m| m.reserve_percent == 100)
        );
        // Wed fertile women nursing no infant: those of 33 and 39; three
        // eighths of two, rounded: one with child, under a pregnancy's
        // six days into it.
        let with_child: Vec<&Member> = band
            .iter()
            .filter(|m| m.with_child_days.is_some())
            .collect();
        assert_eq!(with_child.len(), 1);
        assert!([33, 39].contains(&years(with_child[0])));
        assert!(with_child[0].with_child_days.unwrap() < 6);
        // The same seed, the same band; the other tribe or seed, other draws.
        assert_eq!(plan(25, 1, 0, &content, 8), band);
        let reserves = |b: &[Member]| b.iter().map(|m| m.reserve_percent).collect::<Vec<_>>();
        assert_ne!(reserves(&plan(25, 1, 1, &content, 8)), reserves(&band));
        assert_ne!(reserves(&plan(25, 2, 0, &content, 8)), reserves(&band));
    }

    #[test]
    fn another_size_keeps_the_shape() {
        let content = content();
        let band = plan(50, 3, 0, &content, 8);
        assert_eq!(band.len(), 50);
        let under_16 = band.iter().filter(|m| years(m) < 16).count();
        assert_eq!(under_16, 22, "each of the reference twice");
        assert!(
            band.iter()
                .filter(|m| years(m) < 16)
                .all(|m| m.mother.is_some())
        );
        let small = plan(5, 3, 0, &content, 8);
        let ages: Vec<u32> = small.iter().map(years).collect();
        assert_eq!(ages, vec![3, 11, 20, 33, 54]);
    }
}
