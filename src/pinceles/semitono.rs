//! Pincel PUNTOS DE SEMITONO (Halftone Dots de photocraft; medio seco): un disco suave (dureza 0.5) que NO pinta lleno:
//! la cobertura acumulada del trazo se mezcla con una trama de puntos en modo HardMix, asi el resultado es siempre
//! pintado o no pintado, con agujeros redondos que crecen hacia el borde del trazo (donde la cobertura baja).
//!
//! Lo que copia del preset `Halftone Dots` de photocraft (`presets.rs`):
//!
//! - Punta: la punta redonda de photocraft con dureza 0.5 (`tip_shape` de `lib.rs`): un nucleo lleno de 0.63 radios y un
//!   hombro que baja como `exp(-((d - 0.63) / 0.29)^2)`. Llega hasta 0.63 + 2.6 * 0.29 = 1.384 radios del centro (ahi
//!   vale 0.1 %), mas alla del radio del pincel: en el radio vale 0.196.
//! - Textura: el patron `Dots` de `procedural.rs` (puntos de 8 texeles de periodo; cada punto mide 0.35 de la celda de
//!   radio, con un borde suave de 0.1), en modo HardMix con profundidad 1, aplicada una vez al TRAZO (`each_tip`
//!   apagado) sobre la cobertura ya acumulada: donde `cobertura + patron > 1` se pinta del todo y donde no, nada.
//!   Con la cobertura en 1 (el centro) solo quedan sin pintar los puntos (el 28 % de cada celda); hacia el borde la
//!   cobertura baja y los agujeros crecen hasta el 50 % de la celda (el patron nunca llega a 1 antes de los 0.4 de
//!   celda).
//! - Sin giro, tamano, volteo, dispersion, color ni opacidad por toque: el preset no activa ninguno.
//! - Radio 40 (80 de diametro), espaciado 0.5 del radio (0.25 del diametro) y flujo 1: estan en `brush.rs`.
//!
//! Lo que NO copia:
//!
//! - La punta de computada: photocraft la calcula pixel a pixel en cada toque (y con tamanos de pocos pixeles afina su
//!   borde); aqui es un bitmap de 64 x 64 (`TIP_SIZE`) con ese mismo perfil, estirado hasta el alcance de 1.384 radios.
//!   La forma coincide; el borde es un poco menos fino.
//! - El tamano de los puntos en pantalla: en photocraft la trama mide 8 pixeles del lienzo. Aqui mide 8 TEXELES de la
//!   textura (como el lienzo del pastel: anclada al UV, no a la pantalla), asi que sobre el modelo el tamano de los
//!   puntos depende de cuanto estire el UV de esa pieza.
//! - La regla de acumulacion: con "Acumular pasada" apagado, texturecraft guarda el MAXIMO de los toques en cada texel
//!   (no `c + v * (1 - c)` como photocraft), asi que el trazo llega a la cobertura de un solo toque y los agujeros
//!   quedan un poco mas grandes. Con "Acumular pasada" encendido es la misma regla que photocraft.
//! - La presion del lapiz: el preset no la usa (`pressure_size` apagado en `builtin`), asi que no se pierde nada.
//!
//! Donde se aplica la mezcla: `paint.rs` (`apply_texel`, parametro `hard`), con `dry_hardmix` de `medio_seco.rs`. La
//! miniatura de `panel_pinceles.rs` hace lo mismo al final del trazo.
//!
//! Alcance: la punta llega a 1.384 radios; `DryDab::reach` suma ese exceso y `BrushType::scatter` devuelve
//! `HALFTONE_REACH` (0.384) para que `main.rs` busque los triangulos con ese margen.

use std::sync::OnceLock;

use glam::Vec3;

use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Nucleo y hombro de la punta redonda de photocraft con dureza 0.5, en radios (fila 0.5 de `TIP_SHAPE` en `lib.rs`).
const TIP_CORE: f32 = 0.63;
const TIP_SHOULDER: f32 = 0.29;
/// Hasta donde llega la punta, en radios: el nucleo mas 2.6 hombros (donde la opacidad baja de 0.1 %, `tip_reach`).
const TIP_REACH: f32 = TIP_CORE + 2.6 * TIP_SHOULDER;
/// Lo que la punta se sale del circulo del radio, como fraccion del radio del pincel. Es lo que devuelve
/// `BrushType::scatter`.
pub const HALFTONE_REACH: f32 = TIP_REACH - 1.0;

