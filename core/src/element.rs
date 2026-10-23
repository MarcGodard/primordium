//! Scale -1 (subatomic): elements are not hand-tabled, they fall out of a shell rule.
//! Shell capacities differ from our universe ([2, 6] instead of [2, 8]), so the
//! best backbone atom is trivalent, not tetravalent: chains carry exactly one
//! side group per backbone atom, which is what makes a sequence readable.
//!
//! Energies are integer milli-units (1000 = kT at 300 K) so the energy ledger
//! balances exactly; floats only appear in acceptance probabilities.

use serde::Serialize;

pub const SHELLS: [u8; 2] = [2, 6];
pub const N_ELEMENTS: usize = 8;
pub const NAMES: [&str; N_ELEMENTS] = ["Hy", "Ne", "Li", "Bo", "Ka", "Ox", "Fl", "Ar"];

/// Energy unit: 1000 = kT at 300 K.
pub type Energy = i64;
pub const KT: f64 = 1000.0;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Element {
    pub z: u8,
    pub name: &'static str,
    /// 1-based outer shell index.
    pub shell: u8,
    pub outer: u8,
    pub capacity: u8,
    /// Free bonding slots: distance to the nearer closed shell.
    pub valence: u8,
    /// Screened charge over shell size squared. Pulls shared electrons.
    pub chi: f64,
    /// Visual and packing radius, lattice units (all atoms fit one cell).
    pub radius: f64,
    /// Preference for water neighbours; <0 hydrophobic. Polar atoms like water.
    pub hydropathy: f64,
    /// Visible-light absorption cross-section per bond made (pigment tendency).
    pub absorb: f64,
}

pub fn element(z: u8) -> Element {
    assert!((1..=N_ELEMENTS as u8).contains(&z));
    let mut left = z;
    let mut inner = 0u8;
    let mut shell = 1u8;
    for (i, &cap) in SHELLS.iter().enumerate() {
        if left <= cap {
            shell = i as u8 + 1;
            break;
        }
        left -= cap;
        inner += cap;
    }
    let capacity = SHELLS[shell as usize - 1];
    let outer = left;
    let valence = outer.min(capacity - outer);
    let z_eff = (z - inner) as f64;
    let n2 = (shell as f64).powi(2);
    let chi = z_eff / n2;
    let radius = 0.22 + 0.13 * shell as f64 - 0.02 * z_eff;
    let noble = valence == 0;
    let hydropathy = if noble { 0.0 } else { 1.2 * (chi - 0.75).abs() - 0.35 };
    let absorb = if valence >= 2 { 0.4 + 0.3 * valence as f64 } else { 0.05 };
    Element { z, name: NAMES[z as usize - 1], shell, outer, capacity, valence, chi, radius, hydropathy, absorb }
}

/// Lookup tables built once from the shell rule.
#[derive(Clone, Debug, Serialize)]
pub struct Table {
    pub el: Vec<Element>,
    /// Covalent bond energy [za][zb], milli-units, 0 if either is noble.
    pub bond: [[Energy; N_ELEMENTS + 1]; N_ELEMENTS + 1],
    /// Weak (pairing) bond energy: polarity mismatch only, no valence used.
    pub weak: [[Energy; N_ELEMENTS + 1]; N_ELEMENTS + 1],
}

/// Covalent: shared-pair term grows with both pulls; ionic term with mismatch.
pub const BOND_SHARED: f64 = 10.0;
pub const BOND_IONIC: f64 = 6.0;
pub const WEAK_SCALE: f64 = 2.4;

impl Table {
    pub fn new() -> Self {
        let mut el = vec![element(1)];
        el.clear();
        for z in 1..=N_ELEMENTS as u8 {
            el.push(element(z));
        }
        let mut bond = [[0; N_ELEMENTS + 1]; N_ELEMENTS + 1];
        let mut weak = [[0; N_ELEMENTS + 1]; N_ELEMENTS + 1];
        for a in &el {
            for b in &el {
                if a.valence == 0 || b.valence == 0 {
                    continue;
                }
                let e = BOND_SHARED * (a.chi * b.chi).sqrt() + BOND_IONIC * (a.chi - b.chi).powi(2);
                bond[a.z as usize][b.z as usize] = (e * KT).round() as Energy;
                let w = WEAK_SCALE * (a.chi - b.chi).powi(2).min(1.0);
                weak[a.z as usize][b.z as usize] = (w * KT).round() as Energy;
            }
        }
        Self { el, bond, weak }
    }

    #[inline]
    pub fn get(&self, z: u8) -> &Element {
        &self.el[z as usize - 1]
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_rule_gives_expected_valences() {
        let v: Vec<u8> = (1..=8).map(|z| element(z).valence).collect();
        assert_eq!(v, vec![1, 0, 1, 2, 3, 2, 1, 0]);
    }

    #[test]
    fn bonds_symmetric_and_nobles_inert() {
        let t = Table::new();
        for a in 1..=8 {
            for b in 1..=8 {
                assert_eq!(t.bond[a][b], t.bond[b][a]);
            }
            assert_eq!(t.bond[2][a], 0);
            assert_eq!(t.bond[8][a], 0);
        }
    }
}
