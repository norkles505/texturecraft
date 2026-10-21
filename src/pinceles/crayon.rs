//! Pincel CRAYON DE CERA (Wax Crayon de photocraft; medio seco): una punta redonda de borde suave, aplastada y orientada
//! a lo largo del trazo, y un papel que se mezcla en modo HardMix con CADA toque (`each_tip`), con una profundidad
//! distinta en cada uno.
//!
//! Lo que copia del preset `Wax Crayon` de photocraft (`presets.rs`, `render.rs` y `dynamics.rs`):
//!
//! - Punta: la punta redonda de photocraft con dureza 0.85 (`tip_shape` de `lib.rs`): un nucleo lleno de 0.865 radios y
//!   un hombro que baja como `exp(-((d - 0.865) / 0.13)^2)`. Llega hasta 0.865 + 2.6 * 0.13 = 1.203 radios del centro
//!   (ahi vale 0.1 %), mas alla del radio del pincel: en el radio vale 0.34.
//! - Redondez 0.7 fija: la punta es una elipse de 1 radio de largo por 0.7 de ancho.
//! - Giro: `Control::Direction`, el eje largo de la elipse sigue la direccion del trazo (sin variacion al azar). Si no
//!   se sabe hacia donde va (primer toque) usa una base fija de la superficie, como el carboncillo.
//! - Papel: el patron `Paper` con semilla 17, en modo HardMix, aplicado a CADA toque (`each_tip`) y no al trazo.
//!   Cada toque lleva su profundidad: `0.9 * (1 - 0.3 * azar)`, o sea de 0.63 a 0.90 (`depth` 0.9 y `depth_jitter` 0.3
//!   del preset; `dynamics.rs`). El modo HardMix es `v + (duro - v) * profundidad`, con `duro` = 1 donde `v + papel > 1`
//!   y 0 donde no. Donde la punta pinta del todo (`v = 1`) solo cambia si el papel vale exactamente 0 (queda
//!   `1 - profundidad`); en el hombro (`v < 1`) rompe el borde.
//! - Radio 11 (22 de diametro), espaciado 0.16 del radio (0.08 del diametro) y flujo 1: estan en `brush.rs`.
//! - Sin tamano, dispersion, volteo ni color por toque: el preset no activa ninguno.
//!
//! Lo que NO copia:
//!
//! - La punta calculada: photocraft la calcula pixel a pixel en cada toque (y con tamanos de pocos pixeles afina su
//!   borde); aqui es un bitmap de 64 x 64 (`TIP_SIZE`) con ese mismo perfil, estirado hasta el alcance de 1.203 radios,
//!   y el hombro usa `exp` en vez de la aproximacion de photocraft (difieren menos de 1/255). La forma coincide; el
//!   borde es un poco menos fino.
//! - El papel exacto: usa el papel compartido de texturecraft (`papel.rs`: ruido fractal 3D de 3 octavas anclado al
//!   modelo, mas un grano por texel), con la sal de la semilla 17, en vez del patron 2D de 128 px y 4 octavas de
//!   photocraft. La formula es la misma (se recorta a 0 donde es oscuro) y la proporcion con el pincel tambien (celda
//!   base de 0.08 con radio 0.11, como 8 px con radio 11), pero cada mancha del papel es distinta.
//! - La regla de acumulacion: con "Acumular pasada" apagado, texturecraft guarda el MAXIMO de los toques en cada texel
//!   (no `c + v * (1 - c)` como photocraft), asi que donde el papel vale 0 el hueco se queda en lo que deja UN toque
//!   (a lo sumo 0.37) y no se rellena con los toques que se solapan. Con "Acumular pasada" encendido es la misma regla
//!   que photocraft y esos huecos se van rellenando.
//! - La presion del lapiz y el color de fondo: el preset no los usa (`pressure_size` apagado en `builtin`), asi que no
//!   se pierde nada.
//!
//! Donde se aplica la mezcla: `paint.rs` (`dab_surface`), con `dry_mezcla_toque` de `medio_seco.rs`, sobre la
//! cobertura de cada toque ANTES de acumularla. La miniatura de `panel_pinceles.rs` hace lo mismo.
//!
//! Alcance: la punta llega a 1.203 radios; `DryDab::reach` suma ese exceso y `BrushType::scatter` devuelve
//! `CRAYON_REACH` (0.203) para que `main.rs` busque los triangulos con ese margen.