/// La trama de puntos de photocraft (`PatternStyle::Dots`): periodo en texeles, radio de cada punto y suavidad de su
/// borde, los dos ultimos como fraccion de la celda.
const DOT_PERIOD: f32 = 8.0;
const DOT_RADIUS: f32 = 0.35;
const DOT_SOFT: f32 = 0.1;

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Cobertura de la punta a `d` radios del centro (el perfil de photocraft).
fn perfil(d: f32) -> f32 {
    if d <= TIP_CORE {
        return 1.0;
    }
    let x = (d - TIP_CORE) / TIP_SHOULDER;
    (-x * x).exp()
}

/// La punta (un disco suave que llega hasta `TIP_REACH` radios; el borde del bitmap es ese alcance).
fn construir() -> Punta {
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            // 1 = el alcance de la punta.
            let rn = (dx * dx + dy * dy).sqrt() / c;
            data.push(if rn >= 1.0 {
                0.0
            } else {
                perfil(rn * TIP_REACH)
            });
        }
    }
    Punta::new(data)
}

/// La punta de los puntos de semitono (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Un toque: un disco redondo, sin giro, tamano ni dispersion al azar. La punta mide `TIP_REACH` radios (el bitmap
/// llega hasta el alcance), asi que el radio de la punta es el del pincel por ese factor.
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    DryDab {
        kind: DryKind::Halftone,
        t: t0,
        b: b0,
        inv_radius: inv_radius / TIP_REACH,
        roundness: 1.0,
        shift: Vec3::ZERO,
        follaje: None,
    }
}

/// Cuanto se sale del radio del pincel la punta de un toque con esa `inv_radius` (la ya dividida por `TIP_REACH`):
/// `HALFTONE_REACH` radios. Es lo que `DryDab::reach` suma para este pincel.
pub(super) fn exceso(inv_radius: f32) -> f32 {
    HALFTONE_REACH / TIP_REACH / inv_radius
}

/// Valor de la trama de puntos (0..1) en la posicion `(fx, fy)` de la textura, en texeles y medida al CENTRO del texel
/// (el texel `(3, 4)` esta en `(3.5, 4.5)`). 0 en el centro de cada punto y 1 fuera de el: es `1 - suavizado` del
/// borde de un circulo de `DOT_RADIUS` celdas. Se repite cada `DOT_PERIOD` texeles.
pub fn dots_at(fx: f32, fy: f32) -> f32 {
    let (u, v) = (
        (fx / DOT_PERIOD).fract() - 0.5,
        (fy / DOT_PERIOD).fract() - 0.5,
    );
    let d = (u * u + v * v).sqrt();
    1.0 - smooth(((DOT_RADIUS - d) / DOT_SOFT + 0.5).clamp(0.0, 1.0))
}

/// Valor de la trama en el texel `(tx, ty)`.
pub(super) fn patron(tx: u32, ty: u32) -> f32 {
    dots_at(tx as f32 + 0.5, ty as f32 + 0.5)
}

