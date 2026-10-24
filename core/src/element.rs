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
/// previews a table before switching; the `shells-266` feature builds the whole sim on that
/// table (data in its own folder, `data_dir`).
#[cfg(not(feature = "shells-266"))]
pub const SHELLS: [u8; 2] = [2, 6];
#[cfg(feature = "shells-266")]
pub const SHELLS: [u8; 3] = [2, 6, 6];

/// Where this table's learned data lives, relative to the repo root: `data/` for [2, 6], a
/// folder per table otherwise (`data/shells-2-6-6/`), so tables never read each other's files.
pub fn data_dir() -> String {
    if SHELLS[..] == [2, 6] { "data/".into() } else { format!("data/shells-{}/", SHELLS.map(|s| s.to_string()).join("-")) }
}

pub fn data_file(name: &str) -> String {
    data_dir() + name
}
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

/// Most bonds any element makes: the backbone's valence.
pub const MAX_VALENCE: u8 = valence_in(&SHELLS, BACKBONE);

/// Side-group letters: bondable elements with fewer bonds than the backbone. One with as many (a
/// rival backbone) would branch the chain as a side group, so it is no letter.
pub const N_SIDE: usize = count_side();
pub const SIDE_GROUPS: [u8; N_SIDE] = side_groups();

const fn count_side() -> usize {
    let (mut n, mut i) = (0, 0);
    while i < N_BONDABLE {
        if valence_in(&SHELLS, BONDABLE_Z[i]) < MAX_VALENCE {
            n += 1;
        }
        i += 1;
    }
    n
}

const fn side_groups() -> [u8; N_SIDE] {
    let mut out = [0u8; N_SIDE];
    let (mut k, mut i) = (0, 0);
    while i < N_BONDABLE {
        if valence_in(&SHELLS, BONDABLE_Z[i]) < MAX_VALENCE {
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

    // pins below are values of the [2, 6] world
    #[cfg(not(feature = "shells-266"))]
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

    // pins below are values of the [2, 6] world
    #[cfg(not(feature = "shells-266"))]
    #[test]
    fn universe_reads_our_table_as_the_sim_does() {
        let u = Universe::new(&SHELLS);
        assert_eq!(u.backbone, BACKBONE);
        assert_eq!(u.letters, SIDE_GROUPS.to_vec());
        assert_eq!(u.heritable, vec![3, 7]);
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

/// What an element does in a universe's chemistry of life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Role {
    /// Closed shell: bonds to nothing.
    Noble,
    /// Most-bonded atom: chains are built of it.
    Backbone,
    /// Another atom as bonded as the backbone; it would branch a chain as a side group.
    Rival,
    /// A side group on the backbone: a letter of the polymer alphabet.
    Letter,
}

/// A whole periodic table under some shell capacities, read the way the sim reads ours:
/// elements, the backbone the shell rule picks, side-group letters, each letter's pairing
/// partner (copying puts a template letter's strongest weak-bond partner opposite it) and the
/// heritable alphabet: letters that come back after any number of double copies.
#[derive(Clone, Debug, Serialize)]
pub struct Universe {
    pub shells: Vec<u8>,
    pub elements: Vec<Element>,
    pub roles: Vec<Role>,
    /// Z of the backbone atom; 0 when nothing bonds.
    pub backbone: u8,
    /// Letters as Z, then each letter's partner as Z.
    pub letters: Vec<u8>,
    pub partner: Vec<u8>,
    pub heritable: Vec<u8>,
}

impl Universe {
    pub fn new(shells: &[u8]) -> Self {
        let n = shell_total(shells).min(NAME_POOL.len());
        let elements: Vec<Element> = (1..=n as u8).map(|z| element_in(shells, z)).collect();
        let top = elements.iter().map(|e| e.valence).max().unwrap_or(0);
        let backbone = elements.iter().find(|e| e.valence == top && top > 0).map_or(0, |e| e.z);
        let roles: Vec<Role> = elements
            .iter()
            .map(|e| match e.valence {
                0 => Role::Noble,
                _ if e.z == backbone => Role::Backbone,
                v if v == top => Role::Rival,
                _ => Role::Letter,
            })
            .collect();
        // chains need two bonds plus a side group: below valence 3 there is no sequence to read
        let letters: Vec<&Element> = if top >= 3 { elements.iter().filter(|e| e.valence > 0 && e.valence < top).collect() } else { Vec::new() };
        let comp: Vec<usize> = (0..letters.len()).map(|i| (0..letters.len()).max_by_key(|&j| bond_energies(letters[i], letters[j]).1).unwrap()).collect();
        let mut alive: Vec<usize> = (0..letters.len()).collect();
        loop {
            let mut next: Vec<usize> = alive.iter().map(|&i| comp[comp[i]]).collect();
            next.sort_unstable();
            next.dedup();
            if next == alive {
                break;
            }
            alive = next;
        }
        Self {
            shells: shells.to_vec(),
            roles,
            backbone,
            partner: comp.iter().map(|&j| letters[j].z).collect(),
            heritable: alive.iter().map(|&i| letters[i].z).collect(),
            letters: letters.iter().map(|e| e.z).collect(),
            elements,
        }
    }

    pub fn get(&self, z: u8) -> &Element {
        &self.elements[z as usize - 1]
    }
}