use std::sync::OnceLock;

use glam::Vec3;

use super::azar::unit;
use super::medio_seco::{DryDab, DryKind};
use super::papel::{PAPER_SALT, paper_value};
use super::punta::{Punta, TIP_SIZE};

/// Nucleo y hombro de la punta redonda de photocraft con dureza 0.85, en radios (entre las filas 0.8 y 0.9 de
/// `TIP_SHAPE` en `lib.rs`: (0.84, 0.15) y (0.89, 0.11), a la mitad).
const TIP_CORE: f32 = 0.865;
const TIP_SHOULDER: f32 = 0.13;
/// Hasta donde llega la punta, en radios: el nucleo mas 2.6 hombros (donde la opacidad baja de 0.1 %, `tip_reach`).
const TIP_REACH: f32 = TIP_CORE + 2.6 * TIP_SHOULDER;
/// Lo que la punta se sale del circulo del radio, como fraccion del radio del pincel. Es lo que devuelve
/// `BrushType::scatter`.
pub const CRAYON_REACH: f32 = TIP_REACH - 1.0;
/// Redondez del preset: la elipse mide 0.7 de ancho por 1 de largo (el largo va a lo largo del trazo).
const ROUNDNESS: f32 = 0.7;

/// Papel del crayon: semilla 17 (la de `paper(17, 0.9)` en photocraft).
const CRAYON_PAPER_SALT: u64 = PAPER_SALT + 17;
/// Profundidad de la textura (`depth` 0.9) y su variacion por toque (`depth_jitter` 0.3).
const TEXTURE_DEPTH: f32 = 0.9;
const DEPTH_JITTER: f32 = 0.3;
const DEPTH_SALT: u64 = 0x4352_4450;

/// Cobertura de la punta a `d` radios del centro (el perfil de photocraft).
fn perfil(d: f32) -> f32 {
    if d <= TIP_CORE {
        return 1.0;
    }
    let x = (d - TIP_CORE) / TIP_SHOULDER;
    (-x * x).exp()
}

/// La punta (un disco suave que llega hasta `TIP_REACH` radios; el borde del bitmap es ese alcance). El aplastado
/// (redondez 0.7) lo pone el toque, no el bitmap.
fn construir() -> Punta {
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            // 1 = el alcance de la punta.
            let rn = (dx * dx + dy * dy).sqrt() / c;
            data.push(if rn >= 1.0 { 0.0 } else { perfil(rn * TIP_REACH) });
        }
    }
    Punta::new(data)
}

/// La punta del crayon (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Un toque de crayon: la elipse apunta en la direccion del trazo (`along`, ya proyectada en el plano de la
/// superficie), sin variacion de tamano ni dispersion. Si no se sabe hacia donde va (primer toque) usa la base fija
/// `t0`. La punta mide `TIP_REACH` radios (el bitmap llega hasta el alcance), asi que el radio de la punta es el del
/// pincel por ese factor.
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    let t = along.unwrap_or(t0);
    DryDab {
        kind: DryKind::Crayon,
        t,
        b: n.cross(t),
        inv_radius: inv_radius / TIP_REACH,
        roundness: ROUNDNESS,
        shift: Vec3::ZERO,
        follaje: None,
    }
}

/// Cuanto se sale del radio del pincel la punta de un toque con esa `inv_radius` (la ya dividida por `TIP_REACH`):
/// `CRAYON_REACH` radios. Es lo que `DryDab::reach` suma para este pincel.
pub(super) fn exceso(inv_radius: f32) -> f32 {
    CRAYON_REACH / TIP_REACH / inv_radius
}