/// Modo HardMix de photocraft con profundidad 1: pinta del todo (1) donde la cobertura `v` mas el patron `t` pasan de
/// 1, y nada (0) donde no. Con `v = 0` nunca pinta (una mascara no agrega pintura donde el pincel no tiene).
pub fn hard_mix(v: f32, t: f32) -> f32 {
    if v + t > 1.0 { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};
    use crate::pinceles::{dry_hardmix, dry_subtract};

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
            // El nucleo (0.63 radios) pinta del todo.
            if rn * TIP_REACH <= TIP_CORE {
                assert_eq!(a.data[i], 1.0, "texel {i} del nucleo");
            }
        }
        // Baja hacia afuera: el promedio de cada anillo es menor que el del anterior.
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
        let medias = [
            anillo(0.0, 0.3),
            anillo(0.5, 0.6),
            anillo(0.7, 0.8),
            anillo(0.9, 1.0),
        ];
        assert!(
            medias.windows(2).all(|w| w[0] > w[1]),
            "no baja hacia afuera: {medias:?}"
        );
        // En el radio del pincel (1 / TIP_REACH del alcance) vale `exp(-(0.37 / 0.29)^2)` = 0.196.
        let en_radio = a.sample(0.5 + 0.5 / TIP_REACH, 0.5);
        assert!(
            (en_radio - 0.196).abs() < 0.03,
            "en el radio vale {en_radio}"
        );
    }

    #[test]
    fn la_trama_son_puntos_de_8_texeles_en_rango_y_se_repite() {
        let (mut ceros, mut unos, mut medios) = (0, 0, 0);
        for ty in 0..64u32 {
            for tx in 0..64u32 {
                let v = patron(tx, ty);
                assert!((0.0..=1.0).contains(&v), "{v}");
                assert_eq!(v, patron(tx + 8, ty));
                assert_eq!(v, patron(tx, ty + 64));
                match v {
                    v if v == 0.0 => ceros += 1,
                    v if v == 1.0 => unos += 1,
                    _ => medios += 1,
                }
            }
        }
        // Cada celda de 8 x 8 tiene 16 texeles dentro del punto (el bloque de 4 x 4 del centro) y 32 que son pura
        // pintura; el resto es el borde suave.
        assert_eq!(ceros, 16 * 64);
        assert_eq!(unos, 32 * 64);
        assert_eq!(medios, 16 * 64);
        // El centro de un punto vale 0 y la esquina de la celda, 1.
        assert_eq!(patron(3, 3), 0.0);
        assert_eq!(patron(4, 4), 0.0);
        assert_eq!(patron(0, 0), 1.0);
        // A 0.35 celdas (2.8 texeles) del centro vale la mitad, como en photocraft.
        assert!((dots_at(4.0 + 2.8, 4.0) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn hard_mix_pinta_del_todo_o_nada() {
        // Donde el patron es 1 pinta con cualquier cobertura; donde es 0 no pinta ni con la cobertura entera.
        assert_eq!(hard_mix(0.01, 1.0), 1.0);
        assert_eq!(hard_mix(1.0, 0.0), 0.0);
        // En medio, pinta cuando la suma pasa de 1.
        assert_eq!(hard_mix(0.5, 0.6), 1.0);
        assert_eq!(hard_mix(0.5, 0.4), 0.0);
        // Sin cobertura no pinta nunca.
        assert_eq!(hard_mix(0.0, 1.0), 0.0);
        // Solo da 0 o 1 y sube con la cobertura.
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            let mut prev = 0.0;
            for j in 0..=20 {
                let r = hard_mix(j as f32 / 20.0, t);
                assert!(r == 0.0 || r == 1.0);
                assert!(r >= prev, "baja al subir la cobertura");
                prev = r;
            }
        }
    }

    #[test]
    fn el_toque_es_un_disco_redondo_sin_azar_que_llega_mas_alla_del_radio() {
        let radio = 2.0;
        let p = Vec3::new(0.7, 0.2, 0.0);
        let uno = DryDab::new(DryKind::Halftone, 1, Vec3::Z, Vec3::X, radio);
        for dab in 1..50 {
            let d = DryDab::new(DryKind::Halftone, dab, Vec3::Z, Vec3::X, radio);
            // Sin dispersion ni aplastamiento.
            assert_eq!(d.shift, Vec3::ZERO);
            assert_eq!(d.roundness, 1.0);
            assert!(d.follaje.is_none());
            // Llega HALFTONE_REACH radios mas alla del radio.
            assert!(
                (d.reach() - HALFTONE_REACH * radio).abs() < 1e-4,
                "alcance {}",
                d.reach()
            );
            // Todos los toques son iguales (no hay azar).
            assert_eq!(d.alpha(p), uno.alpha(p));
        }
        // Centro y nucleo: pinta del todo.
        assert!(uno.alpha(Vec3::ZERO) > 0.999);
        assert!(uno.alpha(Vec3::new(0.5 * radio, 0.0, 0.0)) > 0.999);
        // En el radio vale 0.196 y pasado el alcance, nada.
        assert!((uno.alpha(Vec3::new(radio, 0.0, 0.0)) - 0.196).abs() < 0.03);
        assert_eq!(
            uno.alpha(Vec3::new(radio * TIP_REACH * 1.01, 0.0, 0.0)),
            0.0
        );
        // Es redondo: lo mismo en cualquier direccion.
        let (a, b) = (
            uno.alpha(Vec3::new(0.9 * radio, 0.0, 0.0)),
            uno.alpha(Vec3::new(0.0, 0.9 * radio, 0.0)),
        );
        assert!((a - b).abs() < 0.03, "{a} y {b}");
        // No resta papel; solo este pincel tiene mezcla HardMix.
        assert_eq!(dry_subtract(DryKind::Halftone, Vec3::ONE, 5, 9), 0.0);
        assert_eq!(dry_hardmix(DryKind::Halftone, 3, 3), Some(0.0));
        assert_eq!(dry_hardmix(DryKind::Halftone, 0, 0), Some(1.0));
        for k in [
            DryKind::Charcoal,
            DryKind::Pastel,
            DryKind::Stipple,
            DryKind::Foliage,
            DryKind::Sponge,
            DryKind::Spatter,
            DryKind::Grass,
        ] {
            assert_eq!(dry_hardmix(k, 3, 3), None, "{k:?}");
        }
    }

    /// Cuadrado de 4 x 4 unidades que cubre toda la textura de 256 (64 texeles por unidad: un periodo de la trama mide
    /// 0.125 unidades).
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

    const BASE: [u8; 3] = [30, 60, 160];
    const CENTRO: Vec3 = Vec3::new(2.0, 2.0, 0.0);
    const RADIO: f32 = 0.5;

    /// Pinta toques en los `centros` y devuelve los pixeles de la capa.
    fn pintar(ceiling: bool, flow: f32, centros: &[Vec3]) -> Vec<u8> {
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Halftone,
            normal: Vec3::Z,
            dir: Vec3::X,
            ceiling,
            flow,
            ..Default::default()
        };
        let mut s = StrokeState::new(256);
        let mut pixels = vec![0u8; 256 * 256 * 4];
        s.begin_stroke();
        for c in centros {
            s.dab_surface(&mut pixels, &tris, *c, RADIO, BASE, 1.0, opts);
        }
        pixels
    }

    /// Recorre los texeles de la capa con su distancia al `centro` (en el modelo) y si estan pintados.
    fn texeles(pixels: &[u8], centro: Vec3) -> impl Iterator<Item = (f32, &[u8])> {
        (0..256u32 * 256).map(move |i| {
            let (tx, ty) = (i % 256, i / 256);
            let pos = Vec3::new(tx as f32 / 255.0 * 4.0, ty as f32 / 255.0 * 4.0, 0.0);
            (
                pos.distance(centro),
                &pixels[i as usize * 4..i as usize * 4 + 4],
            )
        })
    }

    #[test]
    fn un_toque_pinta_agujeros_que_crecen_hacia_el_borde_sin_pasar_del_alcance() {
        let alcance = RADIO * (1.0 + BrushType::Halftone.scatter());
        for ceiling in [false, true] {
            let pixels = pintar(ceiling, 1.0, &[CENTRO]);
            let (mut nucleo, mut anillo) = ((0u32, 0u32), (0u32, 0u32));
            for (d, q) in texeles(&pixels, CENTRO) {
                let pintado = q[3] > 0;
                if pintado {
                    // Pinta del todo o nada, y siempre con el color del pincel.
                    assert_eq!(q[3], 255, "ceiling {ceiling}: alfa {}", q[3]);
                    assert_eq!(
                        &q[..3],
                        &BASE,
                        "ceiling {ceiling}: el color no es el del pincel"
                    );
                    assert!(
                        d < alcance + 0.02,
                        "ceiling {ceiling}: pinto a {d}, mas alla del alcance {alcance}"
                    );
                }
                if d < 0.4 * RADIO {
                    nucleo = (nucleo.0 + 1, nucleo.1 + u32::from(pintado));
                } else if (RADIO..1.3 * RADIO).contains(&d) {
                    anillo = (anillo.0 + 1, anillo.1 + u32::from(pintado));
                }
            }
            let (fn_, fa) = (
                nucleo.1 as f32 / nucleo.0 as f32,
                anillo.1 as f32 / anillo.0 as f32,
            );
            // En el centro la cobertura es 1: solo quedan sin pintar los puntos (alrededor del 25 %).
            assert!(
                (0.68..0.82).contains(&fn_),
                "ceiling {ceiling}: en el nucleo se pinta el {fn_}"
            );
            // Hacia el borde la cobertura baja y los agujeros crecen: se pinta menos.
            assert!(
                (0.40..0.62).contains(&fa),
                "ceiling {ceiling}: en el anillo se pinta el {fa}"
            );
            assert!(
                fn_ > fa + 0.1,
                "ceiling {ceiling}: los agujeros no crecen hacia el borde ({fn_} y {fa})"
            );
        }
    }

    #[test]
    fn con_flujo_bajo_y_techo_cada_toque_sube_la_cobertura_y_se_repinta() {
        // Con flujo 0.4, un toque deja la cobertura en 0.4 y seis en 0.95: donde la cobertura sube y pasa el umbral del
        // patron, el texel se pinta (la regla de `apply_texel` solo repinta si la cobertura acumulada sube).
        let contar = |p: &[u8]| {
            texeles(p, CENTRO)
                .filter(|(d, q)| *d < 0.4 * RADIO && q[3] > 0)
                .count()
        };
        let (uno, seis) = (
            pintar(true, 0.4, &[CENTRO]),
            pintar(true, 0.4, &[CENTRO; 6]),
        );
        let (menos, mas) = (contar(&uno), contar(&seis));
        assert!(menos > 0, "con un toque no pinta nada");
        assert!(
            mas > menos + menos / 20,
            "mas toques no pintan mas: {mas} contra {menos}"
        );
        // Con flujo 1 un toque ya deja el nucleo con la cobertura entera: repetirlo no cambia lo pintado ahi (en el
        // hombro si sube, `c + v * (1 - c)`, y los agujeros se achican).
        let (uno, cuatro) = (
            pintar(true, 1.0, &[CENTRO]),
            pintar(true, 1.0, &[CENTRO; 4]),
        );
        assert_eq!(contar(&uno), contar(&cuatro));
        let todo = |p: &[u8]| p.chunks(4).filter(|q| q[3] > 0).count();
        assert!(
            todo(&cuatro) > todo(&uno),
            "repetir el toque no achica los agujeros del hombro"
        );
    }

    #[test]
    fn un_trazo_pinta_dos_veces_igual_y_la_trama_no_depende_del_toque() {
        // Una linea de toques separados 0.5 del radio.
        let linea: Vec<Vec3> = (0..16)
            .map(|i| Vec3::new(1.0 + i as f32 * 0.5 * RADIO, 2.0, 0.0))
            .collect();
        for ceiling in [false, true] {
            let a = pintar(ceiling, 1.0, &linea);
            assert_eq!(
                a,
                pintar(ceiling, 1.0, &linea),
                "ceiling {ceiling}: no es determinista"
            );
            let pintados = a.chunks(4).filter(|q| q[3] > 0).count();
            assert!(
                pintados > 500,
                "ceiling {ceiling}: pinto pocos texeles: {pintados}"
            );
            // La trama esta anclada a la textura: un texel pintado por el trazo coincide con la trama de ese texel (donde
            // el punto vale 0 nunca se pinta: ni con la cobertura entera).
            for (i, q) in a.chunks(4).enumerate() {
                let (tx, ty) = (i as u32 % 256, i as u32 / 256);
                if patron(tx, ty) == 0.0 {
                    assert_eq!(
                        q[3], 0,
                        "ceiling {ceiling}: pinto el centro de un punto en ({tx}, {ty})"
                    );
                }
            }
        }
    }
}
