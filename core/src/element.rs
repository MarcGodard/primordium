//! Scale -1 (subatomic): elements are not hand-tabled, they fall out of a shell rule.
//! Shell capacities differ from our universe ([2, 6] instead of [2, 8]), so the
//! best backbone atom is trivalent, not tetravalent: chains carry exactly one
//! side group per backbone atom, which is what makes a sequence readable.
//!
//! Energies are integer milli-units (1000 = kT at 300 K) so the energy ledger
//! balances exactly; floats only appear in acceptance probabilities.

use serde::Serialize;

/// Shell capacities: the one number set that defines the periodic table. Adding a shell (say
/// `[2, 6, 6]`) grows the table; element count, bondable set, backbone atom and side-group
/// letters below all follow from it at compile time. `primordium elements --shells=2,6,6`
/// previews a table before switching. Scales 1+ still assume the 5-letter alphabet this
/// table gives (see the check in `polymer.rs`).
pub const SHELLS: [u8; 2] = [2, 6];
pub const N_ELEMENTS: usize = shell_total(&SHELLS);
pub const NAMES: [&str; N_ELEMENTS] = names();

/// Invented symbols in table order. The first 8 are this universe's; the rest wait for bigger shells.
pub const NAME_POOL: [&str; 40] = [
    "Hy", "Ne", "Li", "Bo", "Ka", "Ox", "Fl", "Ar", "Sa", "Mu", "Ti", "Ze", "Pa", "Qu", "Vo", "Ry", "Dn", "Ec", "Gu", "Ib", "Jo", "Lu", "Mi", "Nu", "Or", "Pe",
    "Ro", "Sy", "Ta", "Ul", "Vi", "Wa", "Xa", "Yo", "Zu", "Ax", "Br", "Cy", "Dr", "Ev",
];

const fn names() -> [&'static str; N_ELEMENTS] {
    assert!(N_ELEMENTS <= NAME_POOL.len(), "add symbols to NAME_POOL for the bigger table");
    let mut out = [""; N_ELEMENTS];
    let mut i = 0;
    while i < N_ELEMENTS {
        out[i] = NAME_POOL[i];
        i += 1;
    }
    out
}

pub const fn shell_total(shells: &[u8]) -> usize {
    let mut s = 0;
    let mut i = 0;
    while i < shells.len() {
        s += shells[i] as usize;
        i += 1;
    }
    s
}

/// (1-based shell, electrons in it, its capacity, electrons in closed shells below) for `z`.
pub const fn shell_of(shells: &[u8], z: u8) -> (u8, u8, u8, u8) {
    let mut left = z;
    let mut inner = 0u8;
    let mut i = 0;
    while i < shells.len() {
        if left <= shells[i] {
            return (i as u8 + 1, left, shells[i], inner);
        }
        left -= shells[i];
        inner += shells[i];
        i += 1;
    }
    panic!("z beyond the last shell")
}

/// Free bonding slots: distance to the nearer closed shell.
pub const fn valence_in(shells: &[u8], z: u8) -> u8 {
    let (_, outer, cap, _) = shell_of(shells, z);
    if outer < cap - outer { outer } else { cap - outer }
}

const fn count_bondable() -> usize {
    let mut n = 0;
    let mut z = 1;
    while z <= N_ELEMENTS {
        if valence_in(&SHELLS, z as u8) > 0 {
            n += 1;
        }
        z += 1;
    }
    n
}

/// Elements that bond (nobles excluded), as Z.
pub const N_BONDABLE: usize = count_bondable();
pub const BONDABLE_Z: [u8; N_BONDABLE] = bondable();

const fn bondable() -> [u8; N_BONDABLE] {
    let mut out = [0u8; N_BONDABLE];
    let (mut k, mut z) = (0, 1);
    while z <= N_ELEMENTS {
        if valence_in(&SHELLS, z as u8) > 0 {
            out[k] = z as u8;
            k += 1;
        }
        z += 1;
    }
    out
}

/// Backbone atom: the most-bonded element (lowest Z on a tie). Trivalent under [2, 6]: two
/// bonds for the chain and one for a side group, which is what makes a sequence readable.
pub const BACKBONE: u8 = backbone();