/// Profundidad de la textura en el toque numero `dab`: `0.9 * (1 - 0.3 * azar)`, de 0.63 a 0.90 (en photocraft,
/// `depth * scalar(depth_jitter)` de `dynamics.rs`, con minimo 0).
pub(super) fn profundidad(dab: u32) -> f32 {
    TEXTURE_DEPTH * (1.0 - DEPTH_JITTER * unit(DEPTH_SALT.wrapping_add(u64::from(dab))))
}

/// Modo HardMix de photocraft (`mask_combine`) con la profundidad `d` del toque: mezcla la cobertura `v` del toque con
/// lo que daria el "todo o nada" (`duro`: 1 donde `v + t > 1`, 0 donde no). Con `v = 0` nunca pinta (una mascara no
/// agrega pintura donde el pincel no tiene). `t` es el valor del papel.
pub(super) fn hard_mix_toque(v: f32, t: f32, d: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let duro = if v + t > 1.0 { 1.0 } else { 0.0 };
    (v + (duro - v) * d).clamp(0.0, 1.0)
}

/// La cobertura `v` del toque `dab` en el texel `(tx, ty)` (en la posicion `pos` del modelo) ya mezclada con su papel.
pub(super) fn mezcla(v: f32, dab: u32, pos: Vec3, tx: u32, ty: u32) -> f32 {
    hard_mix_toque(v, paper_value(pos, tx, ty, CRAYON_PAPER_SALT), profundidad(dab))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};
    use crate::pinceles::{dry_hardmix, dry_mezcla_toque, dry_subtract};

    #[test]
    fn la_punta_es_un_disco_suave_que_llega_mas_alla_del_radio_y_es_determinista() {
        let a = construir();
        assert_eq!(a.data, construir().data);
        assert_eq!(a.data, punta().data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        let c = n as f32 / 2.0;
        for i in 0..n * n {
            let (x, y) = ((i % n) as f32 + 0.5 - c, (i / n) as f32 + 0.5 - c);
            let rn = x.hypot(y) / c;
            // Vacia en las esquinas y pasado el alcance.
            if rn >= 1.0 {
                assert_eq!(a.data[i], 0.0, "texel {i} fuera del alcance");
            }
            // El nucleo (0.865 radios) pinta del todo.
            if rn * TIP_REACH <= TIP_CORE {
                assert_eq!(a.data[i], 1.0, "texel {i} del nucleo");
            }
        }
        // Baja hacia afuera: el nucleo vale 1 en los dos primeros anillos (no sube) y despues cada anillo es menor.
        let anillo = |lo: f32, hi: f32| {
            let v: Vec<f32> = (0..n * n)
                .filter(|&i| {
                    let rn = ((i % n) as f32 + 0.5 - c).hypot((i / n) as f32 + 0.5 - c) / c;
                    (lo..hi).contains(&rn)
                })
                .map(|i| a.data[i])
                .collect();
            v.iter().sum::<f32>() / v.len() as f32
        };
        let medias = [anillo(0.0, 0.3), anillo(0.6, 0.7), anillo(0.8, 0.9), anillo(0.95, 1.0)];
        assert!(medias.windows(2).all(|w| w[0] >= w[1]), "no baja hacia afuera: {medias:?}");
        assert!(medias[1] > medias[2] && medias[2] > medias[3], "el hombro no baja: {medias:?}");
        // En el radio del pincel (1 / TIP_REACH del alcance) vale `exp(-(0.135 / 0.13)^2)` = 0.34.
        let en_radio = a.sample(0.5 + 0.5 / TIP_REACH, 0.5);
        assert!((en_radio - 0.34).abs() < 0.03, "en el radio vale {en_radio}");
    }

    #[test]
    fn la_profundidad_de_cada_toque_va_de_063_a_09_y_es_determinista() {
        let (mut lo, mut hi) = (f32::MAX, 0.0f32);
        let mut distintas = HashSet::new();
        for dab in 1..500 {
            let d = profundidad(dab);
            assert!((0.63 - 1e-4..=0.9 + 1e-4).contains(&d), "profundidad {d}");
            assert_eq!(d, profundidad(dab));
            lo = lo.min(d);
            hi = hi.max(d);
            distintas.insert((d * 10_000.0) as i32);
        }
        assert!(lo < 0.66 && hi > 0.87, "casi no varia: de {lo} a {hi}");
        assert!(distintas.len() > 300, "los toques se repiten: {} valores", distintas.len());
    }

    #[test]
    fn hard_mix_toque_mezcla_con_la_profundidad_del_toque() {
        // Sin cobertura no pinta nunca.
        assert_eq!(hard_mix_toque(0.0, 1.0, 0.9), 0.0);
        // Profundidad 0: no cambia nada.
        for v in [0.2, 0.5, 1.0] {
            assert!((hard_mix_toque(v, 0.3, 0.0) - v).abs() < 1e-6);
        }
        // Donde la punta pinta del todo solo cambia si el papel vale 0: queda `1 - profundidad`.
        assert_eq!(hard_mix_toque(1.0, 0.5, 0.9), 1.0);
        assert!((hard_mix_toque(1.0, 0.0, 0.9) - 0.1).abs() < 1e-6);
        // En el hombro: con papel claro sube hacia 1 y con papel oscuro baja hacia 0.
        assert!((hard_mix_toque(0.5, 0.6, 0.9) - 0.95).abs() < 1e-6);
        assert!((hard_mix_toque(0.5, 0.4, 0.9) - 0.05).abs() < 1e-6);
        // Siempre entre 0 y 1.
        for i in 0..=20 {
            for j in 0..=20 {
                let r = hard_mix_toque(i as f32 / 20.0, j as f32 / 20.0, 0.63);
                assert!((0.0..=1.0).contains(&r), "{r}");
            }
        }
    }

    #[test]
    fn el_toque_es_una_elipse_que_sigue_la_direccion_del_trazo_y_llega_mas_alla_del_radio() {
        let radio = 2.0;
        // El trazo va por Y: la punta es larga a lo largo de Y y corta de lado.
        let d = DryDab::new(DryKind::Crayon, 1, Vec3::Z, Vec3::Y, radio);
        assert_eq!(d.roundness, 0.7);
        assert_eq!(d.shift, Vec3::ZERO);
        assert!(d.follaje.is_none());
        assert!((d.reach() - CRAYON_REACH * radio).abs() < 1e-4, "alcance {}", d.reach());
        // Sin azar de forma: todos los toques son iguales (la profundidad del papel es otra cosa).
        let p = Vec3::new(0.3, 0.9, 0.0);
        for dab in 1..50 {
            let e = DryDab::new(DryKind::Crayon, dab, Vec3::Z, Vec3::Y, radio);
            assert_eq!(e.alpha(p), d.alpha(p));
        }
        // A 0.75 radios: a lo largo del trazo es nucleo (pinta del todo) y de lado ya esta en el hombro (0.7 de ancho).
        let a_lo_largo = d.alpha(Vec3::new(0.0, 0.75 * radio, 0.0));
        let de_lado = d.alpha(Vec3::new(0.75 * radio, 0.0, 0.0));
        assert!(a_lo_largo > 0.9, "a lo largo vale {a_lo_largo}");
        assert!(de_lado < 0.5, "de lado vale {de_lado}");
        // Centro: pinta del todo. Pasado el alcance a lo largo del trazo, nada.
        assert!(d.alpha(Vec3::ZERO) > 0.999);
        assert_eq!(d.alpha(Vec3::new(0.0, radio * TIP_REACH * 1.01, 0.0)), 0.0);
        // Sin direccion (primer toque) usa una base fija y no falla.
        let sin = DryDab::new(DryKind::Crayon, 1, Vec3::Z, Vec3::ZERO, radio);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
    }

    #[test]
    fn solo_el_crayon_mezcla_el_papel_con_cada_toque() {
        let pos = Vec3::new(0.3, 0.2, 0.0);
        for k in [
            DryKind::Charcoal,
            DryKind::Pastel,
            DryKind::Stipple,
            DryKind::Foliage,
            DryKind::Sponge,
            DryKind::Spatter,
            DryKind::Grass,
            DryKind::Halftone,
        ] {
            assert_eq!(dry_mezcla_toque(k, 0.5, 7, pos, 3, 4), 0.5, "{k:?}");
        }
        // El crayon la cambia (con v = 0.5 nunca queda igual: sube o baja) y la deja en 0..1.
        let cambia = (0..200u32)
            .map(|i| dry_mezcla_toque(DryKind::Crayon, 0.5, 7, Vec3::new(i as f32 * 0.013, 0.3, i as f32 * 0.007), i, 5))
            .inspect(|r| assert!((0.0..=1.0).contains(r), "{r}"))
            .filter(|r| *r != 0.5)
            .count();
        assert_eq!(cambia, 200);
        // No resta papel al trazo ni lleva la trama de los puntos de semitono.
        assert_eq!(dry_subtract(DryKind::Crayon, Vec3::ONE, 5, 9), 0.0);
        assert_eq!(dry_hardmix(DryKind::Crayon, 3, 3), None);
    }

    /// Cuadrado de 4 x 4 unidades que cubre toda la textura de 256 (64 texeles por unidad).
    fn cuadrado() -> [SurfTri; 2] {
        let p = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0), Vec3::new(4.0, 4.0, 0.0), Vec3::new(0.0, 4.0, 0.0)];
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        [
            SurfTri { p: [p[0], p[1], p[2]], uv: [uv[0], uv[1], uv[2]] },
            SurfTri { p: [p[0], p[2], p[3]], uv: [uv[0], uv[2], uv[3]] },
        ]
    }

    const BASE: [u8; 3] = [30, 60, 160];
    const CENTRO: Vec3 = Vec3::new(2.0, 2.0, 0.0);
    const RADIO: f32 = 0.5;

    /// Pinta toques en los `centros` (el trazo va en la direccion `dir`) y devuelve los pixeles de la capa.
    fn pintar(ceiling: bool, flow: f32, dir: Vec3, centros: &[Vec3]) -> Vec<u8> {
        let tris = cuadrado();
        let opts = DabOptions { brush: BrushType::WaxCrayon, normal: Vec3::Z, dir, ceiling, flow, ..Default::default() };
        let mut s = StrokeState::new(256);
        let mut pixels = vec![0u8; 256 * 256 * 4];
        s.begin_stroke();
        for c in centros {
            s.dab_surface(&mut pixels, &tris, *c, RADIO, BASE, 1.0, opts);
        }
        pixels
    }

    /// Recorre los texeles de la capa con su posicion en el modelo.
    fn texeles(pixels: &[u8]) -> impl Iterator<Item = (Vec3, &[u8])> {
        (0..256u32 * 256).map(move |i| {
            let (tx, ty) = (i % 256, i / 256);
            (Vec3::new(tx as f32 / 255.0 * 4.0, ty as f32 / 255.0 * 4.0, 0.0), &pixels[i as usize * 4..i as usize * 4 + 4])
        })
    }

    /// Lo que mide lo pintado en X y en Y, en unidades del modelo.
    fn extension(pixels: &[u8]) -> (f32, f32) {
        let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for (p, q) in texeles(pixels) {
            if q[3] > 0 {
                (x0, x1, y0, y1) = (x0.min(p.x), x1.max(p.x), y0.min(p.y), y1.max(p.y));
            }
        }
        (x1 - x0, y1 - y0)
    }

    #[test]
    fn un_toque_pinta_con_el_color_del_pincel_con_variacion_y_sin_pasar_del_alcance() {
        let alcance = RADIO * (1.0 + BrushType::WaxCrayon.scatter());
        for ceiling in [false, true] {
            let pixels = pintar(ceiling, 1.0, Vec3::X, &[CENTRO]);
            let (mut pintados, mut alfas) = (0, HashSet::new());
            for (p, q) in texeles(&pixels) {
                if q[3] > 0 {
                    pintados += 1;
                    alfas.insert(q[3]);
                    assert_eq!(&q[..3], &BASE, "ceiling {ceiling}: el color no es el del pincel");
                    let d = p.distance(CENTRO);
                    assert!(d < alcance + 0.02, "ceiling {ceiling}: pinto a {d}, mas alla del alcance {alcance}");
                }
            }
            assert!(pintados > 1000, "ceiling {ceiling}: pinto pocos texeles: {pintados}");
            assert!(alfas.len() > 8, "ceiling {ceiling}: la opacidad es casi pareja: {} valores", alfas.len());
        }
    }

    #[test]
    fn la_punta_se_alarga_a_lo_largo_del_trazo() {
        let (ax, ay) = extension(&pintar(false, 1.0, Vec3::X, &[CENTRO]));
        assert!(ax > ay * 1.25, "con el trazo en X mide {ax} de ancho y {ay} de alto");
        let (bx, by) = extension(&pintar(false, 1.0, Vec3::Y, &[CENTRO]));
        assert!(by > bx * 1.25, "con el trazo en Y mide {bx} de ancho y {by} de alto");
    }

    #[test]
    fn el_papel_deja_huecos_en_el_interior_y_el_techo_los_rellena() {
        // Una linea de toques separados 0.16 del radio (el espaciado del pincel), de izquierda a derecha.
        let linea: Vec<Vec3> = (0..40).map(|i| Vec3::new(0.8 + i as f32 * 0.16 * RADIO, 2.0, 0.0)).collect();
        // El interior del trazo: donde cubren todos los toques que pueden llegar y la punta pinta del todo.
        let interior = |p: &[u8]| -> Vec<u8> {
            texeles(p).filter(|(p, _)| (1.6..3.1).contains(&p.x) && (p.y - 2.0).abs() < 0.1).map(|(_, q)| q[3]).collect()
        };
        let sin_techo = interior(&pintar(false, 1.0, Vec3::X, &linea));
        let con_techo = interior(&pintar(true, 1.0, Vec3::X, &linea));
        let huecos = |v: &[u8]| v.iter().filter(|a| **a < 100).count();
        // Sin techo (el maximo de los toques): donde el papel vale 0 un toque deja `1 - profundidad`, a lo sumo 0.37
        // (94 de 255); en el resto, la punta llena pinta del todo. No hay valores intermedios.
        assert!(sin_techo.iter().all(|a| *a <= 95 || *a >= 250), "valores intermedios: {:?}", sin_techo.iter().filter(|a| **a > 95 && **a < 250).collect::<Vec<_>>());
        assert!(huecos(&sin_techo) > 20, "casi no hay huecos: {} de {}", huecos(&sin_techo), sin_techo.len());
        // Con techo (`c + v * (1 - c)`, como photocraft) los toques que se solapan rellenan esos huecos.
        assert!(huecos(&con_techo) * 4 < huecos(&sin_techo), "el techo no rellena: {} contra {}", huecos(&con_techo), huecos(&sin_techo));
    }

    #[test]
    fn con_flujo_bajo_el_trazo_es_mas_tenue_y_el_mismo_trazo_da_lo_mismo() {
        let linea: Vec<Vec3> = (0..20).map(|i| Vec3::new(1.0 + i as f32 * 0.16 * RADIO, 2.0, 0.0)).collect();
        let suma = |p: &[u8]| p.chunks(4).map(|q| u32::from(q[3])).sum::<u32>();
        for ceiling in [false, true] {
            let (pleno, bajo) = (pintar(ceiling, 1.0, Vec3::X, &linea), pintar(ceiling, 0.3, Vec3::X, &linea));
            assert!(suma(&bajo) < suma(&pleno), "ceiling {ceiling}: {} no es menos que {}", suma(&bajo), suma(&pleno));
            assert_eq!(pleno, pintar(ceiling, 1.0, Vec3::X, &linea), "ceiling {ceiling}: no es determinista");
        }
    }
}
