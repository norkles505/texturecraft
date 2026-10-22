//! Mapas auxiliares por UV (ESPECIFICACION.md, seccion 10): mapa de islas y cobertura.
//!
//! * Isla: dos triangulos son de la misma isla si comparten un vertice UV (misma posicion en el mapa). Cada texel
//!   guarda el numero de su isla (1, 2...) o 0 si no lo cubre ningun triangulo, con 2 texeles de margen.
//! * Cobertura: 1 donde algun triangulo cubre el texel, con 1 texel de margen.
//!
//! Se construyen la primera vez que se necesitan (el relleno) y se tiran al cambiar de modelo.

use std::collections::HashMap;

pub struct UvMaps {
    pub size: u32,
    /// Numero de isla por texel (0 = fuera de toda isla). Margen de 2 texeles.
    pub island: Vec<u32>,
    /// 1 donde algun triangulo cubre el texel. Margen de 1 texel.
    pub coverage: Vec<u8>,
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        let p = parent[x as usize];
        parent[x as usize] = parent[p as usize];
        x = parent[x as usize];
    }
    x
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra as usize] = rb;
    }
}

/// Los texeles vacios pegados (4 vecinos) a una isla toman su numero.
fn dilate(map: &mut [u32], size: u32) {
    let s = size as usize;
    let old = map.to_vec();
    for y in 0..s {
        for x in 0..s {
            let i = y * s + x;
            if old[i] != 0 {
                continue;
            }
            let mut v = 0;
            if x > 0 && old[i - 1] != 0 {
                v = old[i - 1];
            } else if x + 1 < s && old[i + 1] != 0 {
                v = old[i + 1];
            } else if y > 0 && old[i - s] != 0 {
                v = old[i - s];
            } else if y + 1 < s && old[i + s] != 0 {
                v = old[i + s];
            }
            map[i] = v;
        }
    }
}

