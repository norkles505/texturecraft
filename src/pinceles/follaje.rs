//! Pincel de FOLLAJE (Foliage de photocraft; medio seco): `leaf_tip(64)` como punta, 1 o 2 hojas por toque, cada una
//! con tamano, giro, redondez, volteo y dispersion al azar, y un color un poco distinto por hoja. Sin papel ni lienzo:
//! su preset de photocraft no activa ninguna textura.
//!
//! Lo que copia del preset `Foliage` de photocraft (`presets.rs`):
//!
//! - Tamano: cada hoja mide del 50 al 100 % del radio (jitter 0.5, minimo 0.3).
//! - Giro: libre (jitter 1.0), cada hoja mira a un lado distinto.
//! - Redondez: del 60 al 100 % (jitter 0.4, minimo 0.4); aplasta la hoja a lo ancho.
//! - Volteo vertical al azar en la mitad de las hojas (en una hoja simetrica no se nota, pero es lo que hace el preset).
//! - Dispersion: hasta 1.2 veces el radio de la hoja en cada eje del plano de la superficie (`both_axes`).
//! - Cantidad: `count 2` con `count_jitter 0.5`, que da 2 hojas en la mitad de los toques y 1 en la otra mitad.
//! - Color por hoja (`per_tip`): el tono varia hasta +-0.02 vueltas (hue_jitter 0.04 * 0.5) y el brillo hasta +-0.12.
//!
//! Lo que NO copia: la mezcla entre el color frontal y el de fondo (`fg_bg` 0.6), porque texturecraft solo tiene un
//! color de pincel.

use std::f32::consts::{PI, SQRT_2};
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::unit;
use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Dispersion del follaje en cada eje, como fraccion del radio de la hoja (1.2 = hasta el 120 %).
pub const FOLIAGE_SCATTER: f32 = 1.2;
/// Lo mas que se aleja el centro de una hoja del punto tocado, como fraccion del radio del pincel (la esquina del
/// cuadrado de lado 1.2; la hoja mas grande mide un radio, asi que este es el techo).
pub const FOLIAGE_REACH: f32 = FOLIAGE_SCATTER * SQRT_2;
/// Variacion del tamano de cada hoja: se reduce hasta `SIZE_JITTER` y nunca baja de `SIZE_MIN`.
const SIZE_JITTER: f32 = 0.5;
const SIZE_MIN: f32 = 0.3;
/// Variacion de la redondez: se reduce hasta `ROUNDNESS_JITTER` y nunca baja de `ROUNDNESS_MIN`.
const ROUNDNESS_JITTER: f32 = 0.4;
const ROUNDNESS_MIN: f32 = 0.4;
/// Variacion del tono (en vueltas de 0 a 1; photocraft aplica la mitad del jitter) y del brillo de cada hoja.
const HUE_JITTER: f32 = 0.04;
const BRIGHTNESS_JITTER: f32 = 0.12;

const SIZE_SALT: u64 = 0x4653_495A;
const ANGLE_SALT: u64 = 0x4641_4E47;
const ROUNDNESS_SALT: u64 = 0x4652_4E44;
const FLIP_Y_SALT: u64 = 0x4646_4C59;
const SCATTER_X_SALT: u64 = 0x4653_5841;
const SCATTER_Y_SALT: u64 = 0x4653_5941;
const HUE_SALT: u64 = 0x4648_5545;
const BRIGHT_SALT: u64 = 0x4642_5247;
const COUNT_SALT: u64 = 0x4643_4E54;

/// Lo que el follaje agrega a un toque (`DryDab::follaje`): el cambio de color de la hoja y la segunda hoja, si la hay.
pub(super) struct Extras {
    /// Cambio de tono (en vueltas) y de brillo de esta hoja.
    pub(super) hue: f32,
    pub(super) bright: f32,
    /// Color de esta hoja ya calculado (`DryDab::with_color`); `None` mientras no se conoce el color del pincel.
    pub(super) paint: Option<[u8; 3]>,
    /// La segunda hoja del toque.
    pub(super) otra: Option<DryDab>,
}

