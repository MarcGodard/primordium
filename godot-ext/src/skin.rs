//! A smooth skin over a body: an isosurface of metaballs at the cell centres (surface nets on a
//! grid), coloured by the cells under it. For the gallery and for bodies picked on the reef.
use godot::prelude::*;

/// Grid steps per cell spacing.
const RES: f32 = 3.0;
/// Metaball width in cell spacings and the iso level: neighbours fuse, a lone cell stays round.
const SIGMA: f32 = 0.62;
const ISO: f32 = 0.5;

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
pub struct BodySkin {
    base: Base<RefCounted>,
}

#[godot_api]
impl BodySkin {
    /// Mesh arrays (Mesh.ARRAY_MAX long: vertex, normal, color, index) for cells at `points` with
    /// `colors`; empty when there are no cells.
    #[func]
    fn build(points: PackedVector3Array, colors: PackedColorArray) -> VarArray {
        let pts: Vec<[f32; 3]> = points.as_slice().iter().map(|p| [p.x, p.y, p.z]).collect();
        let cols: Vec<[f32; 3]> = colors.as_slice().iter().map(|c| [c.r, c.g, c.b]).collect();
        let mut out = VarArray::new();
        let Some(m) = surface(&pts, &cols) else { return out };
        let mut verts = PackedVector3Array::new();
        let mut norms = PackedVector3Array::new();
        let mut vcols = PackedColorArray::new();
        for i in 0..m.verts.len() {
            let v = m.verts[i];
            let n = m.normals[i];
            let c = m.colors[i];
            verts.push(Vector3::new(v[0], v[1], v[2]));
            norms.push(Vector3::new(n[0], n[1], n[2]));
            vcols.push(Color::from_rgb(c[0], c[1], c[2]));
        }
        let idx = PackedInt32Array::from(m.index.as_slice());
        // Mesh.ARRAY_VERTEX 0, NORMAL 1, TANGENT 2, COLOR 3, ... INDEX 12, ARRAY_MAX 13
        for k in 0..13 {
            match k {
                0 => out.push(&verts.to_variant()),
                1 => out.push(&norms.to_variant()),
                3 => out.push(&vcols.to_variant()),
                12 => out.push(&idx.to_variant()),
                _ => out.push(&Variant::nil()),
            }
        }
        out
    }
}

pub struct SkinMesh {
    pub verts: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub index: Vec<i32>,
}

