//! BVH de triangulos para el raycast de pantalla a textura (ESPECIFICACION.md, seccion 9.2).
//!
//! Möller–Trumbore sin descartar caras traseras (descarta `|det| < 1e-9`, acepta `t > 1e-5`). Cajas con holgura
//! `1e-5`, hojas de hasta 4 triangulos, particion por la mediana de los centros en el eje donde mas se separan.
//! El recorrido visita primero la caja mas cercana para podar con el mejor `t`.

use glam::Vec3;

use crate::obj::ObjMesh;

const PAD: f32 = 1e-5;
const LEAF: usize = 4;

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    /// Distancia a lo largo de `dir` (en unidades de `dir`, que no hace falta normalizar).
    pub t: f32,
    /// UV interpolada del punto tocado.
    pub u: f32,
    pub v: f32,
    /// Indice del triangulo tocado.
    pub tri: usize,
}

struct Node {
    min: Vec3,
    max: Vec3,
    /// Hijos (solo nodos internos).
    left: usize,
    right: usize,
    /// Rango en `order` (solo hojas, `count > 0`).
    start: usize,
    count: usize,
}

pub struct TriBvh {
    tris: Vec<[Vec3; 3]>,
    uvs: Vec<[[f32; 2]; 3]>,
    order: Vec<u32>,
    nodes: Vec<Node>,
    /// Triangulos que el rayo puede tocar (los de mallas ocultas se saltan: el rayo los atraviesa).
    visible: Vec<bool>,
}

impl TriBvh {
    /// Construye el BVH de una malla sin indices (cada 3 vertices = un triangulo).
    pub fn new(mesh: &ObjMesh) -> TriBvh {
        let n = mesh.vertex_count() / 3;
        let pos = |i: usize| {
            Vec3::new(
                mesh.positions[i * 3],
                mesh.positions[i * 3 + 1],
                mesh.positions[i * 3 + 2],
            )
        };
        let uv = |i: usize| [mesh.uvs[i * 2], mesh.uvs[i * 2 + 1]];
        let tris: Vec<[Vec3; 3]> = (0..n)
            .map(|t| [pos(t * 3), pos(t * 3 + 1), pos(t * 3 + 2)])
            .collect();
        let uvs: Vec<[[f32; 2]; 3]> = (0..n)
            .map(|t| [uv(t * 3), uv(t * 3 + 1), uv(t * 3 + 2)])
            .collect();
        let cent: Vec<Vec3> = tris.iter().map(|t| (t[0] + t[1] + t[2]) / 3.0).collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = Vec::new();
        if n > 0 {
            build(&tris, &cent, &mut order, 0, &mut nodes);
        }
        TriBvh {
            tris,
            uvs,
            order,
            nodes,
            visible: vec![true; n],
        }
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// Elige que triangulos puede tocar el rayo (uno por triangulo; si la cantidad no coincide no se cambia nada).
    /// La estructura del arbol no cambia: los ocultos solo se saltan al recorrer.
    pub fn set_visible(&mut self, visible: Vec<bool>) {
        if visible.len() == self.tris.len() {
            self.visible = visible;
        }
    }

    /// Posiciones y UVs de los 3 vertices del triangulo `i`.
    pub fn tri(&self, i: usize) -> (&[Vec3; 3], &[[f32; 2]; 3]) {
        (&self.tris[i], &self.uvs[i])
    }

    /// Triangulos visibles cuya caja toca la esfera `(center, radius)`, en `out` (se vacia antes). Es la caja del
    /// triangulo, no el triangulo exacto: de sobra para el pincel, que despues mide la distancia de cada texel.
    pub fn query_sphere(&self, center: Vec3, radius: f32, out: &mut Vec<usize>) {
        out.clear();
        if self.nodes.is_empty() {
            return;
        }
        let r2 = radius * radius;
        let mut stack: Vec<usize> = Vec::with_capacity(128);
        stack.push(0);
        while let Some(ni) = stack.pop() {
            let node = &self.nodes[ni];
            if box_dist2(node.min, node.max, center) > r2 {
                continue;
            }
            if node.count > 0 {
                for &ti in &self.order[node.start..node.start + node.count] {
                    let ti = ti as usize;
                    if !self.visible[ti] {
                        continue;
                    }
                    let t = &self.tris[ti];
                    let (mn, mx) = (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]));
                    if box_dist2(mn, mx, center) <= r2 {
                        out.push(ti);
                    }
                }
            } else {
                stack.push(node.left);
                stack.push(node.right);
            }
        }
    }

    /// Primer triangulo que toca el rayo `origin + t * dir` (el de menor `t`).
    pub fn pick(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::ONE / dir;
        let mut best: Option<Hit> = None;
        let mut best_t = f32::MAX;
        // Pila de (nodo, distancia de entrada). 128 sobra: la profundidad es ~log2(n).
        let mut stack: Vec<(usize, f32)> = Vec::with_capacity(128);
        if let Some(t0) = hit_box(&self.nodes[0], origin, inv) {
            stack.push((0, t0));
        }
        while let Some((ni, t_enter)) = stack.pop() {
            if t_enter >= best_t {
                continue;
            }
            let node = &self.nodes[ni];
            if node.count > 0 {
                for &ti in &self.order[node.start..node.start + node.count] {
                    if !self.visible[ti as usize] {
                        continue;
                    }
                    if let Some(h) =
                        hit_tri(&self.tris[ti as usize], &self.uvs[ti as usize], origin, dir)
                    {
                        if h.0 < best_t {
                            best_t = h.0;
                            best = Some(Hit {
                                t: h.0,
                                u: h.1,
                                v: h.2,
                                tri: ti as usize,
                            });
                        }
                    }
                }
            } else {
                let l = hit_box(&self.nodes[node.left], origin, inv);
                let r = hit_box(&self.nodes[node.right], origin, inv);
                // Se apila primero el mas lejano para sacar antes el mas cercano.
                match (l, r) {
                    (Some(a), Some(b)) => {
                        if a <= b {
                            stack.push((node.right, b));
                            stack.push((node.left, a));
                        } else {
                            stack.push((node.left, a));
                            stack.push((node.right, b));
                        }
                    }
                    (Some(a), None) => stack.push((node.left, a)),
                    (None, Some(b)) => stack.push((node.right, b)),
                    (None, None) => {}
                }
            }
        }
        best
    }

    /// Recorrido de todos los triangulos (solo para comprobar que el BVH da lo mismo).
    #[cfg(test)]
    fn pick_brute(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        let mut best: Option<Hit> = None;
        for (i, (t, uv)) in self.tris.iter().zip(&self.uvs).enumerate() {
            if !self.visible[i] {
                continue;
            }
            if let Some(h) = hit_tri(t, uv, origin, dir) {
                if best.is_none_or(|b| h.0 < b.t) {
                    best = Some(Hit {
                        t: h.0,
                        u: h.1,
                        v: h.2,
                        tri: i,
                    });
                }
            }
        }
        best
    }
}

