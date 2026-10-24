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
        Self { mult: std::array::from_fn(|i| mult.get(i).copied().unwrap_or(1.0).clamp(MULT_MIN, MULT_MAX)) }
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