/// Surface nets: one vertex per grid cube the surface crosses (at the mean of its edge crossings),
/// one quad per grid edge the surface crosses, joining the four cubes around that edge.
pub fn surface(pts: &[[f32; 3]], cols: &[[f32; 3]]) -> Option<SkinMesh> {
    if pts.is_empty() || pts.len() != cols.len() {
        return None;
    }
    let pad = 2.0;
    let lo: [f32; 3] = std::array::from_fn(|k| pts.iter().map(|p| p[k]).fold(f32::MAX, f32::min) - pad);
    let hi: [f32; 3] = std::array::from_fn(|k| pts.iter().map(|p| p[k]).fold(f32::MIN, f32::max) + pad);
    let h = 1.0 / RES;
    let n: [usize; 3] = std::array::from_fn(|k| ((hi[k] - lo[k]) / h).ceil() as usize + 1);
    let at = |i: usize, j: usize, k: usize| (k * n[1] + j) * n[0] + i;
    let pos = |i: usize, j: usize, k: usize| [lo[0] + i as f32 * h, lo[1] + j as f32 * h, lo[2] + k as f32 * h];
    // splat: each cell adds its gaussian (and colour) to the grid points within 3 sigma
    let len = n[0] * n[1] * n[2];
    let mut val = vec![0.0f32; len];
    let mut col = vec![[0.0f32; 3]; len];
    let reach = (3.0 * SIGMA / h).ceil() as i64;
    for (q, c) in pts.iter().zip(cols) {
        let g: [i64; 3] = std::array::from_fn(|d| ((q[d] - lo[d]) / h).round() as i64);
        for k in (g[2] - reach).max(0)..=(g[2] + reach).min(n[2] as i64 - 1) {
            for j in (g[1] - reach).max(0)..=(g[1] + reach).min(n[1] as i64 - 1) {
                for i in (g[0] - reach).max(0)..=(g[0] + reach).min(n[0] as i64 - 1) {
                    let p = pos(i as usize, j as usize, k as usize);
                    let d2 = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
                    let w = (-d2 / (SIGMA * SIGMA)).exp();
                    let x = at(i as usize, j as usize, k as usize);
                    val[x] += w;
                    for e in 0..3 {
                        col[x][e] += w * c[e];
                    }
                }
            }
        }
    }
    for x in 0..len {
        if val[x] > 0.0 {
            for e in 0..3 {
                col[x][e] /= val[x];
            }
        }
        val[x] -= ISO;
    }
    // a vertex per crossed cube
    let mut vid = vec![-1i32; len];
    let mut m = SkinMesh { verts: Vec::new(), normals: Vec::new(), colors: Vec::new(), index: Vec::new() };
    const CORNERS: [[usize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
    const EDGES: [[usize; 2]; 12] = [[0, 1], [2, 3], [4, 5], [6, 7], [0, 2], [1, 3], [4, 6], [5, 7], [0, 4], [1, 5], [2, 6], [3, 7]];
    for k in 0..n[2] - 1 {
        for j in 0..n[1] - 1 {
            for i in 0..n[0] - 1 {
                let ix: [usize; 8] = std::array::from_fn(|c| at(i + CORNERS[c][0], j + CORNERS[c][1], k + CORNERS[c][2]));
                let v: [f32; 8] = std::array::from_fn(|c| val[ix[c]]);
                let inside = v.iter().filter(|&&x| x > 0.0).count();
                if inside == 0 || inside == 8 {
                    continue;
                }
                let mut sum = [0.0f32; 3];
                let mut cnt = 0.0;
                for e in EDGES {
                    let (a, b) = (v[e[0]], v[e[1]]);
                    if (a > 0.0) != (b > 0.0) {
                        let t = a / (a - b);
                        for d in 0..3 {
                            let ca = CORNERS[e[0]][d] as f32;
                            let cb = CORNERS[e[1]][d] as f32;
                            sum[d] += ca + (cb - ca) * t;
                        }
                        cnt += 1.0;
                    }
                }
                let f: [f32; 3] = std::array::from_fn(|d| sum[d] / cnt);
                let base = pos(i, j, k);
                // trilinear weights of the vertex inside its cube, for normal and colour
                let wt: [f32; 8] = std::array::from_fn(|c| (0..3).map(|d| if CORNERS[c][d] == 1 { f[d] } else { 1.0 - f[d] }).product());
                // normal: minus the field's gradient across the cube (the field falls outward)
                let g: [f32; 3] = std::array::from_fn(|d| {
                    let mut gsum = 0.0;
                    for c in 0..8 {
                        let s = if CORNERS[c][d] == 1 { 1.0 } else { -1.0 };
                        // weight by the other two axes' position
                        let w: f32 = (0..3).filter(|&e| e != d).map(|e| if CORNERS[c][e] == 1 { f[e] } else { 1.0 - f[e] }).product();
                        gsum += s * w * v[c];
                    }
                    -gsum
                });
                let gl = (g[0] * g[0] + g[1] * g[1] + g[2] * g[2]).sqrt().max(1e-6);
                let mut c3 = [0.0f32; 3];
                for c in 0..8 {
                    for e in 0..3 {
                        c3[e] += wt[c] * col[ix[c]][e];
                    }
                }
                vid[at(i, j, k)] = m.verts.len() as i32;
                m.verts.push([base[0] + f[0] * h, base[1] + f[1] * h, base[2] + f[2] * h]);
                m.normals.push([g[0] / gl, g[1] / gl, g[2] / gl]);
                m.colors.push(c3);
            }
        }
    }
    // a quad per crossed grid edge, between the four cubes sharing it
    for k in 1..n[2] - 1 {
        for j in 1..n[1] - 1 {
            for i in 1..n[0] - 1 {
                let here = val[at(i, j, k)] > 0.0;
                // edges along +x, +y, +z from this grid point
                let steps = [([1, 0, 0], [[0, 0, 0], [0, -1, 0], [0, -1, -1], [0, 0, -1]]), ([0, 1, 0], [[0, 0, 0], [0, 0, -1], [-1, 0, -1], [-1, 0, 0]]), ([0, 0, 1], [[0, 0, 0], [-1, 0, 0], [-1, -1, 0], [0, -1, 0]])];
                for (d, cubes) in steps {
                    let (i2, j2, k2) = (i + d[0], j + d[1], k + d[2]);
                    if i2 >= n[0] || j2 >= n[1] || k2 >= n[2] {
                        continue;
                    }
                    let there = val[at(i2, j2, k2)] > 0.0;
                    if here == there {
                        continue;
                    }
                    let q: Vec<i32> = cubes.iter().map(|c| vid[at((i as i32 + c[0]) as usize, (j as i32 + c[1]) as usize, (k as i32 + c[2]) as usize)]).collect();
                    if q.iter().any(|&x| x < 0) {
                        continue;
                    }
                    // Godot's front faces wind clockwise seen from outside
                    if here {
                        m.index.extend_from_slice(&[q[0], q[2], q[1], q[0], q[3], q[2]]);
                    } else {
                        m.index.extend_from_slice(&[q[0], q[1], q[2], q[0], q[2], q[3]]);
                    }
                }
            }
        }
    }
    Some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One ball: a closed surface at about the iso radius, every vertex on it.
    #[test]
    fn single_ball_is_closed_and_round() {
        let m = surface(&[[0.0, 0.0, 0.0]], &[[1.0, 0.0, 0.0]]).unwrap();
        assert!(m.verts.len() > 50 && !m.index.is_empty());
        let r_iso = SIGMA * (1.0 / ISO).ln().sqrt();
        for v in &m.verts {
            let r = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((r - r_iso).abs() < 0.12, "vertex at r {r}, iso radius {r_iso}");
        }
        // closed: every edge shared by exactly two triangles
        let mut edges = std::collections::HashMap::new();
        for t in m.index.chunks(3) {
            for e in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((e.0.min(e.1), e.0.max(e.1))).or_insert(0) += 1;
            }
        }
        assert!(edges.values().all(|&c| c == 2), "surface has holes or folds");
        // outward: normals point away from the centre
        assert!(m.verts.iter().zip(&m.normals).all(|(v, n)| v[0] * n[0] + v[1] * n[1] + v[2] * n[2] > 0.0));
    }

    /// Two touching cells fuse into one surface coloured by both.
    #[test]
    fn neighbours_fuse_and_blend() {
        let m = surface(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]], &[[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]).unwrap();
        let mid = m.verts.iter().zip(&m.colors).filter(|(v, _)| (v[0] - 0.5).abs() < 0.2).map(|(_, c)| *c).next().expect("no skin over the neck: cells did not fuse");
        assert!(mid[0] > 0.2 && mid[2] > 0.2, "neck colour {mid:?} should mix both cells");
    }
}
