//! Scale 1: sequence polymers. Detailed model = lattice folding scored with
//! scale-0 energies; function falls out of the fold, never from a lookup.
//!
//! Backbone atom Ka is trivalent: two backbone bonds plus one side group, so a
//! chain is a sequence of side-group letters. Letters are scale-0 elements
//! bonded to Ka (valence-2 letters carry an Hy cap). Everything a letter does
//! here (stickiness, pairing, water preference, colour) is read from the
//! scale-0 table.
//!
//! Function vector of a folded chain:
//!   - catalysis[p]: barrier cut (kT) for forming scale-0 bond pair p, from
//!     surface "sites" (two exposed residues in contact) that bind the two
//!     reactants with weak bonds and hold them adjacent;
//!   - stability: fraction of time folded at this kT (hot unfolds, loses function);
//!   - amphiphile: hydrophobic-tail / hydrophilic-head score (membrane builder);
//!   - pigment: light capture.

use crate::element::{KT, Table};
use crate::kinetics::{BONDABLE, N_PAIRS, NB, bidx, pair};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Letter alphabet as side-group Z. Nobles cannot bond; Ka as side group is a branch (excluded).
pub const LETTERS: [u8; crate::element::N_SIDE] = crate::element::SIDE_GROUPS;
pub const N_LETTERS: usize = LETTERS.len();
pub const LETTER_NAMES: [&str; N_LETTERS] = letter_names();

const fn letter_names() -> [&'static str; N_LETTERS] {
    let mut out = [""; N_LETTERS];
    let mut i = 0;
    while i < N_LETTERS {
        out[i] = crate::element::NAMES[LETTERS[i] as usize - 1];
        i += 1;
    }
    out
}

// Scales 1-5 are written for this table's 5 letters: the genetic code (`cell::CODE`, 3-bit
// codons onto 5 letters), the Li/Fl heritable pair (`cell.rs`, `lineage.rs`) and per-letter
// tables. A bigger table compiles up to here; generalising those is the next step.
const _: () = assert!(N_LETTERS == 5, "scales 1+ assume 5 side-group letters: generalise cell::CODE and the Li/Fl roles first");

pub type Seq = Vec<u8>;

/// A letter in sequence text: its element name's lowercase initial ("hlbof" in [2, 6]).
pub fn letter_char(l: u8) -> char {
    LETTER_NAMES[l as usize].chars().next().unwrap().to_ascii_lowercase()
}

pub fn seq_string(s: &[u8]) -> String {
    s.iter().map(|&l| letter_char(l)).collect()
}

pub fn parse_seq(s: &str) -> Seq {
    s.chars().filter_map(|c| (0..N_LETTERS as u8).find(|&l| letter_char(l) == c)).collect()
}

/// Per-letter physics, derived once from the scale-0 table.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LetterPhysics {
    /// Weak-bond energy between letters, kT.
    pub pair: [[f64; N_LETTERS]; N_LETTERS],
    /// Weak-bond energy letter -> scale-0 bondable element, kT. Reactant binding.
    pub bind: [[f64; NB]; N_LETTERS],
    pub hydropathy: [f64; N_LETTERS],
    pub absorb: [f64; N_LETTERS],
}

impl LetterPhysics {
    pub fn new(t: &Table) -> Self {
        let mut pair = [[0.0; N_LETTERS]; N_LETTERS];
        let mut bind = [[0.0; NB]; N_LETTERS];
        let mut hydropathy = [0.0; N_LETTERS];
        let mut absorb = [0.0; N_LETTERS];
        for (i, &zi) in LETTERS.iter().enumerate() {
            for (j, &zj) in LETTERS.iter().enumerate() {
                pair[i][j] = t.weak[zi as usize][zj as usize] as f64 / KT;
            }
            for (k, &zk) in BONDABLE.iter().enumerate() {
                // polar grip (weak bond) plus hydrophobic burial: oily pockets hold oily reactants
                let (hl, hr) = (t.get(zi).hydropathy, t.get(zk).hydropathy);
                let burial = if hl < 0.0 && hr < 0.0 { HYDROPHOBIC_CONTACT * (hl * hr).sqrt() } else { 0.0 };
                bind[i][k] = t.weak[zi as usize][zk as usize] as f64 / KT + burial;
            }
            let e = t.get(zi);
            // valence-2 letters carry an Hy cap: average the two atoms' water preference
            hydropathy[i] = if e.valence == 2 { 0.5 * (e.hydropathy + t.get(1).hydropathy) } else { e.hydropathy };
            absorb[i] = e.absorb;
        }
        Self { pair, bind, hydropathy, absorb }
    }
}