impl UvMaps {
    /// `uvs`: u,v por vertice, de a 3 vertices por triangulo (malla sin indices). Mismo mapeo que el pincel:
    /// texel = uv * (size - 1), y la textura se repite fuera de 0..1.
    pub fn build(uvs: &[f32], size: u32) -> UvMaps {
        let tris = uvs.len() / 6;
        let max = (size - 1) as f32;
        let n = (size * size) as usize;

        // Islas: union-find sobre triangulos que comparten un vertice UV (cuantizado a 20 bits por eje).
        let mut parent: Vec<u32> = (0..tris as u32).collect();
        let mut first: HashMap<(i64, i64), u32> = HashMap::new();
        for t in 0..tris {
            for k in 0..3 {
                let u = uvs[t * 6 + k * 2];
                let v = uvs[t * 6 + k * 2 + 1];
                let key = (
                    (u * 1_048_576.0).round() as i64,
                    (v * 1_048_576.0).round() as i64,
                );
                match first.get(&key) {
                    Some(&other) => union(&mut parent, other, t as u32),
                    None => {
                        first.insert(key, t as u32);
                    }
                }
            }
        }
        // Numeros de isla consecutivos a partir de 1.
        let mut ids: HashMap<u32, u32> = HashMap::new();
        let mut tri_island = vec![0u32; tris];
        for t in 0..tris {
            let root = find(&mut parent, t as u32);
            let next = ids.len() as u32 + 1;
            tri_island[t] = *ids.entry(root).or_insert(next);
        }

        // Rasterizado de cada triangulo (coordenadas baricentricas) con el numero de su isla.
        let mut island = vec![0u32; n];
        for t in 0..tris {
            let mut p = [[0f32; 2]; 3];
            for (k, q) in p.iter_mut().enumerate() {
                q[0] = uvs[t * 6 + k * 2];
                q[1] = uvs[t * 6 + k * 2 + 1];
            }
            // La textura se repite: el triangulo se lleva a 0..1 segun su centro.
            let cu = ((p[0][0] + p[1][0] + p[2][0]) / 3.0).floor();
            let cv = ((p[0][1] + p[1][1] + p[2][1]) / 3.0).floor();
            for q in p.iter_mut() {
                q[0] = (q[0] - cu) * max;
                q[1] = (q[1] - cv) * max;
            }
            let denom = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0])
                + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
            if denom.abs() < 1e-9 {
                continue;
            }
            let min_x = p[0][0].min(p[1][0]).min(p[2][0]).floor().max(0.0) as i32;
            let max_x = p[0][0].max(p[1][0]).max(p[2][0]).ceil().min(max) as i32;
            let min_y = p[0][1].min(p[1][1]).min(p[2][1]).floor().max(0.0) as i32;
            let max_y = p[0][1].max(p[1][1]).max(p[2][1]).ceil().min(max) as i32;
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let (fx, fy) = (x as f32, y as f32);
                    let w0 = ((p[1][1] - p[2][1]) * (fx - p[2][0])
                        + (p[2][0] - p[1][0]) * (fy - p[2][1]))
                        / denom;
                    let w1 = ((p[2][1] - p[0][1]) * (fx - p[2][0])
                        + (p[0][0] - p[2][0]) * (fy - p[2][1]))
                        / denom;
                    let w2 = 1.0 - w0 - w1;
                    if w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4 {
                        island[(y as u32 * size + x as u32) as usize] = tri_island[t];
                    }
                }
            }
        }

        // 1 texel de margen = cobertura; otro mas = mapa de islas (margen de 2).
        dilate(&mut island, size);
        let coverage = island.iter().map(|&i| (i != 0) as u8).collect();
        dilate(&mut island, size);
        UvMaps {
            size,
            island,
            coverage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un cuadrado (2 triangulos que comparten vertices) y un triangulo suelto, en una textura de 64.
    fn two_islands() -> Vec<f32> {
        vec![
            0.1, 0.1, 0.5, 0.1, 0.1, 0.5, //
            0.5, 0.1, 0.5, 0.5, 0.1, 0.5, //
            0.7, 0.7, 0.9, 0.7, 0.7, 0.9,
        ]
    }

    fn at(m: &UvMaps, x: u32, y: u32) -> usize {
        (y * m.size + x) as usize
    }

    #[test]
    fn dos_islas_distintas_y_fuera_vale_cero() {
        let m = UvMaps::build(&two_islands(), 64);
        let a = m.island[at(&m, 19, 19)];
        let b = m.island[at(&m, 50, 50)];
        assert!(a != 0 && b != 0 && a != b, "a = {a}, b = {b}");
        assert_eq!(m.island[at(&m, 0, 63)], 0);
        assert_eq!(m.coverage[at(&m, 0, 63)], 0);
        assert_eq!(m.coverage[at(&m, 19, 19)], 1);
    }

    #[test]
    fn los_dos_triangulos_del_cuadrado_son_una_sola_isla() {
        let m = UvMaps::build(&two_islands(), 64);
        // (12, 12) cae en el primer triangulo y (28, 28) en el segundo.
        assert_eq!(m.island[at(&m, 12, 12)], m.island[at(&m, 28, 28)]);
    }

    #[test]
    fn la_cobertura_tiene_un_texel_de_margen_y_las_islas_dos() {
        let m = UvMaps::build(&two_islands(), 64);
        // El cuadrado va de x = 6.3 a 31.5: el primer texel cubierto es el 7, el 6 es el margen de la cobertura
        // (1 texel) y el 5 el segundo margen de las islas (2 texeles).
        assert_eq!(m.coverage[at(&m, 7, 19)], 1);
        assert_eq!(m.coverage[at(&m, 6, 19)], 1);
        assert_eq!(m.coverage[at(&m, 5, 19)], 0);
        assert_ne!(m.island[at(&m, 5, 19)], 0);
        assert_eq!(m.island[at(&m, 4, 19)], 0);
    }
}