fn build(
    tris: &[[Vec3; 3]],
    cent: &[Vec3],
    order: &mut [u32],
    base: usize,
    nodes: &mut Vec<Node>,
) -> usize {
    let (mut mn, mut mx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let (mut cmn, mut cmx) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for &i in order.iter() {
        for v in tris[i as usize] {
            mn = mn.min(v);
            mx = mx.max(v);
        }
        cmn = cmn.min(cent[i as usize]);
        cmx = cmx.max(cent[i as usize]);
    }
    let idx = nodes.len();
    nodes.push(Node {
        min: mn - Vec3::splat(PAD),
        max: mx + Vec3::splat(PAD),
        left: 0,
        right: 0,
        start: base,
        count: order.len(),
    });
    let ext = cmx - cmn;
    let axis = if ext.x >= ext.y && ext.x >= ext.z {
        0
    } else if ext.y >= ext.z {
        1
    } else {
        2
    };
    // Pocos triangulos, o todos con el mismo centro (no se pueden separar): queda como hoja.
    if order.len() <= LEAF || ext[axis] <= 0.0 {
        return idx;
    }
    let mid = order.len() / 2;
    order.select_nth_unstable_by(mid, |&a, &b| {
        cent[a as usize][axis]
            .partial_cmp(&cent[b as usize][axis])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let (l, r) = order.split_at_mut(mid);
    let left = build(tris, cent, l, base, nodes);
    let right = build(tris, cent, r, base + mid, nodes);
    nodes[idx].left = left;
    nodes[idx].right = right;
    nodes[idx].count = 0;
    idx
}

/// Distancia al cuadrado del punto `c` a la caja `[min, max]` (0 si esta dentro).
fn box_dist2(min: Vec3, max: Vec3, c: Vec3) -> f32 {
    (min - c).max(c - max).max(Vec3::ZERO).length_squared()
}

/// Distancia de entrada del rayo a la caja (0 si el origen esta dentro), o `None` si no la toca.
fn hit_box(n: &Node, origin: Vec3, inv: Vec3) -> Option<f32> {
    let t1 = (n.min - origin) * inv;
    let t2 = (n.max - origin) * inv;
    let t_near = t1.min(t2).max_element();
    let t_far = t1.max(t2).min_element();
    if t_far >= t_near.max(0.0) {
        Some(t_near.max(0.0))
    } else {
        None
    }
}

/// Möller–Trumbore. Devuelve `(t, u, v)` con la UV interpolada.
fn hit_tri(t: &[Vec3; 3], uv: &[[f32; 2]; 3], o: Vec3, d: Vec3) -> Option<(f32, f32, f32)> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - t[0];
    let bu = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&bu) {
        return None;
    }
    let q = s.cross(e1);
    let bv = d.dot(q) * inv;
    if bv < 0.0 || bu + bv > 1.0 {
        return None;
    }
    let dist = e2.dot(q) * inv;
    if dist <= 1e-5 {
        return None;
    }
    let w0 = 1.0 - bu - bv;
    Some((
        dist,
        w0 * uv[0][0] + bu * uv[1][0] + bv * uv[2][0],
        w0 * uv[0][1] + bu * uv[1][1] + bv * uv[2][1],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El BVH tiene que dar exactamente lo mismo que recorrer todos los triangulos.
    #[test]
    fn bvh_igual_que_recorrido_completo() {
        let mesh = ObjMesh::uv_sphere(48, 24);
        let bvh = TriBvh::new(&mesh);
        let mut seed = 12345u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16777216.0
        };
        let (mut aciertos, mut fallos) = (0, 0);
        for _ in 0..2000 {
            let origin = Vec3::new(rnd() * 4.0 - 2.0, -5.0 - rnd(), rnd() * 4.0 - 2.0);
            let target = Vec3::new(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
            let dir = target - origin;
            let a = bvh.pick(origin, dir);
            let b = bvh.pick_brute(origin, dir);
            match (a, b) {
                (Some(a), Some(b)) => {
                    assert!((a.t - b.t).abs() < 1e-4, "t distinto: {} vs {}", a.t, b.t);
                    aciertos += 1;
                }
                (None, None) => fallos += 1,
                _ => panic!("uno acierta y el otro no"),
            }
        }
        assert!(
            aciertos > 100 && fallos > 100,
            "la prueba no cubre aciertos y fallos ({aciertos}/{fallos})"
        );
    }

    /// La consulta por esfera tiene que dar los mismos triangulos que recorrerlos todos con la misma prueba de cajas.
    #[test]
    fn query_sphere_igual_que_recorrido_completo() {
        let mesh = ObjMesh::uv_sphere(32, 16);
        let bvh = TriBvh::new(&mesh);
        let mut out = Vec::new();
        for (c, r) in [
            (Vec3::new(0.0, -0.5, 0.0), 0.1),
            (Vec3::new(0.2, 0.1, 0.3), 0.25),
            (Vec3::new(5.0, 5.0, 5.0), 0.5),
        ] {
            bvh.query_sphere(c, r, &mut out);
            out.sort_unstable();
            let mut brute: Vec<usize> = (0..bvh.triangle_count())
                .filter(|&i| {
                    let t = &bvh.tris[i];
                    box_dist2(t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]), c) <= r * r
                })
                .collect();
            brute.sort_unstable();
            assert_eq!(out, brute, "centro {c:?} radio {r}");
        }
        // Un punto lejano no trae nada; uno sobre la superficie trae algo.
        bvh.query_sphere(Vec3::new(5.0, 5.0, 5.0), 0.5, &mut out);
        assert!(out.is_empty());
        bvh.query_sphere(Vec3::new(0.0, -0.5, 0.0), 0.1, &mut out);
        assert!(!out.is_empty());
    }

    /// Los triangulos de una malla oculta se atraviesan; al volver a mostrarlos se tocan otra vez.
    #[test]
    fn los_triangulos_ocultos_se_atraviesan() {
        let mesh = ObjMesh::uv_sphere(16, 8);
        let n = mesh.triangle_count();
        let mut bvh = TriBvh::new(&mesh);
        let (origin, dir) = (Vec3::new(0.01, -5.0, 0.01), Vec3::Y);
        assert!(bvh.pick(origin, dir).is_some());
        bvh.set_visible(vec![false; n]);
        assert!(bvh.pick(origin, dir).is_none());
        assert!(bvh.pick_brute(origin, dir).is_none());
        bvh.set_visible(vec![true; n]);
        assert!(bvh.pick(origin, dir).is_some());
        // Una lista de otro tamano no se acepta.
        bvh.set_visible(vec![false; n + 1]);
        assert!(bvh.pick(origin, dir).is_some());
    }
}
