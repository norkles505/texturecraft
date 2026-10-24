//! Punta del pincel ARCANE (`BrushType::Arcane`, de ibisPaint): una ELIPSE APLASTADA de borde casi duro, ancha de lado a
//! lado del trazo y de poco alto (en la direccion en que se mueve). Los desvanecidos de inicio y de fin estan en
//! `desvanecido.rs`.
//!
//! Como se llego a esta forma (medido en las capturas de 3 trazos verticales del pincel, radio de 27 px; el QR no trae
//! la imagen de la punta, asi que NO se puede leer, solo deducir):
//!
//! - El frente del trazo es mucho mas plano que un circulo: el ancho (en fraccion del ancho total) a 0, 2, 4, 6, 8 y 10 px
//!   del inicio es 0.19, 0.45, 0.79, 0.95, 1.04 y 1.05. Un circulo da 0.13, 0.39, 0.54, 0.61, 0.72 y 0.76; un cuadrado
//!   redondeado (la primera version) 0.35, 0.69, 0.80, 0.83, 0.91 y 0.91 (llega muy ancho al principio y tarda en
//!   llenarse).
//! - Se simulo el trazo completo (toques cada 0.05 radios con el desvanecido de inicio de `desvanecido.rs`) con elipses
//!   de distinta altura y se comparo con esas medidas: la que mejor calza (error 0.05, contra 0.43 del circulo y 0.15 del
//!   cuadrado) tiene el alto en 0.42 del ancho: 0.13, 0.57, 0.76, 0.87, 0.94 y 0.98. Es un calce aproximado, no exacto.
//! - Los lados del trazo son rectos y de 54 px (el ancho completo de la elipse), y el borde pasa de 0 a 1 en unos 0.08
//!   radios (en las capturas, 2 a 3 px de 27).
//!
//! Orientacion: la elipse es ancha de lado a lado del trazo (`b` va a lo largo del movimiento), asi el ancho del trazo es el
//! mismo vaya hacia donde vaya. No se sabe si en ibisPaint la punta gira con el trazo o queda fija (en las capturas solo hay
//! trazos verticales). Sin la direccion (primer toque) usa una base fija de la superficie.

use std::sync::OnceLock;

use glam::Vec3;

use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Alto de la elipse respecto de su ancho (redondez): 0.42 (medido: ver arriba).
const ROUNDNESS: f32 = 0.42;
/// Cuanto tarda el borde en pasar de 0 a 1, en radios hacia adentro.
const EDGE: f32 = 0.08;

/// La punta: un disco de borde casi duro de radio 1 (el borde del bitmap); el toque lo aplasta con `ROUNDNESS`.
fn construir() -> Punta {
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5 - c) / c, (y as f32 + 0.5 - c) / c);
            data.push(((1.0 - u.hypot(v)) / EDGE).clamp(0.0, 1.0));
        }
    }
    Punta::new(data)
}

/// La punta de Arcane (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Un toque de Arcane: la elipse es ancha de lado a lado del trazo (`along` es la direccion del trazo, ya proyectada en
/// el plano de la superficie) y aplastada a lo largo de el. Sin direccion usa la base fija `t0` (ancha en ese eje).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    let t = along.map_or(t0, |a| n.cross(a));
    DryDab {
        kind: DryKind::Arcane,
        t,
        b: n.cross(t),
        inv_radius,
        roundness: ROUNDNESS,
        shift: Vec3::ZERO,
        follaje: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_punta_es_un_disco_de_borde_casi_duro_y_es_determinista() {
        let a = construir();
        assert_eq!(a.data, construir().data);
        assert_eq!(a.data, punta().data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        // Centro lleno, esquinas vacias.
        assert_eq!(a.data[(n / 2) * n + n / 2], 1.0);
        for i in [0, n - 1, n * (n - 1), n * n - 1] {
            assert_eq!(a.data[i], 0.0, "esquina {i}");
        }
        // Simetrica en los dos ejes y en la diagonal.
        for y in 0..n {
            for x in 0..n {
                let v = a.data[y * n + x];
                assert!((v - a.data[y * n + (n - 1 - x)]).abs() < 1e-5);
                assert!((v - a.data[(n - 1 - y) * n + x]).abs() < 1e-5);
                assert!((v - a.data[x * n + y]).abs() < 1e-5);
            }
        }
        // Borde casi duro: la franja que baja de 1 a 0 es chica (menos del 25 % de los texeles pintados).
        let pintados = a.data.iter().filter(|v| **v > 0.0).count();
        let suaves = a.data.iter().filter(|v| **v > 0.0 && **v < 1.0).count();
        assert!(suaves * 4 < pintados, "borde blando: {suaves} de {pintados}");
    }

    #[test]
    fn el_toque_es_una_elipse_ancha_de_lado_a_lado_del_trazo_y_aplastada_a_lo_largo() {
        let radio = 2.0;
        // El trazo va por X: la elipse es ancha en Y (de lado a lado del trazo) y baja en X.
        let d = DryDab::new(DryKind::Arcane, 1, Vec3::Z, Vec3::X, radio);
        assert_eq!(d.roundness, ROUNDNESS);
        assert_eq!(d.shift, Vec3::ZERO);
        assert_eq!(d.reach(), 0.0);
        assert!(d.alpha(Vec3::ZERO) > 0.999);
        // De lado a lado llega al radio completo; a lo largo del trazo solo a 0.42 radios.
        assert!(d.alpha(Vec3::new(0.0, 0.9 * radio, 0.0)) > 0.99);
        assert_eq!(d.alpha(Vec3::new(0.0, 1.01 * radio, 0.0)), 0.0);
        assert!(d.alpha(Vec3::new(0.3 * radio, 0.0, 0.0)) > 0.99);
        assert_eq!(d.alpha(Vec3::new(0.5 * radio, 0.0, 0.0)), 0.0);
        // Con el trazo por Y se gira: ahora es ancha en X.
        let y = DryDab::new(DryKind::Arcane, 1, Vec3::Z, Vec3::Y, radio);
        assert!(y.alpha(Vec3::new(0.9 * radio, 0.0, 0.0)) > 0.99);
        assert_eq!(y.alpha(Vec3::new(0.0, 0.5 * radio, 0.0)), 0.0);
        // Sin direccion (primer toque) no falla.
        let sin = DryDab::new(DryKind::Arcane, 1, Vec3::Z, Vec3::ZERO, radio);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
        // Sin azar: todos los toques son iguales.
        let p = Vec3::new(0.3, 0.9, 0.0);
        for dab in 1..50 {
            let e = DryDab::new(DryKind::Arcane, dab, Vec3::Z, Vec3::X, radio);
            assert_eq!(e.alpha(p), d.alpha(p));
        }
    }
}
