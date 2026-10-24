//! The world recipe: how common each element is, set by the player at the elements scale and
//! carried up. Chemistry gets it as its atom mix; every sea above (pond, cells, coast, reef)
//! gets each letter's and the backbone's share against the default mix, so an element made
//! scarce stays scarce all the way up.
//!
//! The total stays fixed: making one element common makes the rest rarer, as in a real cloud
//! where every atom counted once.
use crate::cell::CellParams;
use crate::chem::default_composition;
use crate::coast::CoastParams;
use crate::pool::PoolParams;
use crate::protocell::PondParams;
use crate::reef::ReefParams;
use crate::element::{BACKBONE, N_ELEMENTS};
use crate::polymer::{LETTERS, N_LETTERS};

/// Each element's abundance against the default mix: x1/10 .. x10 (real stellar mixes vary
/// about that much for these light elements; outside it the higher scales' learned tables have
/// never seen the chemistry).
pub const MULT_MIN: f64 = 0.1;
pub const MULT_MAX: f64 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// Multiplier per element (index Z - 1) on its default abundance.
    pub mult: [f64; N_ELEMENTS],
}

impl Default for Recipe {
    fn default() -> Self {
        Self { mult: [1.0; N_ELEMENTS] }
    }
}

impl Recipe {
    pub fn new(mult: &[f64]) -> Self {
        Self { mult: std::array::from_fn(|i| mult.get(i).copied().filter(|v| v.is_finite()).unwrap_or(1.0).clamp(MULT_MIN, MULT_MAX)) }
    }

    /// Relative abundance per element, summing to 1: what chemistry seeds its atoms from.
    pub fn composition(&self) -> [f64; N_ELEMENTS] {
        let d = default_composition();
        let raw: [f64; N_ELEMENTS] = std::array::from_fn(|i| d[i] * self.mult[i]);
        let sum: f64 = raw.iter().sum();
        raw.map(|v| v / sum)
    }

    /// Share of element `z` against its default share (1 = unchanged).
    pub fn share(&self, z: u8) -> f64 {
        let d = default_composition();
        let dsum: f64 = d.iter().sum();
        let c = self.composition();
        let i = z as usize - 1;
        c[i] / (d[i] / dsum)
    }

    /// Sea strength per letter (CellParams::sea and friends).
    pub fn sea(&self) -> [f64; N_LETTERS] {
        std::array::from_fn(|l| self.share(LETTERS[l]))
    }

    /// Sea strength of the backbone atom (multiplies `ocean_ka`).
    pub fn backbone(&self) -> f64 {
        self.share(BACKBONE)
    }

    /// The pool's starting water: loose monomers and raw atoms by letter, backbone atoms.
    pub fn apply_pool(&self, p: &mut PoolParams) {
        let sea = self.sea();
        for l in 0..N_LETTERS {
            p.monomers[l] = (p.monomers[l] as f64 * sea[l]).round() as i64;
            p.raw[l] = (p.raw[l] as f64 * sea[l]).round() as i64;
        }
        p.raw_ka = (p.raw_ka as f64 * self.backbone()).round() as i64;
    }

    pub fn apply_pond(&self, p: &mut PondParams) {
        p.sea = self.sea();
        p.ocean_ka *= self.backbone();
    }

    pub fn apply_cell(&self, p: &mut CellParams) {
        p.sea = self.sea();
        p.ocean_ka *= self.backbone();
    }

    pub fn apply_coast(&self, p: &mut CoastParams) {
        p.sea = self.sea();
        p.sea_ka = self.backbone();
    }

    pub fn apply_reef(&self, p: &mut ReefParams) {
        p.sea = self.sea();
        p.ocean_ka *= self.backbone();
    }

    pub fn is_default(&self) -> bool {
        self.mult.iter().all(|&m| (m - 1.0).abs() < 1e-9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_recipe_is_the_default_sea() {
        let r = Recipe::default();
        assert!(r.sea().iter().all(|&s| (s - 1.0).abs() < 1e-12));
        assert!((r.backbone() - 1.0).abs() < 1e-12);
    }

    /// Total fixed: one letter made common thins every other element.
    #[test]
    fn one_element_up_thins_the_rest() {
        let mut m = [1.0; N_ELEMENTS];
        m[LETTERS[0] as usize - 1] = 4.0;
        let r = Recipe::new(&m);
        let sea = r.sea();
        assert!(sea[0] > 2.0);
        assert!(sea[1..].iter().all(|&s| s < 1.0) && r.backbone() < 1.0);
        assert!((r.composition().iter().sum::<f64>() - 1.0).abs() < 1e-12);
    }
}

#[cfg(test)]
mod sea_tests {
    use super::*;
    use crate::cosmos::{Cosmos, CosmosParams};
    use crate::reef::{Reef, ReefSeed};
    use crate::surrogate::RateTable;

    fn load() -> (RateTable, ReefSeed) {
        let dir = format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), crate::element::data_dir());
        (serde_json::from_str(&std::fs::read_to_string(format!("{dir}rates0.json")).unwrap()).unwrap(), serde_json::from_str(&std::fs::read_to_string(format!("{dir}reef_seed.json")).unwrap()).unwrap())
    }

    /// The thinnest sea a recipe allows (one letter at the floor, the rest at the ceiling): cells
    /// and reef found what they can afford, no reservoir goes negative, matter stays exact.
    #[test]
    fn thinnest_sea_keeps_ledgers() {
        let (rates, rs) = load();
        let mut m = [MULT_MAX; N_ELEMENTS];
        m[LETTERS[0] as usize - 1] = MULT_MIN;
        let r = Recipe::new(&m);
        let mut cp = CellParams::default();
        r.apply_cell(&mut cp);
        let mut w = crate::cell::World::new(PoolParams::default(), cp, Cosmos::new(CosmosParams::default()), rates.clone(), 1, 200);
        let m0 = w.matter();
        for _ in 0..40 {
            w.step(0.02);
        }
        assert_eq!(w.matter(), m0);
        assert!(w.ocean.iter().all(|&v| v >= 0) && w.ocean_ka >= 0);
        let mut rp = ReefParams { width: 8, depth: 8, founders: 60, ..Default::default() };
        r.apply_reef(&mut rp);
        let mut reef = Reef::new(rp, CosmosParams::default(), rates, &rs, 2);
        assert!(reef.ocean.iter().all(|&v| v >= 0) && reef.ocean_ka >= 0);
        let m0 = reef.matter();
        for _ in 0..200 {
            reef.step(0.02);
        }
        assert_eq!(reef.matter(), m0);
        // release until the thin letter runs out: spores the sea cannot pay for are not planted
        let f = &rs.founders[0];
        let genome: Vec<u8> = f.genome.iter().map(|&l| reef.canon_letter(l)).collect();
        let mut refused = false;
        for _ in 0..400 {
            if reef.release(&genome, f.mass.map(|v| v.clamp(0, 40)), (0.0, 1.0), 50).is_none() {
                refused = true;
                break;
            }
        }
        assert!(refused, "the thin sea never ran out");
        assert!(reef.ocean.iter().all(|&v| v >= 0) && reef.ocean_ka >= 0);
        assert_eq!(reef.matter(), m0);
    }
}
