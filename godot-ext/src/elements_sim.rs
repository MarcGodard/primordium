//! Periodic-table bridge: any shell capacities read as a `Universe`, for the elements screen.
use godot::prelude::*;
use primordium_core::element::{KT, Role, SHELLS, Universe, bond_energies};

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
