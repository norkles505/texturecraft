//! Pincel ESPONJA DE TINTA (Ink Wash Sponge de photocraft; humedo): `sponge_tip(96, 8)` como punta, giro libre, tamano
//! un poco distinto en cada toque, flujo 0.6 y BORDES MOJADOS. Sin papel, sin dispersion y sin color por toque.
//!
//! Lo que copia del preset `Ink Wash Sponge` de photocraft (`presets.rs`):
//!
//! - Punta: un disco de borde irregular lleno de poros (huecos de tamanos distintos), igual que `sponge_tip`.
//! - Tamano: 70 de diametro (radio 35), cada toque mide del 80 al 100 % del radio (jitter 0.2).
//! - Giro: libre (jitter 1.0), cada toque mira a un lado distinto.
//! - Espaciado: 0.3 del diametro (0.6 del radio). Flujo: 0.6 (`BrushType::default_flow`).
//! - Bordes mojados (`wet_edges`): el interior del trazo queda mas claro que su borde. Se aplica al TRAZO, no a cada
//!   toque: cada texel recuerda lo mas hondo que lo alcanzo una punta (el valor de la punta en ese texel) y, al
//!   mezclar con el color, la cobertura acumulada se multiplica por `wet_factor`: la mitad donde la punta pinto del
//!   todo y hasta la cobertura entera en el borde (`paint.rs` guarda esa profundidad en `stroke_depth`).
//!
//! Diferencia con photocraft: la punta se genera de 64 x 64 (el lado que usan todas las puntas de texturecraft) y no
//! de 96 x 96; los poros se calculan sobre el tamano de la punta, asi que la forma es la misma, solo menos fina.

use std::f32::consts::PI;
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{dab_angle, unit, value_noise2};
use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Semilla de la punta (`sponge_tip(96, 8)`).
const SPONGE_TIP_SEED: u32 = 8;
/// Variacion del tamano de cada toque: se reduce hasta `SIZE_JITTER` (el minimo de photocraft es 0).
const SIZE_JITTER: f32 = 0.2;
const SIZE_SALT: u64 = 0x4953_495A;

/// Ruido fractal 2D entre 0 y 1: varias octavas, cada una al doble de frecuencia y la mitad de fuerza (el `fbm` de
/// photocraft; `cells` es la cantidad de celdas del ruido base a lo ancho de `size`).
fn fbm(x: f32, y: f32, size: f32, cells: i32, octaves: u32, salt: u64) -> f32 {
    let (mut sum, mut amp, mut norm, mut c) = (0.0f32, 1.0f32, 0.0f32, cells);
    for o in 0..octaves {
        sum += value_noise2(x / size * c as f32, y / size * c as f32, c, salt.wrapping_add(u64::from(o) * 7919)) * amp;
        norm += amp;
        amp *= 0.5;
        c *= 2;
    }
    sum / norm
}

/// La punta de la esponja (`sponge_tip` de photocraft): un disco de borde irregular (el borde cae entre 0.78 y 0.98
/// del radio segun el angulo) con poros: el ruido fractal decide donde hay hueco y donde esponja (por debajo de 0.42 no
/// pinta, por encima de 0.59 pinta del todo). El valor de un texel es la cobertura (1 = pinta del todo).
fn construir(seed: u32) -> Punta {
    let salt = u64::from(seed) + 606;
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = (dx * dx + dy * dy).sqrt() / c;
            let edge = 0.78 + 0.2 * value_noise2((dy.atan2(dx) + PI) * 2.5, 0.5, 16, salt);
            let disc = ((edge - r) * 6.0).clamp(0.0, 1.0);
            let pores = fbm(x as f32, y as f32, n as f32, 6, 3, salt);
            data.push(disc * ((pores - 0.42) * 6.0).clamp(0.0, 1.0));
        }
    }
    Punta::new(data)
}

/// La punta de la esponja (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| construir(SPONGE_TIP_SEED))
}

