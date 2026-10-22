//! Pincel CINCEL PLANO (medio seco): una punta plana en forma de paralelogramo, aplastada y orientada a lo largo del
//! trazo, que deja una banda recta de grosor parejo con los extremos inclinados. Sin papel, sin azar y sin dispersion.
//!
//! Este pincel NO viene de un preset de photocraft: se dedujo de la imagen de un pincel plano del Grease Pencil de
//! Blender (una punta de paralelogramo y su trazo, una banda de bordes casi rectos). Por eso los valores son
//! ESTIMADOS a ojo, no copiados:
//!
//! - Punta: un paralelogramo de 0.72 radios de medio largo (`HALF_LEN`) y 0.9 de medio alto (`HALF_THICK`), con los
//!   lados cortos inclinados (`SKEW` = 0.28: cada fila se corre 0.28 por cada unidad de altura). Bordes suaves de
//!   0.08 (los lados cortos) y 0.15 (los largos) para que no salga con dientes de sierra.
//! - Redondez 0.35 fija: la punta se aplasta a 0.35 de su alto, asi mide unos 1.4 radios de largo por 0.63 de grueso.
//! - Giro: sigue la direccion del trazo (como el carboncillo y el crayon), asi la banda tiene el mismo grosor vaya
//!   hacia donde vaya. En la imagen solo se ve un trazo recto horizontal, por eso no se sabe como reacciona un cincel
//!   de angulo fijo al cambiar de direccion; se eligio el comportamiento predecible. Si no se sabe hacia donde va
//!   (primer toque) usa una base fija de la superficie.
//! - Radio 25 (la punta mide 2 radios de lado, la del paralelogramo cerca de 1.4), espaciado 0.12 del radio (parejo,
//!   sin que se noten los toques) y flujo 1: estan en `brush.rs`.
//!
//! Lo que NO tiene: el degradado leve que se ve en la punta de la imagen y los extremos desiguales del trazo (no se
//! pueden deducir de un solo trazo).

use std::sync::OnceLock;

use glam::Vec3;

use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Medio largo y medio alto del paralelogramo, en radios (la punta llega hasta 1).
const HALF_LEN: f32 = 0.72;
const HALF_THICK: f32 = 0.9;
/// Cuanto se corre cada fila hacia un lado por unidad de altura (inclina los lados cortos).
const SKEW: f32 = 0.28;
/// Suavidad del borde (en unidades de la punta, hacia adentro): lados cortos y largos.
const EDGE_LEN: f32 = 0.08;
const EDGE_THICK: f32 = 0.15;
/// Redondez: el alto de la punta es esta fraccion de su largo.
const ROUNDNESS: f32 = 0.35;

/// La punta: un paralelogramo de borde suave (1 = pinta del todo).
fn construir() -> Punta {
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5 - c) / c, (y as f32 + 0.5 - c) / c);
            let uu = u - SKEW * v;
            let a_len = ((HALF_LEN - uu.abs()) / EDGE_LEN).clamp(0.0, 1.0);
            let a_thick = ((HALF_THICK - v.abs()) / EDGE_THICK).clamp(0.0, 1.0);
            data.push(a_len * a_thick);
        }
    }
    Punta::new(data)
}