/// kT per unit sqrt(hydropathy product) when two oily groups exclude water together.
pub const HYDROPHOBIC_CONTACT: f64 = 1.4;

const SQ: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// A self-avoiding 2D walk: one lattice point per residue.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Fold {
    pub pos: Vec<(i32, i32)>,
    pub energy: f64,
}

/// Contact energy of residues i, j (non-consecutive, lattice-adjacent), kT.
/// Hydrophobic burial plus weak pairing between side groups.
fn contact(ph: &LetterPhysics, a: u8, b: u8) -> f64 {
    let (ha, hb) = (ph.hydropathy[a as usize], ph.hydropathy[b as usize]);
    let hydrophobic = if ha < 0.0 && hb < 0.0 { HYDROPHOBIC_CONTACT * (ha * hb).sqrt() } else { 0.0 };
    -(hydrophobic + ph.pair[a as usize][b as usize])
}

fn energy(seq: &[u8], pos: &[(i32, i32)], ph: &LetterPhysics) -> f64 {
    let mut e = 0.0;
    for i in 0..pos.len() {
        for j in (i + 2)..pos.len() {
            let d = (pos[i].0 - pos[j].0).abs() + (pos[i].1 - pos[j].1).abs();
            if d == 1 {
                e += contact(ph, seq[i], seq[j]);
            }
        }
    }
    e
}

/// Occupancy grid centred on the chain start; any walk of n steps fits in (2n+1)^2.
struct Occ {
    n: i32,
    cells: Vec<u16>,
}

impl Occ {
    fn new(n: usize) -> Self {
        let n = n as i32 + 1;
        Self { n, cells: vec![0; ((2 * n + 1) * (2 * n + 1)) as usize] }
    }

    #[inline]
    fn idx(&self, p: (i32, i32), origin: (i32, i32)) -> usize {
        ((p.1 - origin.1 + self.n) * (2 * self.n + 1) + (p.0 - origin.0 + self.n)) as usize
    }

    /// Fills residue index + 1 per cell; false on a collision (leaves grid dirty: call clear).
    fn fill(&mut self, pos: &[(i32, i32)]) -> bool {
        let o = pos[0];
        for (k, &p) in pos.iter().enumerate() {
            let i = self.idx(p, o);
            if self.cells[i] != 0 {
                return false;
            }
            self.cells[i] = k as u16 + 1;
        }
        true
    }

    fn clear(&mut self, pos: &[(i32, i32)]) {
        let o = pos[0];
        for &p in pos {
            let i = self.idx(p, o);
            self.cells[i] = 0;
        }
    }

    /// Energy with the grid filled: each residue looks at 4 neighbours, O(n).
    fn energy(&self, seq: &[u8], pos: &[(i32, i32)], ph: &LetterPhysics) -> f64 {
        let o = pos[0];
        let mut e = 0.0;
        for (k, &(x, y)) in pos.iter().enumerate() {
            for (dx, dy) in SQ {
                let j = self.cells[self.idx((x + dx, y + dy), o)] as usize;
                if j > k + 2 {
                    e += contact(ph, seq[k], seq[j - 1]);
                }
            }
        }
        e
    }
}

#[cfg(test)]
fn self_avoiding(pos: &[(i32, i32)]) -> bool {
    let mut v: Vec<_> = pos.to_vec();
    v.sort();
    v.windows(2).all(|w| w[0] != w[1])
}

