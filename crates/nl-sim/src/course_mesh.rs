//! Closed triangle meshes for curved course pieces (SPEC §7.11): ramps, kickers and
//! halfpipe walls. Each piece is one mesh lofted between two cross-sections, so its
//! driving surface has no seams for the chassis box to catch on, as separate convex
//! segments had (a car stopped dead on a 22° ramp at 8 m/s). `course_build` builds the
//! sections; `shape_collider` turns the mesh into a Rapier trimesh with internal-edge
//! fixing.

/// A closed, outward-facing triangle mesh, in m.
#[derive(Debug, Clone, PartialEq)]
pub struct LoftedMesh {
    /// Corner points.
    pub points_m: Vec<[f32; 3]>,
    /// Triangles as indices into `points_m`, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
}

/// The solid swept from cross-section `start` to cross-section `end`. Each section is a
/// closed polygon of 2·m points: a lower chain of m points, then the matching upper
/// chain in reverse, so point k pairs with point 2m − 1 − k across the section. Both
/// sections must have the same number of points, in the same order. Caps are split
/// into the quads between paired points, so a section need not be convex as long as
/// each of those quads is.
pub fn loft(start: &[[f32; 3]], end: &[[f32; 3]]) -> LoftedMesh {
    let count = start.len();
    let mut points_m = start.to_vec();
    points_m.extend_from_slice(end);
    let mut triangles = Vec::new();
    for k in 0..count {
        let next = (k + 1) % count;
        let [a, b] = [k as u32, next as u32];
        let [a_end, b_end] = [a + count as u32, b + count as u32];
        triangles.push([a, b, b_end]);
        triangles.push([a, b_end, a_end]);
    }
    let half = count / 2;
    for k in 0..half.saturating_sub(1) {
        // The quad between the pairs (k, 2m − 1 − k) and (k + 1, 2m − 2 − k).
        let [low, low_next] = [k as u32, k as u32 + 1];
        let [up, up_next] = [(count - 1 - k) as u32, (count - 2 - k) as u32];
        triangles.push([low, up, low_next]);
        triangles.push([low_next, up, up_next]);
        let shift = count as u32;
        triangles.push([low + shift, low_next + shift, up + shift]);
        triangles.push([low_next + shift, up_next + shift, up + shift]);
    }
    let mut mesh = LoftedMesh {
        points_m,
        triangles,
    };
    face_outward(&mut mesh);
    mesh
}

/// Flips every triangle if the mesh's signed volume is negative, so the faces point
/// out whichever way round the sections were given.
fn face_outward(mesh: &mut LoftedMesh) {
    if signed_volume_m3(mesh) < 0.0 {
        for triangle in &mut mesh.triangles {
            triangle.swap(1, 2);
        }
    }
}

/// The mesh's volume, positive when its triangles face outward, in m³.
pub fn signed_volume_m3(mesh: &LoftedMesh) -> f32 {
    let mut six_volume = 0.0;
    for [i, j, k] in &mesh.triangles {
        let [a, b, c] = [
            mesh.points_m[*i as usize],
            mesh.points_m[*j as usize],
            mesh.points_m[*k as usize],
        ];
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        six_volume += a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
    }
    six_volume / 6.0
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// A unit cube as a loft of two squares (lower chain then upper chain reversed).
    fn cube() -> LoftedMesh {
        let square = |z: f32| [[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z]];
        loft(&square(0.0), &square(1.0))
    }

    #[test]
    fn a_lofted_cube_has_unit_volume_facing_out() {
        let mesh = cube();

        assert_eq!(mesh.triangles.len(), 12);
        assert!((signed_volume_m3(&mesh) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sections_given_the_other_way_round_still_face_out() {
        let square = |z: f32| [[0.0, 0.0, z], [0.0, 1.0, z], [1.0, 1.0, z], [1.0, 0.0, z]];

        let mesh = loft(&square(0.0), &square(1.0));

        assert!((signed_volume_m3(&mesh) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_edge_is_shared_by_exactly_two_triangles_in_opposite_directions() {
        // A closed, consistently wound surface: each directed edge appears once, and its
        // reverse once, so there are no holes for the car to fall through.
        let section = |z: f32| {
            [
                [0.0, -0.2, z],
                [2.0, -0.2, z],
                [4.0, -0.2, z],
                [4.0, 1.0, z],
                [2.0, 0.3, z],
                [0.0, 0.0, z],
            ]
        };
        let mesh = loft(&section(0.0), &section(3.0));

        let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for [a, b, c] in &mesh.triangles {
            for edge in [(*a, *b), (*b, *c), (*c, *a)] {
                *edges.entry(edge).or_default() += 1;
            }
        }
        for (&(from, to), &uses) in &edges {
            assert_eq!(uses, 1, "edge {from}->{to} used {uses} times");
            assert_eq!(
                edges.get(&(to, from)),
                Some(&1),
                "edge {from}->{to} has no twin"
            );
        }
    }
}