/// Un toque de esponja: la punta gira al azar (cualquier angulo) y mide del 80 al 100 % del radio. No se dispersa ni
/// se voltea.
pub(super) fn nuevo(dab: u32, inv_radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    let (sn, cs) = dab_angle(dab).sin_cos();
    let size = 1.0 - SIZE_JITTER * unit(SIZE_SALT.wrapping_add(u64::from(dab)));
    DryDab {
        kind: DryKind::Sponge,
        t: t0 * cs + b0 * sn,
        b: b0 * cs - t0 * sn,
        inv_radius: inv_radius / size,
        roundness: 1.0,
        shift: Vec3::ZERO,
        follaje: None,
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Bordes mojados (`wet_factor` de photocraft): cuanta pintura se queda en un texel al que la punta mas cercana llega
/// a `rn` (0 en el centro de la punta, 1 en su borde). El interior se queda con la mitad, el borde con todo. `rn` es
/// `1 - profundidad`, con la profundidad el valor mas alto de la punta que paso por el texel.
pub fn wet_factor(rn: f32) -> f32 {
    0.5 + 0.5 * smoothstep(0.5, 1.0, rn)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};
    use crate::pinceles::dry_subtract;

    #[test]
    fn la_punta_es_un_disco_con_poros_y_es_determinista() {
        let a = construir(SPONGE_TIP_SEED);
        assert_eq!(a.data, construir(SPONGE_TIP_SEED).data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        let c = n as f32 / 2.0;
        // Fuera del disco (pasado el 0.98 del radio) no hay nada, ni en las esquinas.
        for i in 0..n * n {
            let (x, y) = ((i % n) as f32 + 0.5 - c, (i / n) as f32 + 0.5 - c);
            if x.hypot(y) / c > 0.99 {
                assert_eq!(a.data[i], 0.0, "texel {i} fuera del disco");
            }
        }
        // Tiene pintura, pero con huecos: dentro del disco (a menos de 0.7 del radio, donde el borde nunca llega) hay
        // texeles que pintan del todo, texeles vacios (los poros) y valores intermedios.
        let dentro: Vec<f32> = (0..n * n)
            .filter(|&i| ((i % n) as f32 + 0.5 - c).hypot((i / n) as f32 + 0.5 - c) / c < 0.7)
            .map(|i| a.data[i])
            .collect();
        let llenos = dentro.iter().filter(|v| **v >= 0.999).count();
        let vacios = dentro.iter().filter(|v| **v <= 0.0).count();
        assert!(llenos > dentro.len() / 10, "pocos texeles llenos: {llenos} de {}", dentro.len());
        assert!(vacios > dentro.len() / 10, "pocos poros: {vacios} de {}", dentro.len());
        assert!(a.data.iter().filter(|v| **v > 0.0).count() > n * n / 8);
    }

    #[test]
    fn el_toque_gira_libre_mide_del_80_al_100_por_ciento_y_no_lleva_papel() {
        let radio = 2.0;
        let (mut tam_min, mut tam_max) = (f32::MAX, 0.0f32);
        let mut cuadrantes = [false; 4];
        for dab in 1..300 {
            let d = DryDab::new(DryKind::Sponge, dab, Vec3::Z, Vec3::X, radio);
            let size = 1.0 / (d.inv_radius * radio);
            assert!((0.8 - 1e-4..=1.0 + 1e-4).contains(&size), "tamano {size}");
            tam_min = tam_min.min(size);
            tam_max = tam_max.max(size);
            // Sin dispersion ni volteo ni aplastamiento.
            assert_eq!(d.shift, Vec3::ZERO);
            assert_eq!(d.roundness, 1.0);
            assert!(d.t.cross(d.b).dot(Vec3::Z) > 0.0, "no deberia voltearse");
            assert_eq!(d.reach(), 0.0);
            // El eje t barre todos los cuadrantes: el giro es libre.
            cuadrantes[(usize::from(d.t.x < 0.0)) * 2 + usize::from(d.t.y < 0.0)] = true;
        }
        assert!(tam_max - tam_min > 0.15, "el tamano casi no varia: de {tam_min} a {tam_max}");
        assert!(cuadrantes.iter().all(|c| *c), "el giro no cubre todas las direcciones");
        assert_eq!(dry_subtract(DryKind::Sponge, Vec3::ONE, 5, 9), 0.0);
        let a = DryDab::new(DryKind::Sponge, 9, Vec3::Z, Vec3::X, 1.0);
        let b = DryDab::new(DryKind::Sponge, 9, Vec3::Z, Vec3::X, 1.0);
        let p = Vec3::new(0.2, -0.1, 0.0);
        assert_eq!(a.alpha(p), b.alpha(p));
        // Como todos los de medio seco que no cambian de color, usa el del pincel.
        assert_eq!(a.alpha_color(p, [10, 20, 30]).1, [10, 20, 30]);
    }

    #[test]
    fn los_bordes_mojados_dejan_la_mitad_en_el_interior_y_todo_en_el_borde() {
        // rn = 1 - profundidad: 0 es el centro de la punta y 1 su borde.
        assert_eq!(wet_factor(0.0), 0.5);
        assert_eq!(wet_factor(0.5), 0.5);
        assert_eq!(wet_factor(1.0), 1.0);
        let mut prev = 0.0;
        for i in 0..=100 {
            let f = wet_factor(i as f32 / 100.0);
            assert!((0.5..=1.0).contains(&f));
            assert!(f >= prev, "no crece");
            prev = f;
        }
    }

    fn cuadrado() -> [SurfTri; 2] {
        let p = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 0.0), Vec3::new(0.0, 2.0, 0.0)];
        let uv = [[0.25, 0.25], [0.5, 0.25], [0.5, 0.75], [0.25, 0.75]];
        [
            SurfTri { p: [p[0], p[1], p[2]], uv: [uv[0], uv[1], uv[2]] },
            SurfTri { p: [p[0], p[2], p[3]], uv: [uv[0], uv[2], uv[3]] },
        ]
    }

    /// Pinta una linea de toques de la esponja (separados 0.6 del radio) y devuelve los pixeles de la capa.
    fn pintar(brush: BrushType, ceiling: bool, flow: f32) -> Vec<u8> {
        let tris = cuadrado();
        let opts = DabOptions { brush, normal: Vec3::Z, ceiling, flow, ..Default::default() };
        let mut s = StrokeState::new(64);
        let mut pixels = vec![0u8; 64 * 64 * 4];
        s.begin_stroke();
        let radio = 0.3;
        for i in 0..14 {
            let c = Vec3::new(0.4 + i as f32 * 0.6 * radio, 1.0, 0.0);
            s.dab_surface(&mut pixels, &tris, c, radio, [30, 60, 160], 1.0, opts);
        }
        pixels
    }

    #[test]
    fn el_trazo_es_mas_claro_por_dentro_y_no_pasa_de_la_mitad_aunque_se_acumulen_toques() {
        for ceiling in [false, true] {
            let pixels = pintar(BrushType::InkSponge, ceiling, 1.0);
            let (mut pintados, mut alfas, mut max) = (0, HashSet::new(), 0u8);
            for q in pixels.chunks(4) {
                if q[3] > 0 {
                    pintados += 1;
                    alfas.insert(q[3]);
                    max = max.max(q[3]);
                    assert_eq!(&q[..3], &[30, 60, 160], "el color no es el del pincel");
                }
            }
            assert!(pintados > 100, "pinto pocos texeles: {pintados}");
            assert!(alfas.len() > 5, "la opacidad es casi pareja: {} valores", alfas.len());
            // La cobertura va de 0 a 1 y los bordes mojados dejan a lo sumo la mitad donde la punta pinta del todo.
            assert!(max <= 131, "ceiling {ceiling}: alfa maximo {max}");
            assert!(max >= 60, "ceiling {ceiling}: casi no pinta, maximo {max}");
        }
    }

    #[test]
    fn con_flujo_bajo_el_trazo_es_mas_tenue_y_el_mismo_trazo_da_lo_mismo() {
        let suma = |p: &[u8]| p.chunks(4).map(|q| u32::from(q[3])).sum::<u32>();
        let (pleno, bajo) = (pintar(BrushType::InkSponge, true, 1.0), pintar(BrushType::InkSponge, true, 0.3));
        assert!(suma(&bajo) < suma(&pleno), "{} no es menos que {}", suma(&bajo), suma(&pleno));
        assert_eq!(pleno, pintar(BrushType::InkSponge, true, 1.0));
    }
}