const fn backbone() -> u8 {
    let (mut best, mut v) = (0u8, 0u8);
    let mut k = 0;
    while k < N_BONDABLE {
        let val = valence_in(&SHELLS, BONDABLE_Z[k]);
        if val > v {
            v = val;
            best = BONDABLE_Z[k];
        }
        k += 1;
    }
    best
}

/// Side-group letters: every bondable element but the backbone (as a side group it would branch).
pub const SIDE_GROUPS: [u8; N_BONDABLE - 1] = side_groups();

const fn side_groups() -> [u8; N_BONDABLE - 1] {
    let mut out = [0u8; N_BONDABLE - 1];
    let (mut k, mut i) = (0, 0);
    while i < N_BONDABLE {
        if BONDABLE_Z[i] != BACKBONE {
            out[k] = BONDABLE_Z[i];
            k += 1;
        }
        i += 1;
    }
    out
}

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
    element_in(&SHELLS, z)
}

/// An element under any shell capacities (for previewing bigger tables).
pub fn element_in(shells: &[u8], z: u8) -> Element {
    let (shell, outer, capacity, inner) = shell_of(shells, z);
    let valence = valence_in(shells, z);
    let z_eff = (z - inner) as f64;
    // sublinear in z_eff, n^1.5 radius penalty: avoids the Hy == Ox tie a plain z_eff/n^2 gives
    let chi = z_eff.powf(0.8) / (shell as f64).powf(1.5);
    let radius = 0.22 + 0.13 * shell as f64 - 0.02 * z_eff;
    let noble = valence == 0;
    // polarity against the backbone-like pull: Ka/Hy are oily, Li/Fl/Bo/Ox like water
    let hydropathy = if noble { 0.0 } else { 2.0 * (chi - CHI_NONPOLAR).abs() - 0.3 };
    let absorb = if valence >= 2 { 0.4 + 0.3 * valence as f64 } else { 0.05 };
    let name = NAME_POOL.get(z as usize - 1).copied().unwrap_or("??");
    Element { z, name, shell, outer, capacity, valence, chi, radius, hydropathy, absorb }
}

/// Covalent and weak bond energies between two elements, milli-units (0 if either is noble).
pub fn bond_energies(a: &Element, b: &Element) -> (Energy, Energy) {
    if a.valence == 0 || b.valence == 0 {
        return (0, 0);
    }
    let e = BOND_SHARED * (a.chi * b.chi).sqrt() + BOND_IONIC * (a.chi - b.chi).powi(2);
    let w = WEAK_SCALE * (a.chi - b.chi).powi(2).min(1.0);
    ((e * KT).round() as Energy, (w * KT).round() as Energy)
}

/// Electronegativity of a perfectly nonpolar partner; sits between Ka and Hy.
pub const CHI_NONPOLAR: f64 = 0.9;

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
        let el: Vec<Element> = (1..=N_ELEMENTS as u8).map(element).collect();
        let mut bond = [[0; N_ELEMENTS + 1]; N_ELEMENTS + 1];
        let mut weak = [[0; N_ELEMENTS + 1]; N_ELEMENTS + 1];
        for a in &el {
            for b in &el {
                (bond[a.z as usize][b.z as usize], weak[a.z as usize][b.z as usize]) = bond_energies(a, b);
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
    fn derived_constants_match_the_shell_rule() {
        assert_eq!(N_ELEMENTS, 8);
        assert_eq!(NAMES, ["Hy", "Ne", "Li", "Bo", "Ka", "Ox", "Fl", "Ar"]);
        assert_eq!(BONDABLE_Z, [1, 3, 4, 5, 6, 7]);
        assert_eq!(BACKBONE, 5);
        assert_eq!(SIDE_GROUPS, [1, 3, 4, 6, 7]);
        // a third shell of 6 repeats the pattern one row down
        let v: Vec<u8> = (9..=14).map(|z| element_in(&[2, 6, 6], z).valence).collect();
        assert_eq!(v, vec![1, 2, 3, 2, 1, 0]);
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
