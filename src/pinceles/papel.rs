//! El papel de la tiza y del carboncillo: ruido fractal en 3D anclado a la posicion en el modelo (no se estira con el
//! UV) mas un grano por texel. Mismo punto, mismo valor: dos trazos sobre el mismo sitio ven el mismo papel.

use glam::Vec3;

use super::azar::{fbm3, hash2};

/// Sal base del papel (el carboncillo usa la suya, derivada de esta).
pub(super) const PAPER_SALT: u64 = 0x5041_5045_5233;
/// Cuanto oscurece el papel donde es oscuro (0.6 = la cobertura baja hasta el 40 %).
pub const PAPER_DEPTH: f32 = 0.6;
/// Tamano de la celda base del ruido del papel, en unidades del modelo (el slider de tamano mide 0.01 por punto).
const PAPER_CELL: f32 = 0.08;
const PAPER_OCTAVES: u32 = 3;

/// Valor del papel (0..1) en la posicion `pos` del modelo. `tx`, `ty` son el texel: el grano fino va por texel, asi
/// que no cambia de un toque a otro.
pub fn paper(pos: Vec3, tx: u32, ty: u32) -> f32 {
    paper_value(pos, tx, ty, PAPER_SALT)
}

/// El papel con su propia semilla (cada pincel de papel usa una distinta).
pub(super) fn paper_value(pos: Vec3, tx: u32, ty: u32, salt: u64) -> f32 {
    let base = fbm3(pos / PAPER_CELL, PAPER_OCTAVES, salt);
    let grain = hash2(tx as i32, ty as i32, salt ^ 0x55);
    ((base - 0.5) * 2.2 + 0.5 + (grain - 0.5) * 0.35).clamp(0.0, 1.0)
}

/// Cuanto deja pasar el papel de la cobertura del trazo (de `1 - PAPER_DEPTH` a 1).
pub fn paper_mask(pos: Vec3, tx: u32, ty: u32) -> f32 {
    1.0 - PAPER_DEPTH + PAPER_DEPTH * paper(pos, tx, ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_papel_esta_en_rango_es_igual_en_el_mismo_punto_y_varia() {
        let mut vals = Vec::new();
        for i in 0..200 {
            let p = Vec3::new(i as f32 * 0.013, 0.3, i as f32 * 0.007);
            let v = paper(p, i, 5);
            assert!((0.0..=1.0).contains(&v));
            assert_eq!(v, paper(p, i, 5));
            vals.push(v);
        }
        let media = vals.iter().sum::<f32>() / vals.len() as f32;
        let var = vals.iter().map(|v| (v - media).abs()).sum::<f32>() / vals.len() as f32;
        assert!(var > 0.03, "el papel es casi plano: {var}");
    }

    #[test]
    fn la_mascara_del_papel_va_de_040_a_1() {
        for i in 0..100 {
            let m = paper_mask(Vec3::new(i as f32 * 0.02, 0.1, 0.4), i, i);
            assert!((1.0 - PAPER_DEPTH - 1e-6..=1.0 + 1e-6).contains(&m), "{m}");
        }
    }
}
