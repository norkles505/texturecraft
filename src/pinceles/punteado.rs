//! Pincel PUNTEADO (Stipple de photocraft; medio seco): `spatter_tip(64, 77, 40)`, giro y volteo X al azar y dispersion
//! 0.6 en los dos ejes. Su punta es un racimo de gotas sueltas que gira, se voltea y se dispersa en el plano de la
//! superficie. Sin papel ni lienzo: su preset de photocraft no activa ninguna textura.

use std::f32::consts::PI;
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{dab_angle, hash2, unit};
use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Semilla de la punta y cantidad de gotas (las del preset de photocraft).
const STIPPLE_TIP_SEED: u32 = 77;
const STIPPLE_DROPS: u32 = 40;
/// Dispersion del punteado en cada eje (0.6 = hasta el 60 % del radio hacia cada lado, en el plano de la superficie).
const STIPPLE_SCATTER: f32 = 0.6;
/// Lo mas que se aleja el centro de un toque de punteado, como fraccion del radio (la esquina del cuadrado de lado 0.6).
pub const STIPPLE_REACH: f32 = STIPPLE_SCATTER * std::f32::consts::SQRT_2;
const DAB_FLIP_SALT: u64 = 0x464C_4950;
const DAB_SCATTER_SALT: u64 = 0x5343_4154_54;
const DAB_SCATTER_Y_SALT: u64 = 0x5343_4154_5459;

/// La punta del punteado (`spatter_tip(64, seed, drops)` de photocraft): un racimo de gotas redondas de tamanos
/// distintos. Cada gota sale de tres numeros al azar: el angulo, la distancia al centro (hasta el 38 % del lado) y
/// el radio (de 3 a 12 % del lado, mas gotas chicas que grandes). El valor de un texel es el de la gota que mas lo
/// cubre, con un borde suave de un texel. Usa el azar de este programa, asi que la disposicion de las gotas no es
/// la misma que la de photocraft (que usa otro generador), pero la forma y las proporciones si.
pub(super) fn construir(seed: u32, drops: u32) -> Punta {
    let salt = u64::from(seed) + 202;
    let n = TIP_SIZE;
    let s = n as f32;
    let blobs: Vec<(f32, f32, f32)> = (0..drops)
        .map(|i| {
            let a = hash2(i as i32, 0, salt) * 2.0 * PI;
            let d = hash2(i as i32, 1, salt).sqrt() * 0.38 * s;
            let r = (0.03 + 0.09 * hash2(i as i32, 2, salt).powi(2)) * s;
            (s / 2.0 + a.cos() * d, s / 2.0 + a.sin() * d, r)
        })
        .collect();
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let v = blobs.iter().fold(0.0f32, |acc, &(bx, by, r)| {
                let d = ((px - bx).powi(2) + (py - by).powi(2)).sqrt();
                acc.max((r + 0.5 - d).clamp(0.0, 1.0))
            });
            data.push(v);
        }
    }
    Punta::new(data)
}

/// La punta del punteado (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| construir(STIPPLE_TIP_SEED, STIPPLE_DROPS))
}

/// Un toque de punteado: la punta de gotas gira al azar, se voltea la mitad de las veces y el centro se desplaza en
/// los dos ejes del plano de la superficie (`t0`, `b0`) hasta `STIPPLE_SCATTER` del radio por eje (`STIPPLE_REACH` en
/// total).
pub(super) fn nuevo(dab: u32, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    // Giro al azar (jitter 1.0 de photocraft), volteo horizontal en la mitad de los toques y dispersion en los dos
    // ejes del plano de la superficie (no solo de lado): cada eje se aleja hasta el 60 % del radio.
    let (sn, cs) = dab_angle(dab).sin_cos();
    let flip = if unit(DAB_FLIP_SALT.wrapping_add(u64::from(dab))) < 0.5 {
        -1.0
    } else {
        1.0
    };
    let k = STIPPLE_SCATTER * radius;
    let kx = (unit(DAB_SCATTER_SALT.wrapping_add(u64::from(dab))) * 2.0 - 1.0) * k;
    let ky = (unit(DAB_SCATTER_Y_SALT.wrapping_add(u64::from(dab))) * 2.0 - 1.0) * k;
    DryDab {
        kind: DryKind::Stipple,
        t: (t0 * cs + b0 * sn) * flip,
        b: b0 * cs - t0 * sn,
        inv_radius,
        roundness: 1.0,
        shift: t0 * kx + b0 * ky,
        follaje: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::dry_subtract;

    #[test]
    fn la_punta_del_punteado_son_gotas_sueltas_y_es_determinista() {
        let a = construir(STIPPLE_TIP_SEED, STIPPLE_DROPS);
        assert_eq!(a.data, construir(STIPPLE_TIP_SEED, STIPPLE_DROPS).data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        let pintados = a.data.iter().filter(|v| **v > 0.0).count();
        // Tiene pintura, pero mucha menos que un disco entero: son gotas con huecos entre ellas.
        assert!(
            pintados > 300 && pintados < n * n * 6 / 10,
            "pintados {pintados}"
        );
        // Dentro de la zona donde caen las gotas (a menos de 24 texeles del centro) hay huecos.
        let c = n as f32 / 2.0;
        let huecos = (0..n * n)
            .filter(|&i| {
                let (x, y) = ((i % n) as f32 + 0.5 - c, (i / n) as f32 + 0.5 - c);
                x.hypot(y) < 24.0 && a.data[i] == 0.0
            })
            .count();
        assert!(huecos > 50, "huecos {huecos}");
        // La esquina queda vacia.
        assert_eq!(a.data[0], 0.0);
    }

    #[test]
    fn el_punteado_se_dispersa_en_los_dos_ejes_y_no_lleva_papel() {
        let radio = 2.0;
        let (mut en_x, mut en_y, mut volteos) = (false, false, [false, false]);
        for dab in 1..200 {
            let d = DryDab::new(DryKind::Stipple, dab, Vec3::Z, Vec3::X, radio);
            assert!(
                d.reach() <= STIPPLE_REACH * radio + 1e-5,
                "reach {}",
                d.reach()
            );
            // Cada eje se aleja a lo sumo el 60 % del radio.
            assert!(
                d.shift.x.abs() <= STIPPLE_SCATTER * radio + 1e-5
                    && d.shift.y.abs() <= STIPPLE_SCATTER * radio + 1e-5
            );
            en_x |= d.shift.x.abs() > 0.1;
            en_y |= d.shift.y.abs() > 0.1;
            assert_eq!(d.roundness, 1.0);
            // El volteo cambia el sentido del eje t respecto al b (el determinante de la base).
            volteos[(d.t.cross(d.b).dot(Vec3::Z) > 0.0) as usize] = true;
        }
        assert!(en_x && en_y, "no se dispersa en los dos ejes");
        assert!(
            volteos[0] && volteos[1],
            "no se voltea en ningun toque o en todos"
        );
        // Sin papel ni lienzo: no resta nada.
        assert_eq!(dry_subtract(DryKind::Stipple, Vec3::ONE, 5, 9), 0.0);
        // Mismo toque, misma forma.
        let a = DryDab::new(DryKind::Stipple, 9, Vec3::Z, Vec3::X, 1.0);
        let b = DryDab::new(DryKind::Stipple, 9, Vec3::Z, Vec3::X, 1.0);
        assert_eq!(
            a.alpha(Vec3::new(0.2, -0.1, 0.0)),
            b.alpha(Vec3::new(0.2, -0.1, 0.0))
        );
    }
}