/// La punta del follaje (`leaf_tip(64)` de photocraft): una hoja puntiaguda por los dos lados, con una nervadura
/// central mas clara, venas oblicuas y un rabito hacia -x. El valor de un texel es la cobertura de la hoja (1 = pinta
/// del todo); el borde es suave de un texel.
fn construir() -> Punta {
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (u, v) = ((x as f32 + 0.5 - c) / c, (y as f32 + 0.5 - c) / c);
            // Medio ancho de la hoja en u: lo mas ancho un poco detras del medio, en punta por los dos extremos.
            let t = ((u + 1.0) / 2.0).clamp(0.0, 1.0);
            let half = 0.42 * (PI * t).sin() * (1.0 - 0.25 * t);
            let inside = ((half - v.abs()) * c).clamp(0.0, 1.0);
            let rib = 1.0 - 0.45 * (1.0 - (v.abs() * c / 1.2).clamp(0.0, 1.0));
            let veins = 1.0 - 0.2 * (1.0 - (((u * 7.0 - v.abs() * 5.0).fract() - 0.5).abs() * 8.0).clamp(0.0, 1.0));
            let stem = if u < -0.75 && v.abs() * c < 0.9 { 1.0 } else { 0.0 };
            data.push((inside * rib * veins).max(stem));
        }
    }
    Punta::new(data)
}

/// La punta del follaje (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Una hoja: `id` identifica su azar (cada hoja de cada toque tiene el suyo). `t0` y `b0` son la base de la
/// superficie. El giro es libre, la hoja se aplasta hasta `ROUNDNESS_JITTER`, mide del 50 al 100 % del radio y su
/// centro se desplaza hasta `FOLIAGE_SCATTER` veces el radio de la hoja en cada eje.
fn hoja(id: u64, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    let r = |salt: u64| unit(salt.wrapping_add(id));
    let size = (1.0 - SIZE_JITTER * r(SIZE_SALT)).max(SIZE_MIN);
    let (sn, cs) = ((r(ANGLE_SALT) * 2.0 - 1.0) * PI).sin_cos();
    let roundness = (1.0 - ROUNDNESS_JITTER * r(ROUNDNESS_SALT)).max(ROUNDNESS_MIN);
    let flip = if r(FLIP_Y_SALT) < 0.5 { -1.0 } else { 1.0 };
    // La dispersion se mide sobre el radio de ESTA hoja (ya reducido por el tamano), como en photocraft.
    let k = FOLIAGE_SCATTER * radius * size;
    let kx = (r(SCATTER_X_SALT) * 2.0 - 1.0) * k;
    let ky = (r(SCATTER_Y_SALT) * 2.0 - 1.0) * k;
    DryDab {
        kind: DryKind::Foliage,
        t: t0 * cs + b0 * sn,
        b: (b0 * cs - t0 * sn) * flip,
        inv_radius: inv_radius / size,
        roundness,
        shift: t0 * kx + b0 * ky,
        follaje: Some(Box::new(Extras {
            hue: (r(HUE_SALT) * 2.0 - 1.0) * HUE_JITTER * 0.5,
            bright: (r(BRIGHT_SALT) * 2.0 - 1.0) * BRIGHTNESS_JITTER,
            paint: None,
            otra: None,
        })),
    }
}

/// Un toque de follaje: 2 hojas en la mitad de los toques y 1 en la otra mitad (`count 2`, `count_jitter 0.5` de
/// photocraft: `round(2 * (1 - 0.5 r))` con `r` al azar de 0 a 1).
pub(super) fn nuevo(dab: u32, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3) -> DryDab {
    let id = u64::from(dab) * 2;
    let mut primera = hoja(id, inv_radius, radius, t0, b0);
    if unit(COUNT_SALT.wrapping_add(u64::from(dab))) <= 0.5 {
        if let Some(e) = primera.follaje.as_mut() {
            e.otra = Some(hoja(id + 1, inv_radius, radius, t0, b0));
        }
    }
    primera
}

/// De RGB (0..1) a tono, saturacion y valor (el tono va de 0 a 1).
fn rgb_a_hsv(c: [f32; 3]) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let d = mx - mn;
    let h = if d <= 0.0 {
        0.0
    } else if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    } / 6.0;
    [h, if mx > 0.0 { d / mx } else { 0.0 }, mx]
}