/// Anneal a fold from a straight chain: pivot moves, Metropolis, cooling to `kt`.
/// Deterministic per (seq, seed). Cost ~ steps * len^2: fine for len <= 32.
pub fn fold(seq: &[u8], ph: &LetterPhysics, kt: f64, steps: usize, seed: u64) -> Fold {
    let n = seq.len();
    let mut pos: Vec<(i32, i32)> = (0..n as i32).map(|i| (i, 0)).collect();
    if n < 4 {
        let e = energy(seq, &pos, ph);
        return Fold { pos, energy: e };
    }
    let mut rng = Rng::new(seed ^ seq.iter().fold(0xcbf29ce484222325u64, |h, &l| (h ^ l as u64).wrapping_mul(0x100000001b3)));
    let mut e = energy(seq, &pos, ph);
    let mut best = (pos.clone(), e);
    let mut occ = Occ::new(n);
    let mut cand = pos.clone();
    for s in 0..steps {
        let temp = kt * (1.0 + 4.0 * (1.0 - s as f64 / steps as f64));
        let pivot = 1 + rng.below(n - 2);
        let rot = 1 + rng.below(3); // 90, 180, 270 degrees
        let (px, py) = pos[pivot];
        cand.copy_from_slice(&pos);
        for p in cand.iter_mut().skip(pivot + 1) {
            let (mut dx, mut dy) = (p.0 - px, p.1 - py);
            for _ in 0..rot {
                (dx, dy) = (-dy, dx);
            }
            *p = (px + dx, py + dy);
        }
        let ok = occ.fill(&cand);
        let ce = if ok { occ.energy(seq, &cand, ph) } else { 0.0 };
        occ.clear(&cand);
        if !ok {
            continue;
        }
        if ce <= e || rng.chance(((e - ce) / temp).exp()) {
            std::mem::swap(&mut pos, &mut cand);
            e = ce;
            if e < best.1 {
                best = (pos.clone(), e);
            }
        }
    }
    let (mut pos, energy) = best;
    // canonical frame: start at origin, first step +x
    let (ox, oy) = pos[0];
    let (fx, fy) = (pos[1].0 - ox, pos[1].1 - oy);
    for p in pos.iter_mut() {
        let (mut dx, mut dy) = (p.0 - ox, p.1 - oy);
        let mut k = 0;
        let (mut ax, mut ay) = (fx, fy);
        while (ax, ay) != (1, 0) && k < 4 {
            (ax, ay) = (ay, -ax);
            (dx, dy) = (dy, -dx);
            k += 1;
        }
        *p = (dx, dy);
    }
    Fold { pos, energy }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Function {
    /// Barrier cut per scale-0 bond pair, kT. Already scaled by stability.
    pub catalysis: Vec<f64>,
    pub stability: f64,
    pub amphiphile: f64,
    pub pigment: f64,
    /// Fold energy, kT (negative = compact).
    pub fold_energy: f64,
    /// Stickiness, 0..1: oily residues left exposed on a stable fold grab the membrane of a
    /// neighbouring cell. Exposing them costs fold stability, so adhesion is not free.
    #[serde(default)]
    pub adhesion: f64,
}

impl Function {
    pub fn best_catalysis(&self) -> (usize, f64) {
        self.catalysis.iter().cloned().enumerate().fold((0, 0.0), |a, (i, v)| if v > a.1 { (i, v) } else { a })
    }

    pub fn scaled(&self, w: f64) -> Function {
        let mut f = self.clone();
        f.catalysis.iter_mut().for_each(|x| *x *= w);
        f.stability *= w;
        f.amphiphile *= w;
        f.pigment *= w;
        f.fold_energy *= w;
        f.adhesion *= w;
        f
    }

    pub fn add_scaled(&mut self, o: &Function, w: f64) {
        for (x, y) in self.catalysis.iter_mut().zip(&o.catalysis) {
            *x += w * y;
        }
        self.stability += w * o.stability;
        self.amphiphile += w * o.amphiphile;
        self.pigment += w * o.pigment;
        self.fold_energy += w * o.fold_energy;
        self.adhesion += w * o.adhesion;
    }

    /// Flat vector for distance/accuracy comparisons.
    pub fn as_vec(&self) -> Vec<f64> {
        let mut v = self.catalysis.clone();
        v.extend_from_slice(&[self.stability, self.amphiphile, self.pigment, self.adhesion]);
        v
    }
}

/// Folding entropy per residue, kT: what a fold must beat to stay folded.
pub const UNFOLD_ENTROPY: f64 = 0.35;
/// Barrier cut per kT of reactant grip at a perfect pocket.
pub const CUT_PER_GRIP: f64 = 1.6;

/// Catalytic sites: exposed, non-consecutive residues in contact, with pocket depth 0..1.
/// A site flanked by chain holds reactants; a bare edge (pocket 0) does not and is omitted.
pub fn sites(f: &Fold) -> Vec<(usize, usize, f64)> {
    let n = f.pos.len();
    let occupied: std::collections::HashSet<(i32, i32)> = f.pos.iter().cloned().collect();
    let walls = |k: usize| SQ.iter().filter(|(dx, dy)| occupied.contains(&(f.pos[k].0 + dx, f.pos[k].1 + dy))).count();
    let exposed = |k: usize| walls(k) < 4;
    let mut out = Vec::new();
    for i in 0..n {
        for j in (i + 2)..n {
            let d = (f.pos[i].0 - f.pos[j].0).abs() + (f.pos[i].1 - f.pos[j].1).abs();
            if d != 1 || !exposed(i) || !exposed(j) {
                continue;
            }
            let pocket = ((walls(i) + walls(j)) as f64 - 4.0).clamp(0.0, 3.0) / 3.0;
            if pocket > 0.0 {
                out.push((i, j, pocket));
            }
        }
    }
    out
}

/// Read function off a fold at temperature kt (units of kT at 300 K).
pub fn function_of(seq: &[u8], f: &Fold, ph: &LetterPhysics, kt: f64) -> Function {
    let n = seq.len();
    // two-state folding: dG = E_fold + T*S_unfold*n ; stability = 1/(1+exp(dG/kT))
    let dg = f.energy + kt * UNFOLD_ENTROPY * n as f64;
    let stability = 1.0 / (1.0 + (dg / kt).exp());
    let mut catalysis = vec![0.0; N_PAIRS];
    for (i, j, pocket) in sites(f) {
        let (li, lj) = (seq[i] as usize, seq[j] as usize);
        // reactants must bind both sites; weaker grip limits the cut
        let grip = |a: usize, b: usize| ph.bind[li][a].min(ph.bind[lj][b]);
        for a in 0..NB {
            for b in 0..NB {
                let cut = CUT_PER_GRIP * grip(a, b) * pocket;
                let p = pair(a, b);
                if cut > catalysis[p] {
                    catalysis[p] = cut;
                }
            }
        }
    }
    for c in catalysis.iter_mut() {
        *c *= stability;
    }
    // amphiphile: one end water-hating, the other water-loving, chain long enough to matter
    let half = n / 2;
    let h = |r: &[u8]| r.iter().map(|&l| ph.hydropathy[l as usize]).sum::<f64>() / r.len().max(1) as f64;
    let amphiphile = if n >= 4 { ((h(&seq[half..]) - h(&seq[..half])).abs() * (n as f64 / 8.0).min(1.0)).min(1.0) } else { 0.0 };
    let pigment = seq.iter().map(|&l| ph.absorb[l as usize]).sum::<f64>() / n.max(1) as f64;
    // adhesion: oily residues with open lattice sides, on a fold that holds together
    let occupied: std::collections::HashSet<(i32, i32)> = f.pos.iter().cloned().collect();
    let open = |k: usize| SQ.iter().filter(|(dx, dy)| !occupied.contains(&(f.pos[k].0 + dx, f.pos[k].1 + dy))).count() as f64;
    let sticky: f64 = (0..n).map(|k| ph.hydropathy[seq[k] as usize].max(0.0) * open(k)).sum();
    let adhesion = (stability * sticky / (2.0 * n.max(1) as f64)).min(1.0);
    Function { catalysis, stability, amphiphile, pigment, fold_energy: f.energy, adhesion }
}

/// Convenience: fold then read function.
pub fn express(seq: &[u8], ph: &LetterPhysics, kt: f64, seed: u64) -> (Fold, Function) {
    express_ensemble(seq, ph, kt, seed, ENSEMBLE, STEPS_MULT)
}

/// Annealing runs averaged per sequence. One run of a long chain lands in a
/// different local minimum per seed, so a single fold's function is mostly dice.
pub const ENSEMBLE: usize = 8;
pub const STEPS_MULT: f64 = 0.5;

/// Function = mean over `k` independent folds (a conformational ensemble).
/// Returned fold is the lowest-energy member, for display.
pub fn express_ensemble(seq: &[u8], ph: &LetterPhysics, kt: f64, seed: u64, k: usize, steps_mult: f64) -> (Fold, Function) {
    let steps = ((400 + 60 * seq.len()) as f64 * steps_mult) as usize;
    let mut best: Option<Fold> = None;
    let mut acc: Option<Function> = None;
    for r in 0..k {
        let f = fold(seq, ph, kt, steps, seed.wrapping_add(r as u64 * 0x9E37));
        let func = function_of(seq, &f, ph, kt);
        acc = Some(match acc {
            None => func,
            Some(mut a) => {
                for (x, y) in a.catalysis.iter_mut().zip(&func.catalysis) {
                    *x += y;
                }
                a.stability += func.stability;
                a.fold_energy += func.fold_energy;
                a.adhesion += func.adhesion;
                a
            }
        });
        if best.as_ref().is_none_or(|b| f.energy < b.energy) {
            best = Some(f);
        }
    }
    let mut a = acc.unwrap();
    let kf = k as f64;
    a.catalysis.iter_mut().for_each(|x| *x /= kf);
    a.stability /= kf;
    a.fold_energy /= kf;
    a.adhesion /= kf;
    (best.unwrap(), a)
}

/// Z of a letter, for scale-0 lookups.
pub fn letter_z(l: u8) -> u8 {
    LETTERS[l as usize]
}

pub fn letter_bidx(l: u8) -> usize {
    bidx(letter_z(l)).expect("letters are bondable")
}

#[cfg(test)]
mod tests {
    /// Sequence text needs one character per letter: the initials must not collide.
    #[test]
    fn letter_chars_unique_and_round_trip() {
        let chars: std::collections::HashSet<char> = (0..N_LETTERS as u8).map(letter_char).collect();
        assert_eq!(chars.len(), N_LETTERS, "two letters share an initial");
        let all: Vec<u8> = (0..N_LETTERS as u8).collect();
        assert_eq!(parse_seq(&seq_string(&all)), all);
    }

    use super::*;

    #[test]
    fn folds_are_valid_and_deterministic() {
        let ph = LetterPhysics::new(&Table::new());
        let s = parse_seq("lfbhoolfhbfllofh");
        let (a, fa) = express(&s, &ph, 1.0, 1);
        let (b, _) = express(&s, &ph, 1.0, 1);
        assert_eq!(a, b);
        assert!(self_avoiding(&a.pos));
        for w in a.pos.windows(2) {
            assert_eq!((w[0].0 - w[1].0).abs() + (w[0].1 - w[1].1).abs(), 1, "chain stays connected");
        }
        assert!(a.energy < 0.0, "some contacts form");
        assert!((0.0..=1.0).contains(&fa.stability));
    }

    #[test]
    fn grid_energy_matches_pairwise() {
        let ph = LetterPhysics::new(&Table::new());
        let s = parse_seq("lfbhoolfhbfllofhlfbh");
        let (f, _) = express(&s, &ph, 1.0, 2);
        let mut occ = Occ::new(s.len());
        assert!(occ.fill(&f.pos));
        assert!((occ.energy(&s, &f.pos, &ph) - energy(&s, &f.pos, &ph)).abs() < 1e-9);
    }

    #[test]
    fn heat_unfolds() {
        let ph = LetterPhysics::new(&Table::new());
        let s = parse_seq("lflflflflflflflf");
        let (f, _) = express(&s, &ph, 1.0, 1);
        let cold = function_of(&s, &f, &ph, 0.9).stability;
        let hot = function_of(&s, &f, &ph, 1.3).stability;
        assert!(cold > hot);
    }

    #[test]
    fn different_sequences_differ_in_function() {
        let ph = LetterPhysics::new(&Table::new());
        let seqs = ["lflflflfhhhh", "hhhhhhhhhhhh", "lfbhoolfhbfllofh", "oobbooffllhhbbff", "fhfhlolobhbhffll"];
        let funcs: Vec<Function> = seqs.iter().map(|s| express(&parse_seq(s), &ph, 1.0, 1).1).collect();
        for i in 0..funcs.len() {
            for j in (i + 1)..funcs.len() {
                assert_ne!(funcs[i].as_vec(), funcs[j].as_vec(), "{} vs {}", seqs[i], seqs[j]);
            }
        }
        // selection acts on specific channels (backbone Ka-Ka): sequences must differ there
        let kk = pair(bidx(crate::element::BACKBONE).unwrap(), bidx(crate::element::BACKBONE).unwrap());
        let cuts: Vec<f64> = funcs.iter().map(|f| f.catalysis[kk]).collect();
        let (lo, hi) = cuts.iter().fold((f64::MAX, 0.0f64), |(l, h), &c| (l.min(c), h.max(c)));
        assert!(hi > 0.2 && hi > 3.0 * lo, "backbone catalysis varies by sequence: {cuts:?}");
    }
}
