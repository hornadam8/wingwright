//! Procedural meshes: smooth lofted parts and low-poly scenery.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Builds a flat-shaded mesh from a raw triangle list (3 positions per triangle).
pub fn flat_mesh(tris: Vec<[f32; 3]>, colors: Option<Vec<[f32; 4]>>) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, tris);
    if let Some(c) = colors {
        mesh = mesh.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, c);
    }
    mesh.with_computed_flat_normals()
}

/// Indexed mesh with smooth normals.
pub fn indexed_mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices))
        .with_computed_smooth_normals()
}

fn push_quad(tris: &mut Vec<[f32; 3]>, a: Vec3, b: Vec3, c: Vec3, d: Vec3) {
    tris.push(a.to_array());
    tris.push(b.to_array());
    tris.push(c.to_array());
    tris.push(a.to_array());
    tris.push(c.to_array());
    tris.push(d.to_array());
}

/// Mirrors a mesh across the YZ plane (x -> -x), keeping faces outward.
pub fn mirror_x(mut mesh: Mesh) -> Mesh {
    for id in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL] {
        if let Some(VertexAttributeValues::Float32x3(v)) = mesh.attribute_mut(id) {
            for p in v.iter_mut() {
                p[0] = -p[0];
            }
        }
    }
    let has_indices = mesh.indices().is_some();
    match mesh.indices_mut() {
        Some(Indices::U32(idx)) => {
            for tri in idx.chunks_mut(3) {
                if tri.len() == 3 {
                    tri.swap(1, 2);
                }
            }
        }
        Some(Indices::U16(idx)) => {
            for tri in idx.chunks_mut(3) {
                if tri.len() == 3 {
                    tri.swap(1, 2);
                }
            }
        }
        None => {}
    }
    if !has_indices {
        // Non-indexed: reorder the vertices of every triangle in each attribute.
        for id in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL, Mesh::ATTRIBUTE_COLOR, Mesh::ATTRIBUTE_UV_0] {
            match mesh.attribute_mut(id) {
                Some(VertexAttributeValues::Float32x3(v)) => {
                    for tri in v.chunks_mut(3) {
                        if tri.len() == 3 {
                            tri.swap(1, 2);
                        }
                    }
                }
                Some(VertexAttributeValues::Float32x4(v)) => {
                    for tri in v.chunks_mut(3) {
                        if tri.len() == 3 {
                            tri.swap(1, 2);
                        }
                    }
                }
                Some(VertexAttributeValues::Float32x2(v)) => {
                    for tri in v.chunks_mut(3) {
                        if tri.len() == 3 {
                            tri.swap(1, 2);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    mesh
}

/// The transform a child needs when its parent part is mirrored across x.
pub fn mirror_transform(t: Transform) -> Transform {
    Transform {
        translation: Vec3::new(-t.translation.x, t.translation.y, t.translation.z),
        rotation: Quat::from_xyzw(t.rotation.x, -t.rotation.y, -t.rotation.z, t.rotation.w),
        scale: t.scale,
    }
}

// ---------------------------------------------------------------------------------------
// profiles

/// Rounded rectangle of size (w, h) with corner radius `r`, counter-clockwise viewed from +Z.
pub fn rounded_rect(w: f32, h: f32, r: f32, seg: usize) -> Vec<Vec2> {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    let (cx, cy) = (w * 0.5 - r, h * 0.5 - r);
    let corners = [(cx, -cy, -FRAC_PI_2), (cx, cy, 0.0), (-cx, cy, FRAC_PI_2), (-cx, -cy, PI)];
    let mut pts = Vec::with_capacity(corners.len() * seg);
    for (x, y, a0) in corners {
        for k in 0..seg {
            let a = a0 + FRAC_PI_2 * k as f32 / (seg - 1).max(1) as f32;
            pts.push(Vec2::new(x + r * a.cos(), y + r * a.sin()));
        }
    }
    pts
}

pub fn circle(r: f32, n: usize) -> Vec<Vec2> {
    (0..n)
        .map(|i| {
            let a = TAU * i as f32 / n as f32;
            Vec2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

/// Cambered NACA-style airfoil as an open loop: lower surface from the trailing edge to the
/// leading edge, then the upper surface back. Leading edge at x = +chord/2 (which becomes
/// -Z after the wing rotation), counter-clockwise viewed from +Z.
pub fn airfoil(chord: f32, thickness: f32, n: usize) -> Vec<Vec2> {
    let t = (thickness / chord).clamp(0.04, 0.35);
    let (m, p) = (0.02f32, 0.4f32);
    let yt = |x: f32| 5.0 * t * (0.2969 * x.sqrt() - 0.1260 * x - 0.3516 * x * x + 0.2843 * x * x * x - 0.1036 * x * x * x * x);
    let yc = |x: f32| if x < p { m / (p * p) * (2.0 * p * x - x * x) } else { m / ((1.0 - p) * (1.0 - p)) * ((1.0 - 2.0 * p) + 2.0 * p * x - x * x) };
    let mut pts = Vec::with_capacity(2 * n + 2);
    // lower: TE -> LE
    for i in 0..=n {
        let th = PI * i as f32 / n as f32;
        let xf = (1.0 + th.cos()) * 0.5; // 1 -> 0
        let x = chord * 0.5 - xf * chord;
        pts.push(Vec2::new(x, (yc(xf) - yt(xf)) * chord));
    }
    // upper: LE -> TE (skip the duplicated LE point)
    for i in 1..=n {
        let th = PI * i as f32 / n as f32;
        let xf = (1.0 - th.cos()) * 0.5; // 0 -> 1
        let x = chord * 0.5 - xf * chord;
        pts.push(Vec2::new(x, (yc(xf) + yt(xf)) * chord));
    }
    pts
}

// ---------------------------------------------------------------------------------------
// lofting

/// Extrudes `profile` along +Z through `sections` of (z, scale_x, scale_y), with smooth
/// side normals and flat end caps. `closed` joins the last profile point to the first.
pub fn loft(profile: &[Vec2], sections: &[(f32, f32, f32)], closed: bool, caps: bool) -> Mesh {
    let np = profile.len();
    let ns = sections.len();
    let mut pos: Vec<[f32; 3]> = Vec::with_capacity(np * ns + 2 * (np + 1));
    let mut idx: Vec<u32> = Vec::new();
    for (z, sx, sy) in sections {
        for p in profile {
            pos.push([p.x * sx, p.y * sy, *z]);
        }
    }
    let edges = if closed { np } else { np - 1 };
    for s in 0..ns - 1 {
        for i in 0..edges {
            let j = (i + 1) % np;
            let a = (s * np + i) as u32;
            let b = (s * np + j) as u32;
            let c = ((s + 1) * np + j) as u32;
            let d = ((s + 1) * np + i) as u32;
            idx.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    if caps {
        for (which, (z, sx, sy)) in [(0usize, sections[0]), (1, sections[ns - 1])] {
            let base = pos.len() as u32;
            let mut cx = 0.0;
            let mut cy = 0.0;
            for p in profile {
                cx += p.x * sx;
                cy += p.y * sy;
            }
            pos.push([cx / np as f32, cy / np as f32, z]);
            for p in profile {
                pos.push([p.x * sx, p.y * sy, z]);
            }
            let n = if closed { np } else { np - 1 };
            for i in 0..n {
                let j = (i + 1) % np;
                let (a, b) = (base + 1 + i as u32, base + 1 + j as u32);
                if which == 0 {
                    idx.extend_from_slice(&[base, b, a]);
                } else {
                    idx.extend_from_slice(&[base, a, b]);
                }
            }
        }
    }
    indexed_mesh(pos, idx)
}

/// Fuselage-style tube with rounded corners.
pub fn rounded_box(w: f32, h: f32, l: f32, radius_frac: f32) -> Mesh {
    let r = w.min(h) * radius_frac;
    let prof = rounded_rect(w, h, r, 5);
    loft(&prof, &[(-l * 0.5, 1.0, 1.0), (l * 0.5, 1.0, 1.0)], true, true)
}

/// Nose or tail cone: round cross-section tapering to `tip_scale` with an ogive curve.
/// `tip_sign` is -1 for a tip at -Z (nose) or +1 for a tip at +Z (tail).
pub fn cone_body(w: f32, h: f32, l: f32, tip_scale: f32, tip_sign: f32) -> Mesh {
    let prof = rounded_rect(w, h, w.min(h) * 0.5, 7);
    let n = 7;
    let mut sections = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32; // 0 at the base, 1 at the tip
        let s = if tip_sign < 0.0 {
            // nose: blunt rounded ogive
            tip_scale + (1.0 - tip_scale) * (1.0 - t * t).sqrt()
        } else {
            // tail: gentle taper
            tip_scale + (1.0 - tip_scale) * (1.0 - t).powf(1.35)
        };
        let z = if tip_sign < 0.0 { l * 0.5 - t * l } else { -l * 0.5 + t * l };
        sections.push((z, s, s));
    }
    if tip_sign < 0.0 {
        sections.reverse();
    }
    loft(&prof, &sections, true, true)
}

/// Wing panel: root at -X, tip at +X, chord along Z (leading edge at -Z). Smooth skin.
pub fn wing_mesh(span: f32, thickness: f32, chord: f32, taper: f32) -> Mesh {
    let prof = airfoil(chord, thickness, 12);
    let tip_round = (chord * 0.12).min(span * 0.15);
    let sections = [
        (-span * 0.5, 1.0, 1.0),
        (span * 0.5 - tip_round, taper, taper),
        (span * 0.5, taper * 0.55, taper * 0.6),
    ];
    loft(&prof, &sections, false, true).rotated_by(Quat::from_rotation_y(FRAC_PI_2))
}

/// Sections along a span from `z0` to `z1` (loft axis) with fabric sagging between ribs
/// every `pitch` meters. `scale(t)` gives the chord/thickness scale at 0..1 along the span.
fn ribbed_sections(z0: f32, z1: f32, pitch: f32, sag: f32, scale: impl Fn(f32) -> f32) -> Vec<(f32, f32, f32)> {
    let len = z1 - z0;
    let ribs = ((len / pitch).round() as usize).max(1);
    let step = len / ribs as f32;
    let mut out = Vec::with_capacity(ribs * 2 + 1);
    for i in 0..=ribs {
        let z = z0 + i as f32 * step;
        let s = scale((z - z0) / len);
        out.push((z, s, s));
        if i < ribs {
            let zm = z + step * 0.5;
            let sm = scale((zm - z0) / len);
            out.push((zm, sm, sm * sag));
        }
    }
    out
}

/// Fabric-covered wing with visible ribs: root at -X, tip at +X, leading edge at -Z.
pub fn ribbed_wing(span: f32, thickness: f32, chord: f32, taper: f32) -> Mesh {
    let prof = airfoil(chord, thickness, 12);
    let tip_round = (chord * 0.12).min(span * 0.15);
    let body_end = span * 0.5 - tip_round;
    let mut sections = ribbed_sections(-span * 0.5, body_end, 0.3, 0.8, |t| 1.0 + (taper - 1.0) * t);
    sections.push((span * 0.5, taper * 0.55, taper * 0.6));
    loft(&prof, &sections, false, true).rotated_by(Quat::from_rotation_y(FRAC_PI_2))
}

/// Full-span symmetric ribbed surface (tail planes): root in the middle, both tips tapered.
pub fn symmetric_wing(span: f32, thickness: f32, chord: f32, taper: f32) -> Mesh {
    let prof = airfoil(chord, thickness, 10);
    let tip_round = (chord * 0.12).min(span * 0.08);
    let half = span * 0.5 - tip_round;
    let mut sections = vec![(-span * 0.5, taper * 0.55, taper * 0.6)];
    sections.extend(ribbed_sections(-half, half, 0.3, 0.8, |t| {
        let d = (t - 0.5).abs() * 2.0;
        1.0 + (taper - 1.0) * d
    }));
    sections.push((span * 0.5, taper * 0.55, taper * 0.6));
    loft(&prof, &sections, false, true).rotated_by(Quat::from_rotation_y(FRAC_PI_2))
}

/// Smooth-skinned symmetric surface for modern tail planes.
pub fn symmetric_wing_smooth(span: f32, thickness: f32, chord: f32, taper: f32) -> Mesh {
    let prof = airfoil(chord, thickness, 10);
    let tip_round = (chord * 0.12).min(span * 0.08);
    let sections = [
        (-span * 0.5, taper * 0.55, taper * 0.6),
        (-span * 0.5 + tip_round, taper, taper),
        (0.0, 1.0, 1.0),
        (span * 0.5 - tip_round, taper, taper),
        (span * 0.5, taper * 0.55, taper * 0.6),
    ];
    loft(&prof, &sections, false, true).rotated_by(Quat::from_rotation_y(FRAC_PI_2))
}

/// Concatenates several meshes with the same attribute layout into one.
pub fn merged(mut parts: Vec<Mesh>) -> Mesh {
    let mut first = parts.remove(0);
    for p in parts {
        first.merge(&p).expect("meshes share a layout");
    }
    first
}

/// Gives every vertex the sRGB colour `rgb`, scaled by a brightness ramp over height
/// (`lo` at `y0`, `hi` at `y1`) so a crown reads lighter at the top than at its skirt.
pub fn tinted(mut mesh: Mesh, rgb: [f32; 3], y0: f32, y1: f32, lo: f32, hi: f32) -> Mesh {
    let l = lin(rgb);
    let colors: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(pos)) => pos
            .iter()
            .map(|p| {
                let t = ((p[1] - y0) / (y1 - y0)).clamp(0.0, 1.0);
                let k = lo + (hi - lo) * t;
                [l[0] * k, l[1] * k, l[2] * k, 1.0]
            })
            .collect(),
        _ => Vec::new(),
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

/// Unit sphere with a baked shade ramp (white top, blue-grey underside) for cumulus puffs
/// drawn unlit: soft and painterly from every direction.
pub fn cloud_puff() -> Mesh {
    let top = lin([1.0, 1.0, 1.0]);
    let bottom = lin([0.74, 0.78, 0.86]);
    let mut mesh = Sphere::new(1.0).mesh().ico(2).unwrap();
    let colors: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(pos)) => pos
            .iter()
            .map(|p| {
                let t = ((p[1] + 0.7) / 1.3).clamp(0.0, 1.0);
                let t = t * t * (3.0 - 2.0 * t);
                [bottom[0] + (top[0] - bottom[0]) * t, bottom[1] + (top[1] - bottom[1]) * t, bottom[2] + (top[2] - bottom[2]) * t, 1.0]
            })
            .collect(),
        _ => Vec::new(),
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

/// Converts an sRGB triple to the linear values vertex colors expect.
pub fn lin(c: [f32; 3]) -> [f32; 3] {
    let l = Color::srgb(c[0], c[1], c[2]).to_linear();
    [l.red, l.green, l.blue]
}

/// Sphere with a vertical color gradient (sRGB inputs), rendered from the inside as a sky dome.
pub fn sky_dome(horizon: [f32; 3], zenith: [f32; 3], below: [f32; 3]) -> Mesh {
    let (horizon, zenith, below) = (lin(horizon), lin(zenith), lin(below));
    let mut mesh = Sphere::new(1.0).mesh().uv(48, 24);
    let colors: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(pos)) => pos
            .iter()
            .map(|p| {
                let y = p[1];
                if y < 0.0 {
                    let t = (-y * 6.0).min(1.0);
                    [horizon[0] + (below[0] - horizon[0]) * t, horizon[1] + (below[1] - horizon[1]) * t, horizon[2] + (below[2] - horizon[2]) * t, 1.0]
                } else {
                    let t = (y / 0.42).min(1.0);
                    let t = t * t * (3.0 - 2.0 * t);
                    [horizon[0] + (zenith[0] - horizon[0]) * t, horizon[1] + (zenith[1] - horizon[1]) * t, horizon[2] + (zenith[2] - horizon[2]) * t, 1.0]
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

/// Vertical fin: root at -Y, tip at +Y, chord along Z.
pub fn fin_mesh(height: f32, thickness: f32, chord: f32, taper: f32) -> Mesh {
    ribbed_wing(height, thickness, chord, taper).rotated_by(Quat::from_rotation_z(FRAC_PI_2))
}

/// Round body of revolution along Z from a list of (z, radius) stations.
pub fn spindle(stations: &[(f32, f32)], segments: usize) -> Mesh {
    let prof = circle(1.0, segments);
    let sections: Vec<(f32, f32, f32)> = stations.iter().map(|(z, r)| (*z, *r, *r)).collect();
    loft(&prof, &sections, true, true)
}

/// Cylinder along the Z axis.
pub fn cylinder_z(radius: f32, length: f32, segments: u32) -> Mesh {
    Cylinder::new(radius, length)
        .mesh()
        .resolution(segments)
        .build()
        .rotated_by(Quat::from_rotation_x(FRAC_PI_2))
}

/// Cylinder along the X axis (wheels).
pub fn cylinder_x(radius: f32, length: f32, segments: u32) -> Mesh {
    Cylinder::new(radius, length)
        .mesh()
        .resolution(segments)
        .build()
        .rotated_by(Quat::from_rotation_z(FRAC_PI_2))
}

/// A cone pointing up (+Y) with its base at y=0.
pub fn cone_up(radius: f32, height: f32, segments: u32) -> Mesh {
    Cone::new(radius, height)
        .mesh()
        .resolution(segments)
        .build()
        .translated_by(Vec3::new(0.0, height * 0.5, 0.0))
}

/// A cone pointing forward (-Z) with its base at z=0 (propeller spinners).
pub fn spinner(radius: f32, length: f32) -> Mesh {
    spindle(&[(-length, 0.02), (-length * 0.7, radius * 0.55), (-length * 0.35, radius * 0.9), (0.0, radius)], 16)
}

/// Simple pyramid roof (4-sided) with base half-size (hx, hz), height h, base at y=0.
pub fn pyramid(hx: f32, hz: f32, h: f32) -> Mesh {
    let apex = Vec3::new(0.0, h, 0.0);
    let c = [
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.0, -hz),
        Vec3::new(hx, 0.0, hz),
        Vec3::new(-hx, 0.0, hz),
    ];
    let mut tris = Vec::new();
    for i in 0..4 {
        let j = (i + 1) % 4;
        tris.push(c[j].to_array());
        tris.push(c[i].to_array());
        tris.push(apex.to_array());
    }
    push_quad(&mut tris, c[0], c[1], c[2], c[3]);
    flat_mesh(tris, None)
}

/// A box whose +Z or -Z end is scaled down, flat shaded (scenery).
pub fn tapered_box(w: f32, h: f32, l: f32, tip_scale: f32, tip_sign: f32) -> Mesh {
    let (hw, hh, hl) = (w * 0.5, h * 0.5, l * 0.5);
    let base_z = -tip_sign * hl;
    let tip_z = tip_sign * hl;
    let (tw, th) = (hw * tip_scale, hh * tip_scale);
    let b = [Vec3::new(-hw, -hh, base_z), Vec3::new(hw, -hh, base_z), Vec3::new(hw, hh, base_z), Vec3::new(-hw, hh, base_z)];
    let t = [Vec3::new(-tw, -th, tip_z), Vec3::new(tw, -th, tip_z), Vec3::new(tw, th, tip_z), Vec3::new(-tw, th, tip_z)];
    let mut tris = Vec::new();
    for i in 0..4 {
        let j = (i + 1) % 4;
        if tip_sign > 0.0 {
            push_quad(&mut tris, b[i], b[j], t[j], t[i]);
        } else {
            push_quad(&mut tris, b[j], b[i], t[i], t[j]);
        }
    }
    if tip_sign > 0.0 {
        push_quad(&mut tris, b[3], b[2], b[1], b[0]);
        push_quad(&mut tris, t[0], t[1], t[2], t[3]);
    } else {
        push_quad(&mut tris, b[0], b[1], b[2], b[3]);
        push_quad(&mut tris, t[3], t[2], t[1], t[0]);
    }
    flat_mesh(tris, None)
}

/// Half sphere (dome) for parachute canopies; radius r, flat side down at y=0.
pub fn dome(r: f32, rings: u32, segs: u32) -> Mesh {
    let mut pos = Vec::new();
    let mut idx = Vec::new();
    for i in 0..=rings {
        let phi = (i as f32 / rings as f32) * FRAC_PI_2;
        let y = phi.sin() * r;
        let rr = phi.cos() * r;
        for j in 0..=segs {
            let th = (j as f32 / segs as f32) * TAU;
            pos.push([rr * th.cos(), y, rr * th.sin()]);
        }
    }
    let w = segs + 1;
    for i in 0..rings {
        for j in 0..segs {
            let a = i * w + j;
            let b = a + w;
            idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    indexed_mesh(pos, idx)
}
