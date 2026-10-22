//! Pincel SALPICADURA (Spatter de photocraft; medio seco): `spatter_tip(64, 11, 14)` como punta, 1 o 2 racimos de
//! gotas por toque, cada uno con su tamano, giro, volteo y dispersion al azar. Sin papel ni lienzo: su preset de
//! photocraft no activa ninguna textura.
//!
//! Lo que copia del preset `Spatter` de photocraft (`presets.rs`):
//!
//! - Punta: un racimo de 14 gotas redondas de tamanos distintos (`spatter_tip(64, 11, 14)`); se arma con la misma
//!   funcion que el punteado (`punteado::construir`), con la semilla y la cantidad de gotas de este preset.
//! - Tamano: cada racimo mide del 40 al 100 % del radio (jitter 0.6, minimo 0.2: el minimo no llega a actuar).
//! - Giro: libre (jitter 1.0), cada racimo mira a un lado distinto.
//! - Volteo X e Y al azar, cada uno en la mitad de los racimos.
//! - Dispersion: hasta 1.5 veces el radio del racimo en cada eje del plano de la superficie (`both_axes`).
//! - Cantidad: `count 2` con `count_jitter 0.5`, que da 2 racimos en la mitad de los toques y 1 en la otra mitad.
//! - Radio 30 (60 de diametro), espaciado 1.2 del radio (0.6 del diametro) y flujo 1: estan en `brush.rs`.
//!
//! Lo que NO copia: nada del preset. No usa presion, color de fondo, opacidad ni textura. Lo unico que difiere es el
//! azar: la disposicion exacta de las gotas no es la de photocraft (que usa otro generador), pero la forma y las
//! proporciones si.
//!
//! El segundo racimo de un toque se guarda donde el follaje guarda su segunda hoja (`follaje::Extras::otra`), con
//! tono y brillo en 0: asi no hace falta agregar un campo a `DryDab` ni tocar los demas pinceles.

use std::f32::consts::{PI, SQRT_2};
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::unit;
use super::follaje::Extras;
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;
use super::punteado;

/// Semilla de la punta y cantidad de gotas (las del preset de photocraft).
const SPATTER_TIP_SEED: u32 = 11;
const SPATTER_DROPS: u32 = 14;
/// Dispersion de la salpicadura en cada eje, como fraccion del radio del racimo (1.5 = hasta el 150 %).
const SPATTER_SCATTER: f32 = 1.5;
/// Lo mas que se aleja el centro de un racimo del punto tocado, como fraccion del radio del pincel (la esquina del
/// cuadrado de lado 1.5; el racimo mas grande mide un radio, asi que este es el techo).
pub const SPATTER_REACH: f32 = SPATTER_SCATTER * SQRT_2;
/// Variacion del tamano de cada racimo: se reduce hasta `SIZE_JITTER` y nunca baja de `SIZE_MIN`.
const SIZE_JITTER: f32 = 0.6;
const SIZE_MIN: f32 = 0.2;

/// Sales del azar de cada racimo. Estan separadas por 2^32 para que el azar de un dato (tamano, giro...) nunca
/// coincida con el de otro en otro toque.
const SIZE_SALT: u64 = 0x5350_0001_0000_0000;
const ANGLE_SALT: u64 = 0x5350_0002_0000_0000;
const FLIP_X_SALT: u64 = 0x5350_0003_0000_0000;
const FLIP_Y_SALT: u64 = 0x5350_0004_0000_0000;
const SCATTER_X_SALT: u64 = 0x5350_0005_0000_0000;
const SCATTER_Y_SALT: u64 = 0x5350_0006_0000_0000;
const COUNT_SALT: u64 = 0x5350_0007_0000_0000;

/// La punta de la salpicadura (`spatter_tip(64, 11, 14)` de photocraft): un racimo de gotas redondas de tamanos
/// distintos. El valor de un texel es el de la gota que mas lo cubre (1 = pinta del todo).
fn construir() -> Punta {
    punteado::construir(SPATTER_TIP_SEED, SPATTER_DROPS)
}

/// La punta de la salpicadura (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Un racimo de gotas: `id` identifica su azar (cada racimo de cada toque tiene el suyo). `t0` y `b0` son la base de
/// la superficie. El giro es libre, el racimo se voltea en cada eje la mitad de las veces, mide del 40 al 100 % del
/// radio y su centro se desplaza hasta `SPATTER_SCATTER` veces el radio del racimo en cada eje.
fn racimo(id: u64, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    let r = |salt: u64| unit(salt.wrapping_add(id));
    let size = (1.0 - SIZE_JITTER * r(SIZE_SALT)).max(SIZE_MIN);
    let (sn, cs) = ((r(ANGLE_SALT) * 2.0 - 1.0) * PI).sin_cos();
    let flip_x = if r(FLIP_X_SALT) < 0.5 { -1.0 } else { 1.0 };
    let flip_y = if r(FLIP_Y_SALT) < 0.5 { -1.0 } else { 1.0 };
    // La dispersion se mide sobre el radio de ESTE racimo (ya reducido por el tamano), como en photocraft.
    let k = SPATTER_SCATTER * radius * size;
    let kx = (r(SCATTER_X_SALT) * 2.0 - 1.0) * k;
    let ky = (r(SCATTER_Y_SALT) * 2.0 - 1.0) * k;
    DryDab {
        kind: DryKind::Spatter,
        t: (t0 * cs + b0 * sn) * flip_x,
        b: (b0 * cs - t0 * sn) * flip_y,
        inv_radius: inv_radius / size,
        roundness: 1.0,
        shift: t0 * kx + b0 * ky,
        follaje: None,
    }
}

