//! Pincel de PASTEL (portado de photocraft; medio seco). Su punta es la de grano de la tiza con otra semilla; gira en
//! cada toque, se aplasta un poco y se dispersa de lado respecto al trazo. Su lienzo (baldosa de 64, semilla 2,
//! profundidad 0.55, modo Subtract invertido) RESTA cobertura al trazo ya acumulado, no a cada toque (ver
//! `medio_seco.rs`).

use std::f32::consts::PI;
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{dab_angle, hash2, unit};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;
use super::tiza::construir_grano;

/// Semilla de la punta (la de photocraft: 31).
const PASTEL_TIP_SEED: u32 = 31;
/// Lienzo del pastel: baldosa de 64, semilla 2, profundidad 0.55, modo Subtract invertido.
pub(super) const PASTEL_CANVAS_DEPTH: f32 = 0.55;
const CANVAS_SALT: u64 = 2 * 0x0100_0000_01B3 + 17;
/// Un hilo del lienzo mide 8 texeles y la trama se repite cada 64.
const CANVAS_PERIOD: f32 = 8.0;
const CANVAS_TILE: u32 = 64;
/// Cuanto varia la redondez de cada toque de pastel (0.3 = del 70 al 100 %).
const ROUNDNESS_JITTER: f32 = 0.3;
const DAB_ROUND_SALT: u64 = 0x524F_554E_44;
const DAB_SCATTER_SALT: u64 = 0x5343_4154_54;
/// Dispersion del pastel: hasta esta fraccion del radio, de lado respecto al trazo (0.25 = 25 %).
pub const PASTEL_SCATTER: f32 = 0.25;

/// La punta del pastel (la de la tiza con otra semilla; se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| construir_grano(PASTEL_TIP_SEED))
}

/// Valor del lienzo (0..1) en el texel `(tx, ty)`: una trama de hilos de `CANVAS_PERIOD` texeles que pasan por encima y
/// por debajo, con un poco de grano. Se repite cada `CANVAS_TILE` texeles: esta anclado a la textura (el UV), no a la
/// pantalla.
fn canvas_value(tx: u32, ty: u32) -> f32 {
    let (x, y) = (tx % CANVAS_TILE, ty % CANVAS_TILE);
    let cell = |v: u32| (v as f32 / CANVAS_PERIOD).floor();
    let within = |v: u32| (v as f32 % CANVAS_PERIOD) / CANVAS_PERIOD;
    let over = (cell(x) + cell(y)) as u32 % 2 == 0;
    // Hilo horizontal (varia en y) o vertical (varia en x), segun quien este por encima en esta celda.
    let thread = if over { (PI * within(y)).sin() } else { (PI * within(x)).sin() };
    let grain = hash2(x as i32, y as i32, CANVAS_SALT);
    ((0.2 + 0.8 * thread) * (0.85 + 0.15 * grain)).clamp(0.0, 1.0)
}

/// Cuanta cobertura resta el lienzo en un texel: `profundidad * (1 - valor)`. Va de 0 a `PASTEL_CANVAS_DEPTH`.
pub(super) fn subtract(tx: u32, ty: u32) -> f32 {
    PASTEL_CANVAS_DEPTH * (1.0 - canvas_value(tx, ty))
}

/// Un toque de pastel: la punta gira un angulo al azar, se aplasta hasta `ROUNDNESS_JITTER` y el centro se desplaza de
/// lado respecto al trazo hasta `PASTEL_SCATTER` del radio. `t0` y `b0` son la base de la superficie, `n` su normal y
/// `along` la direccion del trazo proyectada en el plano (si no se sabe hacia donde va, la dispersion va a lo largo
/// de un eje cualquiera).
pub(super) fn nuevo(dab: u32, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    let (sn, cs) = dab_angle(dab).sin_cos();
    let roundness = 1.0 - ROUNDNESS_JITTER * unit(DAB_ROUND_SALT.wrapping_add(u64::from(dab)));
    let side = along.map_or(b0, |a| n.cross(a));
    let k = (unit(DAB_SCATTER_SALT.wrapping_add(u64::from(dab))) * 2.0 - 1.0) * PASTEL_SCATTER * radius;
    DryDab {
        kind: DryKind::Pastel,
        t: t0 * cs + b0 * sn,
        b: b0 * cs - t0 * sn,
        inv_radius,
        roundness,
        shift: side * k,
        follaje: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_pastel_se_dispersa_solo_de_lado() {
        let mut algun_desvio = false;
        for dab in 1..100 {
            let d = DryDab::new(DryKind::Pastel, dab, Vec3::Z, Vec3::X, 2.0);
            assert!(d.shift.dot(Vec3::X).abs() < 1e-4, "se desplaza a lo largo del trazo");
            assert!(d.reach() <= PASTEL_SCATTER * 2.0 + 1e-5);
            assert!((1.0 - ROUNDNESS_JITTER - 1e-6..=1.0).contains(&d.roundness));
            algun_desvio |= d.reach() > 0.05;
        }
        assert!(algun_desvio);
        // El carboncillo no se dispersa.
        assert_eq!(DryDab::new(DryKind::Charcoal, 5, Vec3::Z, Vec3::X, 2.0).reach(), 0.0);
    }

    #[test]
    fn el_lienzo_se_repite_cada_baldosa() {
        for (x, y) in [(0, 0), (3, 10), (17, 40), (63, 63)] {
            assert_eq!(canvas_value(x, y), canvas_value(x + CANVAS_TILE, y));
            assert_eq!(canvas_value(x, y), canvas_value(x, y + 3 * CANVAS_TILE));
        }
    }
}