/// La punta del cincel (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Un toque de cincel: la punta apunta en la direccion del trazo (`along`, ya proyectada en el plano de la superficie),
/// sin variacion de tamano ni dispersion. Sin direccion (primer toque) usa la base fija `t0`.
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    let t = along.unwrap_or(t0);
    DryDab {
        kind: DryKind::Chisel,
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
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};
    use crate::pinceles::{dry_mezcla_toque, dry_subtract};

    #[test]
    fn la_punta_es_un_paralelogramo_de_borde_suave_y_es_determinista() {
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
        // Simetrica al girarla media vuelta (un paralelogramo).
        for i in 0..n * n {
            assert!(
                (a.data[i] - a.data[n * n - 1 - i]).abs() < 1e-4,
                "texel {i} no es simetrico"
            );
        }
        // Los lados cortos estan inclinados: el centro de lo pintado se corre de una fila de arriba a una de abajo.
        let centro_fila = |y: usize| {
            let (mut s, mut w) = (0.0f32, 0.0f32);
            for x in 0..n {
                let v = a.data[y * n + x];
                s += x as f32 * v;
                w += v;
            }
            s / w
        };
        let (arriba, abajo) = (centro_fila(n / 2 - 12), centro_fila(n / 2 + 12));
        assert!(
            (abajo - arriba).abs() > 5.0,
            "no esta inclinado: {arriba} contra {abajo}"
        );
        // Tiene bordes suaves: valores intermedios, pero la mayoria pinta del todo o nada.
        let medios = a.data.iter().filter(|v| **v > 0.05 && **v < 0.95).count();
        let llenos = a.data.iter().filter(|v| **v >= 0.999).count();
        assert!(medios > 20 && medios < llenos, "medios {medios}, llenos {llenos}");
    }

    #[test]
    fn el_toque_es_una_banda_aplastada_que_sigue_la_direccion_del_trazo() {
        let radio = 2.0;
        // El trazo va por X: la punta es larga a lo largo de X y delgada de lado.
        let d = DryDab::new(DryKind::Chisel, 1, Vec3::Z, Vec3::X, radio);
        assert_eq!(d.roundness, ROUNDNESS);
        assert_eq!(d.shift, Vec3::ZERO);
        assert!(d.follaje.is_none());
        assert_eq!(d.reach(), 0.0);
        assert!(d.alpha(Vec3::new(0.5 * radio, 0.0, 0.0)) > 0.99);
        assert!(d.alpha(Vec3::new(0.0, 0.2 * radio, 0.0)) > 0.99);
        // De lado ya esta fuera (el medio alto es 0.9 * 0.35 = 0.315 radios).
        assert_eq!(d.alpha(Vec3::new(0.0, 0.5 * radio, 0.0)), 0.0);
        // Con el trazo por Y se gira: ahora es delgada en X.
        let e = DryDab::new(DryKind::Chisel, 1, Vec3::Z, Vec3::Y, radio);
        assert!(e.alpha(Vec3::new(0.0, 0.5 * radio, 0.0)) > 0.99);
        assert_eq!(e.alpha(Vec3::new(0.5 * radio, 0.0, 0.0)), 0.0);
        // Sin azar: todos los toques son iguales.
        let p = Vec3::new(0.3, 0.1, 0.0);
        for dab in 1..50 {
            let f = DryDab::new(DryKind::Chisel, dab, Vec3::Z, Vec3::X, radio);
            assert_eq!(f.alpha(p), d.alpha(p));
        }
        // Sin direccion (primer toque) no falla.
        let sin = DryDab::new(DryKind::Chisel, 1, Vec3::Z, Vec3::ZERO, radio);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
        // No lleva papel ni mezcla por toque.
        assert_eq!(dry_subtract(DryKind::Chisel, Vec3::ONE, 5, 9), 0.0);
        assert_eq!(
            dry_mezcla_toque(DryKind::Chisel, 0.5, 7, Vec3::ONE, 3, 4),
            0.5
        );
    }

    /// Cuadrado de 4 x 4 unidades que cubre toda la textura de 256.
    fn cuadrado() -> [SurfTri; 2] {
        let p = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 4.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        ];
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        [
            SurfTri {
                p: [p[0], p[1], p[2]],
                uv: [uv[0], uv[1], uv[2]],
            },
            SurfTri {
                p: [p[0], p[2], p[3]],
                uv: [uv[0], uv[2], uv[3]],
            },
        ]
    }

    const RADIO: f32 = 0.5;

    /// Una linea de toques (`dir` es la direccion del trazo) con el espaciado del pincel; devuelve la capa.
    fn pintar(ceiling: bool, flow: f32, dir: Vec3) -> Vec<u8> {
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Chisel,
            normal: Vec3::Z,
            dir,
            ceiling,
            flow,
            ..Default::default()
        };
        let mut s = StrokeState::new(256);
        let mut pixels = vec![0u8; 256 * 256 * 4];
        s.begin_stroke();
        for i in 0..40 {
            let c = Vec3::new(2.0, 2.0, 0.0) + dir * (-1.2 + i as f32 * 0.12 * RADIO);
            s.dab_surface(&mut pixels, &tris, c, RADIO, [30, 60, 160], 1.0, opts);
        }
        pixels
    }

    /// Lo que mide lo pintado en X y en Y (en unidades del modelo) y los alfas que tiene.
    fn medir(pixels: &[u8]) -> (f32, f32) {
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for i in 0..256u32 * 256 {
            if pixels[i as usize * 4 + 3] > 0 {
                let (x, y) = ((i % 256) as f32 / 255.0 * 4.0, (i / 256) as f32 / 255.0 * 4.0);
                (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
            }
        }
        (x1 - x0, y1 - y0)
    }

    #[test]
    fn el_trazo_es_una_banda_de_grosor_parejo_sin_importar_la_direccion() {
        for ceiling in [false, true] {
            let por_x = pintar(ceiling, 1.0, Vec3::X);
            let por_y = pintar(ceiling, 1.0, Vec3::Y);
            let (largo_x, grueso_x) = medir(&por_x);
            let (grueso_y, largo_y) = medir(&por_y);
            // El grosor es 2 * 0.9 * 0.35 = 0.63 radios.
            let esperado = 0.63 * RADIO;
            assert!(
                (grueso_x - esperado).abs() < 0.05,
                "ceiling {ceiling}: grueso {grueso_x}, esperado {esperado}"
            );
            // Lo mismo con el trazo en Y.
            assert!(
                (grueso_y - grueso_x).abs() < 0.04,
                "ceiling {ceiling}: {grueso_y} contra {grueso_x}"
            );
            assert!(largo_x > 4.0 * grueso_x && largo_y > 4.0 * grueso_y);
            // El color es el del pincel y el interior queda lleno (sin huecos).
            let (mut llenos, mut pintados) = (0, 0);
            for q in por_x.chunks(4) {
                if q[3] > 0 {
                    pintados += 1;
                    assert_eq!(&q[..3], &[30, 60, 160]);
                    if q[3] >= 250 {
                        llenos += 1;
                    }
                }
            }
            assert!(pintados > 1000, "pinto pocos texeles: {pintados}");
            assert!(llenos * 10 > pintados * 7, "interior con huecos: {llenos} de {pintados}");
        }
    }

    #[test]
    fn con_flujo_bajo_es_mas_tenue_y_el_mismo_trazo_da_lo_mismo() {
        let suma = |p: &[u8]| p.chunks(4).map(|q| u32::from(q[3])).sum::<u32>();
        for ceiling in [false, true] {
            let (pleno, bajo) = (
                pintar(ceiling, 1.0, Vec3::X),
                pintar(ceiling, 0.3, Vec3::X),
            );
            assert!(suma(&bajo) < suma(&pleno), "ceiling {ceiling}");
            assert_eq!(pleno, pintar(ceiling, 1.0, Vec3::X));
        }
    }
}
