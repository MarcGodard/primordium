//! Periodic-table bridge: any shell capacities read as a `Universe`, for the elements screen.
use godot::prelude::*;
use primordium_core::element::{KT, N_ELEMENTS, Role, SHELLS, Universe, bond_energies};
use primordium_core::recipe::Recipe;
use std::sync::RwLock;

/// The world recipe every sim builds from (element abundance multipliers, index Z - 1). Set by
/// the elements screen and the campaign save; read when a scale starts.
static RECIPE: RwLock<[f64; N_ELEMENTS]> = RwLock::new([1.0; N_ELEMENTS]);

pub fn recipe() -> Recipe {
    Recipe::new(&*RECIPE.read().unwrap())
}

#[derive(GodotClass)]
#[class(base=Node, init)]
pub struct ElementsSim {
    universe: Option<Universe>,
    base: Base<Node>,
}

#[godot_api]
impl ElementsSim {
    /// Read a table; empty `shells` means this world's.
    #[func]
    fn load(&mut self, shells: PackedByteArray) {
        let s: Vec<u8> = if shells.is_empty() { SHELLS.to_vec() } else { shells.to_vec() };
        self.universe = Some(Universe::new(&s));
    }

    #[func]
    fn shells(&self) -> PackedByteArray {
        self.universe.as_ref().map_or(PackedByteArray::new(), |u| PackedByteArray::from(u.shells.as_slice()))
    }

    /// Per element: identity, shell layout, chemistry, role in life, and bond strengths (kT) to
    /// every element (covalent `bonds`, weak pairing `weak`).
    #[func]
    fn elements(&self) -> VarArray {
        let mut arr = VarArray::new();
        let Some(u) = self.universe.as_ref() else { return arr };
        for (e, r) in u.elements.iter().zip(&u.roles) {
            let mut d = VarDictionary::new();
            d.set("z", e.z as i64);
            d.set("name", e.name);
            d.set("shell", e.shell as i64);
            d.set("outer", e.outer as i64);
            d.set("capacity", e.capacity as i64);
            d.set("valence", e.valence as i64);
            d.set("chi", e.chi);
            d.set("radius", e.radius);
            d.set("hydropathy", e.hydropathy);
            d.set("absorb", e.absorb);
            d.set(
                "role",
                match r {
                    Role::Noble => "noble",
                    Role::Backbone => "backbone",
                    Role::Rival => "rival",
                    Role::Letter => "letter",
                },
            );
            let k = u.letters.iter().position(|&l| l == e.z);
            d.set("partner", k.map_or(0, |k| u.partner[k] as i64));
            d.set("heritable", u.heritable.contains(&e.z));
            let mut bonds = PackedFloat32Array::new();
            let mut weak = PackedFloat32Array::new();
            for o in &u.elements {
                let (b, w) = bond_energies(e, o);
                bonds.push((b as f64 / KT) as f32);
                weak.push((w as f64 / KT) as f32);
            }
            // starting share of atoms on the beach (this world's table only)
            if u.shells == SHELLS {
                let mix = primordium_core::chem::default_composition();
                d.set("abundance", mix[e.z as usize - 1] / mix.iter().sum::<f64>());
            }
            d.set("bonds", &bonds);
            d.set("weak", &weak);
            arr.push(&d.to_variant());
        }
        arr
    }

    /// Set the world recipe: abundance multiplier per element (index Z - 1, clamped x1/10..x10;
    /// missing entries x1). Scales started from now on use it.
    #[func]
    fn set_recipe(mult: PackedFloat64Array) {
        let r = Recipe::new(mult.as_slice());
        *RECIPE.write().unwrap() = r.mult;
    }

    #[func]
    fn recipe() -> PackedFloat64Array {
        PackedFloat64Array::from(recipe().mult.as_slice())
    }

    /// Each side-group letter's sea strength under the recipe (1 = default), in letter order,
    /// then the backbone's as the last entry.
    #[func]
    fn recipe_sea() -> PackedFloat64Array {
        let r = recipe();
        let mut v: Vec<f64> = r.sea().to_vec();
        v.push(r.backbone());
        PackedFloat64Array::from(v.as_slice())
    }

    /// Each element's share of all atoms under the recipe (index Z - 1).
    #[func]
    fn recipe_composition() -> PackedFloat64Array {
        PackedFloat64Array::from(recipe().composition().as_slice())
    }

    /// This world's side-group letters as Z, in letter order (residue k is element letters_z()[k]).
    #[func]
    fn letters_z() -> PackedByteArray {
        PackedByteArray::from(primordium_core::polymer::LETTERS.as_slice())
    }

    /// Element names by Z ("" at 0), and the side-group letters' names in letter order.
    #[func]
    fn element_names() -> PackedStringArray {
        std::iter::once("").chain(primordium_core::element::NAMES.iter().copied()).map(GString::from).collect()
    }

    #[func]
    fn letter_names() -> PackedStringArray {
        primordium_core::polymer::LETTER_NAMES.iter().map(|n| GString::from(*n)).collect()
    }

    /// Sequence-text character per letter (`polymer::letter_char`).
    #[func]
    fn letter_chars() -> GString {
        GString::from(&(0..primordium_core::polymer::N_LETTERS as u8).map(primordium_core::polymer::letter_char).collect::<String>())
    }

    /// This table's data folder relative to the repo root ("data/" for [2, 6]).
    #[func]
    fn data_dir() -> GString {
        GString::from(&primordium_core::element::data_dir())
    }

    /// Elements in this world's table.
    #[func]
    fn n_elements() -> i64 {
        primordium_core::element::N_ELEMENTS as i64
    }

    /// The table's summary: backbone, letters, heritable alphabet (as Z).
    #[func]
    fn summary(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        let Some(u) = self.universe.as_ref() else { return d };
        d.set("backbone", u.backbone as i64);
        d.set("letters", &PackedByteArray::from(u.letters.as_slice()));
        d.set("heritable", &PackedByteArray::from(u.heritable.as_slice()));
        d.set("this_world", u.shells == SHELLS);
        d
    }
}