/// Un toque de salpicadura: 2 racimos en la mitad de los toques y 1 en la otra mitad (`count 2`, `count_jitter 0.5`
/// de photocraft: `round(2 * (1 - 0.5 r))` con `r` al azar de 0 a 1).
pub(super) fn nuevo(dab: u32, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    let id = u64::from(dab) * 2;
    let mut primero = racimo(id, inv_radius, radius, t0, b0);
    if unit(COUNT_SALT.wrapping_add(u64::from(dab))) <= 0.5 {
        primero.follaje = Some(Box::new(Extras {
            hue: 0.0,
            bright: 0.0,
            paint: None,
            otra: Some(racimo(id + 1, inv_radius, radius, t0, b0)),
        }));
    }
    primero
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};
    use crate::pinceles::dry_subtract;
    use crate::pinceles::punta::TIP_SIZE;

    #[test]
    fn la_punta_son_gotas_sueltas_y_es_determinista() {
        let a = construir();
        assert_eq!(a.data, construir().data);
        assert_eq!(a.data, punta().data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        let pintados = a.data.iter().filter(|v| **v > 0.0).count();
        // Tiene pintura, pero mucha menos que un disco entero: son 14 gotas con huecos entre ellas.
        assert!(
            pintados > 200 && pintados < n * n / 3,
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
        assert!(huecos > 100, "huecos {huecos}");
        // La esquina queda vacia.
        assert_eq!(a.data[0], 0.0);
        // No es la punta del punteado (otra semilla y otra cantidad de gotas).
        assert_ne!(a.data, punteado::construir(77, 40).data);
    }

    #[test]
    fn el_toque_tiene_uno_o_dos_racimos_y_cada_uno_varia_dentro_de_su_rango() {
        let radio = 2.0;
        let (mut con_uno, mut con_dos) = (0, 0);
        let (mut en_x, mut en_y) = (false, false);
        let (mut tam_min, mut tam_max) = (f32::MAX, 0.0f32);
        let mut volteos = [false, false];
        for dab in 1..400 {
            let d = DryDab::new(DryKind::Spatter, dab, Vec3::Z, Vec3::X, radio);
            let mut racimos = vec![&d];
            if let Some(o) = d.follaje.as_ref().and_then(|f| f.otra.as_ref()) {
                racimos.push(o);
                con_dos += 1;
            } else {
                con_uno += 1;
            }
            for h in racimos {
                // Tamano: de 0.4 a 1 del radio (la inversa del radio de la punta va de 1 a 2.5 veces la normal).
                let size = 1.0 / (h.inv_radius * radio);
                assert!((0.4 - 1e-4..=1.0 + 1e-4).contains(&size), "tamano {size}");
                tam_min = tam_min.min(size);
                tam_max = tam_max.max(size);
                assert_eq!(h.roundness, 1.0);
                // Cada eje se aleja a lo sumo 1.5 veces el radio del racimo (como mucho 1.5 radios del pincel).
                let tope = SPATTER_SCATTER * radio * size + 1e-4;
                assert!(
                    h.shift.x.abs() <= tope && h.shift.y.abs() <= tope,
                    "desplazamiento {:?}",
                    h.shift
                );
                en_x |= h.shift.x.abs() > 0.3;
                en_y |= h.shift.y.abs() > 0.3;
                // El volteo cambia el sentido del eje t respecto al b (el determinante de la base).
                volteos[(h.t.cross(h.b).dot(Vec3::Z) > 0.0) as usize] = true;
            }
            assert!(
                d.reach() <= SPATTER_REACH * radio + 1e-4,
                "alcance {}",
                d.reach()
            );
        }
        // Mas o menos la mitad de los toques lleva dos racimos.
        assert!(
            con_uno > 100 && con_dos > 100,
            "uno {con_uno}, dos {con_dos}"
        );
        assert!(en_x && en_y, "no se dispersa en los dos ejes");
        assert!(
            tam_min < 0.5 && tam_max > 0.9,
            "los tamanos casi no varian: de {tam_min} a {tam_max}"
        );
        assert!(volteos[0] && volteos[1], "el volteo no cambia nunca");
    }

    #[test]
    fn la_salpicadura_no_lleva_papel_ni_cambia_el_color_y_el_mismo_toque_da_lo_mismo() {
        assert_eq!(dry_subtract(DryKind::Spatter, Vec3::ONE, 5, 9), 0.0);
        // El segundo racimo guarda tono y brillo en 0: el color de cada texel es siempre el del pincel.
        for base in [
            [60u8, 160, 60],
            [255, 0, 0],
            [10, 10, 10],
            [200, 180, 40],
            [255, 255, 255],
        ] {
            for dab in 1..60 {
                let d = DryDab::new(DryKind::Spatter, dab, Vec3::Z, Vec3::X, 1.0).with_color(base);
                for p in [
                    Vec3::new(0.2, -0.1, 0.0),
                    Vec3::new(-0.5, 0.4, 0.0),
                    Vec3::ZERO,
                ] {
                    assert_eq!(d.alpha_color(p, base).1, base, "toque {dab}");
                }
            }
        }
        // Mismo toque, misma forma.
        let a = DryDab::new(DryKind::Spatter, 9, Vec3::Z, Vec3::X, 1.0);
        let b = DryDab::new(DryKind::Spatter, 9, Vec3::Z, Vec3::X, 1.0);
        let p = Vec3::new(0.2, -0.1, 0.0);
        assert_eq!(a.alpha(p), b.alpha(p));
    }

    #[test]
    fn la_salpicadura_pinta_gotas_con_el_color_del_pincel_y_no_pasa_de_su_alcance() {
        // Cuadrado de 2x2 unidades; en la textura de 256 ocupa 64 texeles de ancho y 128 de alto.
        let p = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let uv = [[0.25, 0.25], [0.5, 0.25], [0.5, 0.75], [0.25, 0.75]];
        let tris = [
            SurfTri {
                p: [p[0], p[1], p[2]],
                uv: [uv[0], uv[1], uv[2]],
            },
            SurfTri {
                p: [p[0], p[2], p[3]],
                uv: [uv[0], uv[2], uv[3]],
            },
        ];
        let opts = DabOptions {
            brush: BrushType::Spatter,
            normal: Vec3::Z,
            ..Default::default()
        };
        let (centro, radio) = (Vec3::new(1.0, 1.0, 0.0), 0.2);
        let base = [60u8, 160, 60];
        // Lo mas lejos que puede llegar un toque: su radio mas lo que se desplaza su centro.
        let alcance = radio * (1.0 + BrushType::Spatter.scatter());
        let pintar = |toques: u32| {
            let mut s = StrokeState::new(256);
            let mut pixels = vec![0u8; 256 * 256 * 4];
            s.begin_stroke();
            for _ in 0..toques {
                s.dab_surface(&mut pixels, &tris, centro, radio, base, 1.0, opts);
            }
            pixels
        };
        // (texeles pintados, texeles de la zona del alcance, pintados mas alla del alcance, pintados de otro color)
        let medir = |pixels: &[u8]| {
            let (mut pintados, mut en_zona, mut lejos, mut otro_color) = (0, 0, 0, 0);
            for ty in 0..256u32 {
                for tx in 0..256u32 {
                    let x = (tx as f32 / 255.0 - 0.25) / 0.25 * 2.0;
                    let y = (ty as f32 / 255.0 - 0.25) / 0.5 * 2.0;
                    if !(0.0..=2.0).contains(&x) || !(0.0..=2.0).contains(&y) {
                        continue;
                    }
                    let d = ((x - 1.0).powi(2) + (y - 1.0).powi(2)).sqrt();
                    let i = ((ty * 256 + tx) * 4) as usize;
                    if d < alcance - 0.1 {
                        en_zona += 1;
                    }
                    if pixels[i + 3] > 0 {
                        pintados += 1;
                        if d > alcance + 0.1 {
                            lejos += 1;
                        }
                        if pixels[i..i + 3] != base {
                            otro_color += 1;
                        }
                    }
                }
            }
            (pintados, en_zona, lejos, otro_color)
        };
        // Un toque: pinta gotas sueltas (quedan huecos), del color del pincel y sin pasar del alcance.
        let (pintados, en_zona, lejos, otro_color) = medir(&pintar(1));
        assert!(pintados > 5, "pinto pocos texeles: {pintados}");
        assert!(
            pintados < en_zona,
            "pintados {pintados} de {en_zona}: sin huecos"
        );
        assert_eq!(lejos, 0, "pinto mas alla del alcance");
        assert_eq!(otro_color, 0, "pinto con un color que no es el del pincel");
        // Varios toques en el mismo sitio (cada uno con su azar): sigue sin pasar del alcance y con variacion.
        let (mas, _, lejos, otro_color) = medir(&pintar(20));
        assert!(
            mas > pintados,
            "mas toques no pintan mas: {mas} contra {pintados}"
        );
        assert_eq!(lejos, 0, "pinto mas alla del alcance con varios toques");
        assert_eq!(otro_color, 0);
        // Determinista.
        assert_eq!(pintar(7), pintar(7));
    }
}