fn hsv_a_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

/// El color `base` con el tono corrido `hue` (en vueltas) y el brillo corrido `bright` (la saturacion no cambia).
pub(super) fn matizar(base: [u8; 3], hue: f32, bright: f32) -> [u8; 3] {
    let c = [f32::from(base[0]) / 255.0, f32::from(base[1]) / 255.0, f32::from(base[2]) / 255.0];
    let [h, s, v] = rgb_a_hsv(c);
    let o = hsv_a_rgb(h + hue, s, (v + bright).clamp(0.0, 1.0));
    [0, 1, 2].map(|k| (o[k] * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};

    #[test]
    fn la_punta_es_una_hoja_con_rabito_y_es_determinista() {
        let a = construir();
        assert_eq!(a.data, construir().data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        // El rabito sale por el lado -x, justo en el centro vertical.
        assert_eq!(a.data[(n / 2 - 1) * n + 3], 1.0);
        // En el medio de la hoja hay pintura; arriba del todo y en la esquina, no.
        assert!(a.data[(n / 2 - 1) * n + n / 2 - 2] > 0.4);
        assert_eq!(a.data[n / 2], 0.0);
        assert_eq!(a.data[0], 0.0);
        // Es mas larga que ancha: el centro horizontal pinta mas texeles que el vertical.
        let fila = (0..n).filter(|&x| a.data[(n / 2) * n + x] > 0.0).count();
        let columna = (0..n).filter(|&y| a.data[y * n + n / 2] > 0.0).count();
        assert!(fila > columna + 10, "fila {fila}, columna {columna}");
    }

    #[test]
    fn el_toque_tiene_una_o_dos_hojas_y_cada_una_varia_dentro_de_su_rango() {
        let radio = 2.0;
        let (mut con_una, mut con_dos) = (0, 0);
        let (mut en_x, mut en_y) = (false, false);
        let (mut tam_min, mut tam_max) = (f32::MAX, 0.0f32);
        let mut volteos = [false, false];
        for dab in 1..400 {
            let d = DryDab::new(DryKind::Foliage, dab, Vec3::Z, Vec3::X, radio);
            let mut hojas = vec![&d];
            if let Some(o) = d.follaje.as_ref().and_then(|f| f.otra.as_ref()) {
                hojas.push(o);
                con_dos += 1;
            } else {
                con_una += 1;
            }
            for h in hojas {
                // Tamano: de 0.5 a 1 del radio (la inversa del radio de la punta va de 1 a 2 veces la normal).
                let size = 1.0 / (h.inv_radius * radio);
                assert!((0.5 - 1e-4..=1.0 + 1e-4).contains(&size), "tamano {size}");
                tam_min = tam_min.min(size);
                tam_max = tam_max.max(size);
                assert!((0.6 - 1e-4..=1.0).contains(&h.roundness), "redondez {}", h.roundness);
                // Cada eje se aleja a lo sumo 1.2 veces el radio de la hoja (como mucho 1.2 radios del pincel).
                assert!(h.shift.x.abs() <= FOLIAGE_SCATTER * radio + 1e-4 && h.shift.y.abs() <= FOLIAGE_SCATTER * radio + 1e-4);
                en_x |= h.shift.x.abs() > 0.3;
                en_y |= h.shift.y.abs() > 0.3;
                volteos[(h.t.cross(h.b).dot(Vec3::Z) > 0.0) as usize] = true;
            }
            assert!(d.reach() <= FOLIAGE_REACH * radio + 1e-4, "alcance {}", d.reach());
        }
        // Mas o menos la mitad de los toques lleva dos hojas.
        assert!(con_una > 100 && con_dos > 100, "una {con_una}, dos {con_dos}");
        assert!(en_x && en_y, "no se dispersa en los dos ejes");
        assert!(tam_max - tam_min > 0.3, "los tamanos casi no varian: de {tam_min} a {tam_max}");
        assert!(volteos[0] && volteos[1], "el volteo no cambia nunca");
    }

    #[test]
    fn el_follaje_no_lleva_papel_y_el_mismo_toque_da_lo_mismo() {
        use crate::pinceles::dry_subtract;
        assert_eq!(dry_subtract(DryKind::Foliage, Vec3::ONE, 5, 9), 0.0);
        let a = DryDab::new(DryKind::Foliage, 9, Vec3::Z, Vec3::X, 1.0).with_color([60, 160, 60]);
        let b = DryDab::new(DryKind::Foliage, 9, Vec3::Z, Vec3::X, 1.0).with_color([60, 160, 60]);
        let p = Vec3::new(0.2, -0.1, 0.0);
        assert_eq!(a.alpha_color(p, [60, 160, 60]), b.alpha_color(p, [60, 160, 60]));
    }

    #[test]
    fn cada_hoja_tiene_su_color_y_los_otros_pinceles_usan_el_del_pincel() {
        let base = [60u8, 160, 60];
        let mut colores = HashSet::new();
        let (mut verde_min, mut verde_max) = (255u8, 0u8);
        for dab in 1..200 {
            let d = DryDab::new(DryKind::Foliage, dab, Vec3::Z, Vec3::X, 1.0).with_color(base);
            let c = d.follaje.as_ref().and_then(|f| f.paint).expect("la hoja no tiene color");
            colores.insert(c);
            verde_min = verde_min.min(c[1]);
            verde_max = verde_max.max(c[1]);
            // El brillo corre a lo sumo 0.12 (31 de 255) y el tono unos 12: nunca se aleja mucho del color del pincel.
            for k in 0..3 {
                assert!((i32::from(c[k]) - i32::from(base[k])).abs() <= 60, "color {c:?} lejos de {base:?}");
            }
        }
        assert!(colores.len() > 20, "pocos colores distintos: {}", colores.len());
        assert!(verde_max - verde_min > 20, "el verde casi no varia: de {verde_min} a {verde_max}");
        // Los demas pinceles de medio seco no cambian el color.
        let s = DryDab::new(DryKind::Stipple, 3, Vec3::Z, Vec3::X, 1.0).with_color(base);
        assert_eq!(s.alpha_color(Vec3::new(0.1, 0.1, 0.0), base).1, base);
    }

    #[test]
    fn el_matiz_no_cambia_el_color_si_no_se_mueve_nada() {
        for c in [[60u8, 160, 60], [255, 0, 0], [10, 10, 10], [200, 180, 40], [255, 255, 255]] {
            assert_eq!(matizar(c, 0.0, 0.0), c);
        }
        // Mas brillo aclara, menos brillo oscurece.
        let c = [60u8, 160, 60];
        assert!(matizar(c, 0.0, 0.1)[1] > c[1]);
        assert!(matizar(c, 0.0, -0.1)[1] < c[1]);
    }

    #[test]
    fn el_follaje_pinta_hojas_de_colores_distintos_y_no_pasa_de_su_alcance() {
        let p = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 0.0), Vec3::new(0.0, 2.0, 0.0)];
        let uv = [[0.25, 0.25], [0.5, 0.25], [0.5, 0.75], [0.25, 0.75]];
        let tris = [
            SurfTri { p: [p[0], p[1], p[2]], uv: [uv[0], uv[1], uv[2]] },
            SurfTri { p: [p[0], p[2], p[3]], uv: [uv[0], uv[2], uv[3]] },
        ];
        let opts = DabOptions { brush: BrushType::Foliage, normal: Vec3::Z, ..Default::default() };
        let mut s = StrokeState::new(64);
        let mut pixels = vec![0u8; 64 * 64 * 4];
        s.begin_stroke();
        let base = [60u8, 160, 60];
        let radio = 0.3;
        for i in 0..12 {
            let c = Vec3::new(0.5 + i as f32 * 0.1, 1.0, 0.0);
            s.dab_surface(&mut pixels, &tris, c, radio, base, 1.0, opts);
        }
        let (mut pintados, mut colores) = (0, HashSet::new());
        for q in pixels.chunks(4) {
            if q[3] > 0 {
                pintados += 1;
                colores.insert([q[0], q[1], q[2]]);
            }
        }
        assert!(pintados > 30, "pinto pocos texeles: {pintados}");
        assert!(colores.len() > 3, "todas las hojas salen del mismo color: {}", colores.len());
    }
}
