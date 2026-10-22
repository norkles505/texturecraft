//! Pincel de CARBONCILLO (portado de photocraft; medio seco). Su punta es una elipse achatada con vetas que sigue la
//! direccion del trazo. Su papel (semilla 5, profundidad 0.7, modo Subtract invertido) RESTA cobertura al trazo ya
//! acumulado, no a cada toque (ver `medio_seco.rs`).

use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{hash2, value_noise2};
use super::medio_seco::{DryDab, DryKind};
use super::papel::{PAPER_SALT, paper_value};
use super::punta::{Punta, TIP_SIZE};

/// Semilla de la punta (la de photocraft: 4).
const CHARCOAL_TIP_SEED: u32 = 4;
/// Papel del carboncillo: semilla 5, profundidad 0.7, modo Subtract invertido.
const CHARCOAL_PAPER_SALT: u64 = PAPER_SALT + 5;
pub(super) const CHARCOAL_PAPER_DEPTH: f32 = 0.7;

/// La punta del carboncillo (`charcoal_tip(64, seed)` de photocraft): una elipse achatada (el eje vertical cuenta
/// 1.6 veces, asi que es mas ancha que alta) de vetas horizontales cortadas, con el borde granuloso y un 15 % de
/// granos casi vacios.
fn construir(seed: u32) -> Punta {
    let salt = u64::from(seed) + 404;
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = ((x as f32 + 0.5 - c) / c, (y as f32 + 0.5 - c) / c);
            let r = (dx * dx + (dy * 1.6).powi(2)).sqrt();
            let edge = 0.85 + 0.15 * value_noise2(dx * 6.0 + 9.0, dy * 6.0 + 9.0, 64, salt);
            if r > edge {
                data.push(0.0);
                continue;
            }
            // Vetas: la densidad de cada fila se modula a lo largo de x para que se corten.
            let row = hash2(0, y as i32, salt);
            let along = value_noise2(x as f32 / 6.0, y as f32 / 1.5, 1 << 12, salt ^ 0x77);
            let grit = hash2(x as i32, y as i32, salt ^ 0x13);
            let v = (0.35 + 0.65 * row) * (0.4 + 0.6 * along) * if grit < 0.15 { 0.2 } else { 1.0 };
            data.push(v * ((edge - r) * 6.0).clamp(0.0, 1.0));
        }
    }
    Punta::new(data)
}

/// La punta del carboncillo (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| construir(CHARCOAL_TIP_SEED))
}

/// Cuanta cobertura resta el papel en un texel: `profundidad * (1 - papel)`. Va de 0 a `CHARCOAL_PAPER_DEPTH`.
pub(super) fn subtract(pos: Vec3, tx: u32, ty: u32) -> f32 {
    CHARCOAL_PAPER_DEPTH * (1.0 - paper_value(pos, tx, ty, CHARCOAL_PAPER_SALT))
}

/// Un toque de carboncillo: la punta apunta en la direccion del trazo (`along`, ya proyectada en el plano de la
/// superficie), sin variacion de tamano ni dispersion. Si no se sabe hacia donde va (primer toque) usa la base fija
/// `t0`.
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    let t = along.unwrap_or(t0);
    DryDab {
        kind: DryKind::Charcoal,
        t,
        b: n.cross(t),
        inv_radius,
        roundness: 1.0,
        shift: Vec3::ZERO,
        follaje: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_punta_del_carboncillo_es_mas_ancha_que_alta() {
        let t = punta();
        let n = TIP_SIZE;
        let fila = (0..n).filter(|&x| t.data[(n / 2) * n + x] > 0.0).count();
        let columna = (0..n).filter(|&y| t.data[y * n + n / 2] > 0.0).count();
        assert!(fila > columna + 10, "fila {fila}, columna {columna}");
    }

    #[test]
    fn el_carboncillo_sigue_la_direccion_del_trazo() {
        // El trazo va por Y: la punta es larga a lo largo de Y y corta de lado.
        let d = DryDab::new(DryKind::Charcoal, 1, Vec3::Z, Vec3::Y, 1.0);
        let suma = |dir: Vec3| {
            (-9..=9)
                .map(|k| d.alpha(dir * (k as f32 / 10.0)))
                .sum::<f32>()
        };
        assert!(
            suma(Vec3::Y) > suma(Vec3::X) * 1.2,
            "{} no pasa de {}",
            suma(Vec3::Y),
            suma(Vec3::X)
        );
        // Sin direccion (primer toque) usa una base fija y no falla.
        let sin = DryDab::new(DryKind::Charcoal, 1, Vec3::Z, Vec3::ZERO, 1.0);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
    }
}
