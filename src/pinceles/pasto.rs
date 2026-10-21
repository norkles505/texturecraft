//! Pincel PASTO (Grass de photocraft; medio seco): `grass_tip(96, 6, 11)` como punta (aqui de 64), un manojo de 11
//! hojas finas que suben desde el borde de abajo, 1 manojo por toque con su tamano, giro leve, volteo, dispersion de
//! lado y un matiz de color propios. Sin papel ni lienzo: su preset de photocraft no activa ninguna textura.
//!
//! Lo que copia del preset `Grass` de photocraft (`presets.rs`):
//!
//! - Punta: 11 hojas conicas, un poco curvadas, que nacen en el borde de abajo (`grass_tip(96, 6, 11)`), con la misma
//!   forma y proporciones que la de photocraft.
//! - Tamano: cada manojo mide del 60 al 100 % del radio (jitter 0.4; el minimo 0.4 no llega a actuar).
//! - Giro: leve (jitter 0.06 de +-180 grados = hasta +-10.8 grados), asi el pasto siempre queda casi derecho.
//! - Volteo X al azar en la mitad de los manojos.
//! - Dispersion: hasta 0.3 veces el radio del manojo, SOLO de lado respecto al trazo (`both_axes` apagado), igual que
//!   el pastel.
//! - Color por manojo (`per_tip`): el tono varia hasta +-0.015 vueltas (hue_jitter 0.03 * 0.5); sin cambio de brillo.
//! - Radio 35 (70 de diametro), espaciado 0.5 del radio (0.25 del diametro) y flujo 1: estan en `brush.rs`.
//!
//! Lo que NO copia:
//!
//! - La presion del lapiz (el tamano tiene `PenPressure` con minimo 0.4): con mouse no hay presion, asi que solo queda
//!   la variacion al azar.
//! - La mezcla entre el color frontal y el de fondo (`fg_bg` 0.7), porque texturecraft solo tiene un color de pincel.
//!   Por eso el pasto sale casi de un solo color (solo cambia un poco el tono de cada manojo).
//! - La punta de 96 px: aqui es de 64 (`TIP_SIZE`). La forma y las proporciones coinciden (el borde suave se achica
//!   en la misma proporcion), pero la posicion y el largo exactos de cada hoja no, porque el azar es el de texturecraft.
//!
//! Lo que difiere por ser 3D: en photocraft el pasto sube hacia arriba de la pantalla. Aqui sube hacia ARRIBA DEL
//! MODELO (+Z: texturecraft tiene Z arriba, `obj.rs`), proyectado en la superficie. En una superficie casi horizontal
//! (donde "arriba" no apunta a ningun lado) se usa la base de la superficie, como los demas pinceles.
//!
//! Alcance: la punta no cabe en el circulo del radio (las hojas llegan hasta la esquina del cuadrado, raiz de 2
//! radios), asi que ademas de la dispersion `DryDab::reach` suma ese exceso (`GRASS_OVERSHOOT`).
//!
//! El color del manojo se guarda donde el follaje guarda el de su hoja (`follaje::Extras`), con brillo en 0: asi no hace
//! falta agregar un campo a `DryDab` ni tocar los demas pinceles.

use std::f32::consts::{PI, SQRT_2};
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{hash2, unit};
use super::follaje::Extras;
use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Semilla de la punta y cantidad de hojas (las del preset de photocraft).
const GRASS_TIP_SEED: u32 = 6;
const GRASS_BLADES: u32 = 11;
/// Lado de la punta en photocraft: sirve para achicar en la misma proporcion el borde suave de las hojas.
const PHOTOCRAFT_TIP: f32 = 96.0;
/// Medio ancho extra de cada hoja (0.3 px en la punta de 96): le da punta suave y evita que se corte. Aqui, a escala.
const BLADE_EDGE: f32 = 0.3 * TIP_SIZE as f32 / PHOTOCRAFT_TIP;
/// Dispersion del pasto, de lado, como fraccion del radio del manojo (0.3 = hasta el 30 %).
pub const GRASS_SCATTER: f32 = 0.3;
/// Lo que las hojas se salen del circulo del radio: la esquina del cuadrado de la punta esta a raiz de 2 radios.
pub(super) const GRASS_OVERSHOOT: f32 = SQRT_2 - 1.0;
/// Lo mas que se aleja un texel pintado del punto tocado, pasado del radio, como fraccion del radio del pincel
/// (dispersion mas exceso de la punta). Es lo que devuelve `BrushType::scatter`.
pub const GRASS_REACH: f32 = GRASS_SCATTER + GRASS_OVERSHOOT;
/// Variacion del tamano de cada manojo: se reduce hasta `SIZE_JITTER` y nunca baja de `SIZE_MIN`.
const SIZE_JITTER: f32 = 0.4;
const SIZE_MIN: f32 = 0.4;
/// Giro leve de cada manojo: fraccion de +-180 grados (0.06 = hasta +-10.8 grados).
const ANGLE_JITTER: f32 = 0.06;
/// Variacion del tono (en vueltas de 0 a 1; photocraft aplica la mitad del jitter).
const HUE_JITTER: f32 = 0.03;
/// Hacia donde crece el pasto: arriba del modelo (texturecraft tiene Z arriba).
const UP: Vec3 = Vec3::Z;
/// Si el "arriba" proyectado en la superficie mide menos que esto (la superficie es casi horizontal), no se usa.
const MIN_UP_LENGTH_SQ: f32 = 1e-3;

/// Sales del azar de cada manojo. Estan separadas por 2^32 para que el azar de un dato (tamano, giro...) nunca
/// coincida con el de otro en otro toque.
const SIZE_SALT: u64 = 0x4752_0001_0000_0000;
const ANGLE_SALT: u64 = 0x4752_0002_0000_0000;
const FLIP_X_SALT: u64 = 0x4752_0003_0000_0000;
const SCATTER_SALT: u64 = 0x4752_0004_0000_0000;
const HUE_SALT: u64 = 0x4752_0005_0000_0000;

/// La punta del pasto (`grass_tip(96, 6, 11)` de photocraft): un manojo de hojas cónicas, un poco curvadas, que nacen
/// en el borde de abajo. Cada hoja tiene su lugar de nacimiento, su alto, su inclinacion (que se nota mas hacia la
/// punta) y su ancho. El valor de un texel es el de la hoja que mas lo cubre (1 = pinta del todo).
fn construir() -> Punta {
    let n = TIP_SIZE;
    let s = n as f32;
    let salt = u64::from(GRASS_TIP_SEED) + 505;
    // (donde nace, alto, inclinacion, ancho) de cada hoja.
    let hojas: Vec<(f32, f32, f32, f32)> = (0..GRASS_BLADES)
        .map(|i| {
            let base = (0.3 + 0.4 * hash2(i as i32, 0, salt)) * s;
            let alto = (0.55 + 0.42 * hash2(i as i32, 1, salt)) * s;
            let inclinacion = (hash2(i as i32, 2, salt) - 0.5) * 0.9;
            let ancho = (0.025 + 0.025 * hash2(i as i32, 3, salt)) * s;
            (base, alto, inclinacion, ancho)
        })
        .collect();
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // Altura sobre el borde de abajo.
            let arriba = s - py;
            let v = hojas.iter().fold(0.0f32, |acc, &(base, alto, inclinacion, ancho)| {
                if arriba < 0.0 || arriba > alto {
                    return acc;
                }
                let t = arriba / alto;
                let centro = base + inclinacion * alto * t * t;
                let medio = ancho * (1.0 - t) + BLADE_EDGE;
                acc.max((medio - (px - centro).abs()).clamp(0.0, 1.0))
            });
            data.push(v);
        }
    }
    Punta::new(data)
}

/// La punta del pasto (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(construir)
}

/// Un toque de pasto: un manojo que mide del 60 al 100 % del radio, gira un poco, se voltea la mitad de las veces y su
/// centro se desplaza de lado respecto al trazo hasta `GRASS_SCATTER` veces el radio del manojo. `t0` y `b0` son la
/// base de la superficie, `n` su normal y `along` la direccion del trazo proyectada en el plano (si no se sabe hacia
/// donde va, la dispersion va a lo ancho del manojo).
pub(super) fn nuevo(dab: u32, inv_radius: f32, radius: f32, t0: Vec3, b0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    let id = u64::from(dab);
    let r = |salt: u64| unit(salt.wrapping_add(id));
    let size = (1.0 - SIZE_JITTER * r(SIZE_SALT)).max(SIZE_MIN);
    // Base del manojo: el eje `b` baja por la punta (hacia donde nacen las hojas), asi que apunta contra el "arriba".
    // Con la superficie casi horizontal no hay "arriba" en ella: se usa la base de la superficie.
    let arriba = UP - n * UP.dot(n);
    let (t_base, b_base) = if arriba.length_squared() > MIN_UP_LENGTH_SQ {
        let b = -arriba.normalize();
        (b.cross(n), b)
    } else {
        (t0, b0)
    };
    let (sn, cs) = ((r(ANGLE_SALT) * 2.0 - 1.0) * ANGLE_JITTER * PI).sin_cos();
    let flip_x = if r(FLIP_X_SALT) < 0.5 { -1.0 } else { 1.0 };
    // La dispersion se mide sobre el radio de ESTE manojo (ya reducido por el tamano), como en photocraft.
    let side = along.map_or(t_base, |a| n.cross(a));
    let k = (r(SCATTER_SALT) * 2.0 - 1.0) * GRASS_SCATTER * radius * size;
    DryDab {
        kind: DryKind::Grass,
        t: (t_base * cs + b_base * sn) * flip_x,
        b: b_base * cs - t_base * sn,
        inv_radius: inv_radius / size,
        roundness: 1.0,
        shift: side * k,
        follaje: Some(Box::new(Extras { hue: (r(HUE_SALT) * 2.0 - 1.0) * HUE_JITTER * 0.5, bright: 0.0, paint: None, otra: None })),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};
    use crate::pinceles::dry_subtract;

    #[test]
    fn la_punta_es_un_manojo_que_sube_desde_abajo_y_es_determinista() {
        let a = construir();
        assert_eq!(a.data, construir().data);
        assert_eq!(a.data, punta().data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let n = TIP_SIZE;
        let pintados = a.data.iter().filter(|v| **v > 0.0).count();
        // Tiene pintura, pero mucha menos que un disco entero: son hojas finas con huecos entre ellas.
        assert!(pintados > 150 && pintados < n * n / 2, "pintados {pintados}");
        // En la fila de la mitad quedan huecos entre las hojas.
        let huecos = (0..n).filter(|&x| a.data[(n / 2) * n + x] == 0.0).count();
        assert!(huecos > n / 3, "huecos {huecos}");
        // Las hojas nacen en el borde de abajo y no llegan al de arriba.
        assert!((0..n).any(|x| a.data[(n - 1) * n + x] > 0.0), "nada pinta en la fila de abajo");
        assert!((0..n).all(|x| a.data[x] == 0.0), "algo pinta en la fila de arriba");
        // Las esquinas quedan vacias.
        for i in [0, n - 1, n * (n - 1), n * n - 1] {
            assert_eq!(a.data[i], 0.0, "esquina {i}");
        }
        // Se afinan hacia la punta: hay mas pintura en la mitad de abajo que en la de arriba.
        let mitad = |desde: usize, hasta: usize| (desde * n..hasta * n).filter(|&i| a.data[i] > 0.0).count();
        assert!(mitad(n / 2, n) > mitad(0, n / 2), "abajo {} y arriba {}", mitad(n / 2, n), mitad(0, n / 2));
        // No es la punta de otro pincel: la de la salpicadura tiene otra forma.
        assert_ne!(a.data, super::super::salpicadura::punta().data);
    }

    #[test]
    fn el_toque_varia_dentro_de_su_rango_y_se_dispersa_solo_de_lado() {
        let radio = 2.0;
        let normal = -Vec3::Y;
        let (mut tam_min, mut tam_max) = (f32::MAX, 0.0f32);
        let mut volteos = [false, false];
        let (mut max_giro, mut lado_max) = (0.0f32, 0.0f32);
        for dab in 1..400 {
            // Pared de cara al ojo: normal -Y, arriba +Z; el trazo va hacia +X.
            let d = DryDab::new(DryKind::Grass, dab, normal, Vec3::X, radio);
            // Tamano: de 0.6 a 1 del radio (la inversa del radio de la punta va de 1 a 1.67 veces la normal).
            let size = 1.0 / (d.inv_radius * radio);
            assert!((0.6 - 1e-4..=1.0 + 1e-4).contains(&size), "tamano {size}");
            tam_min = tam_min.min(size);
            tam_max = tam_max.max(size);
            assert_eq!(d.roundness, 1.0);
            // El eje b baja por la punta: apunta hacia abajo (-Z) y se desvia a lo sumo 10.8 grados.
            let giro = d.b.dot(-Vec3::Z).clamp(-1.0, 1.0).acos();
            max_giro = max_giro.max(giro);
            assert!(giro <= ANGLE_JITTER * PI + 1e-3, "giro {giro}");
            // Se dispersa solo de lado: nada a lo largo del trazo (+X) y a lo sumo 0.3 del radio del manojo.
            assert!(d.shift.dot(Vec3::X).abs() < 1e-4, "se desplaza a lo largo del trazo");
            assert!(d.shift.length() <= GRASS_SCATTER * radio * size + 1e-4, "desplazamiento {:?}", d.shift);
            lado_max = lado_max.max(d.shift.length());
            // El volteo cambia el sentido del eje t respecto al b (el determinante de la base).
            volteos[(d.t.cross(d.b).dot(normal) > 0.0) as usize] = true;
            // Un solo manojo por toque.
            assert!(d.follaje.as_ref().is_some_and(|f| f.otra.is_none()));
            assert!(d.reach() <= GRASS_REACH * radio + 1e-4, "alcance {}", d.reach());
        }
        assert!(tam_min < 0.7 && tam_max > 0.95, "los tamanos casi no varian: de {tam_min} a {tam_max}");
        assert!(max_giro > 0.1, "el giro casi no varia: {max_giro}");
        assert!(lado_max > 0.2, "casi no se dispersa: {lado_max}");
        assert!(volteos[0] && volteos[1], "el volteo no cambia nunca");
    }

    #[test]
    fn el_pasto_sube_hacia_arriba_del_modelo_y_sin_arriba_usa_la_base_de_la_superficie() {
        // Una pared inclinada: la brizna sube por la pared, hacia +Z.
        let n = Vec3::new(0.0, -1.0, 0.5).normalize();
        let d = DryDab::new(DryKind::Grass, 7, n, Vec3::X, 1.0);
        assert!(d.b.dot(Vec3::Z) < -0.5, "el pasto no sube: b = {:?}", d.b);
        // El suelo (normal +Z) no tiene "arriba" en la superficie: no falla y da una base valida.
        let s = DryDab::new(DryKind::Grass, 7, Vec3::Z, Vec3::X, 1.0);
        assert!((s.t.length() - 1.0).abs() < 1e-4 && (s.b.length() - 1.0).abs() < 1e-4);
        assert!(s.t.dot(s.b).abs() < 1e-4);
        // Sin normal (cero) tampoco falla.
        let z = DryDab::new(DryKind::Grass, 7, Vec3::ZERO, Vec3::ZERO, 1.0);
        assert!(z.alpha(Vec3::new(0.1, 0.1, 0.0)).is_finite());
    }

    #[test]
    fn el_pasto_no_lleva_papel_cambia_poco_el_tono_y_el_mismo_toque_da_lo_mismo() {
        assert_eq!(dry_subtract(DryKind::Grass, Vec3::ONE, 5, 9), 0.0);
        let base = [60u8, 160, 60];
        let mut colores = HashSet::new();
        for dab in 1..200 {
            let d = DryDab::new(DryKind::Grass, dab, -Vec3::Y, Vec3::X, 1.0).with_color(base);
            let c = d.follaje.as_ref().and_then(|f| f.paint).expect("el manojo no tiene color");
            colores.insert(c);
            // Solo cambia el tono: el brillo (el canal mas alto) se queda igual y los canales casi no se mueven.
            assert_eq!(*c.iter().max().unwrap(), 160, "color {c:?}");
            for k in 0..3 {
                assert!((i32::from(c[k]) - i32::from(base[k])).abs() <= 15, "color {c:?} lejos de {base:?}");
            }
            // El color de cada texel es el del manojo.
            assert_eq!(d.alpha_color(Vec3::new(0.0, 0.0, -0.9), base).1, c);
        }
        assert!(colores.len() > 5, "pocos colores distintos: {}", colores.len());
        // Mismo toque, misma forma.
        let a = DryDab::new(DryKind::Grass, 9, -Vec3::Y, Vec3::X, 1.0);
        let b = DryDab::new(DryKind::Grass, 9, -Vec3::Y, Vec3::X, 1.0);
        let p = Vec3::new(0.1, 0.0, -0.8);
        assert_eq!(a.alpha(p), b.alpha(p));
    }

    #[test]
    fn el_pasto_pinta_hojas_del_color_del_pincel_sube_y_no_pasa_de_su_alcance() {
        // Pared de 2x2 unidades en el plano y = 0 (x a la derecha, z hacia arriba), de cara al ojo (normal -Y). En la
        // textura de 256 ocupa 64 texeles de ancho y 128 de alto.
        let p = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 2.0), Vec3::new(0.0, 0.0, 2.0)];
        let uv = [[0.25, 0.25], [0.5, 0.25], [0.5, 0.75], [0.25, 0.75]];
        let tris = [
            SurfTri { p: [p[0], p[1], p[2]], uv: [uv[0], uv[1], uv[2]] },
            SurfTri { p: [p[0], p[2], p[3]], uv: [uv[0], uv[2], uv[3]] },
        ];
        let opts = DabOptions { brush: BrushType::Grass, normal: -Vec3::Y, dir: Vec3::X, ..Default::default() };
        let (centro, radio) = (Vec3::new(1.0, 0.0, 1.0), 0.2);
        let base = [60u8, 160, 60];
        // Lo mas lejos que puede llegar un toque: su radio mas lo que se desplaza y se sale de la punta.
        let alcance = radio * (1.0 + BrushType::Grass.scatter());
        let pintar = |toques: u32| {
            let mut s = StrokeState::new(256);
            let mut pixels = vec![0u8; 256 * 256 * 4];
            s.begin_stroke();
            for _ in 0..toques {
                s.dab_surface(&mut pixels, &tris, centro, radio, base, 1.0, opts);
            }
            pixels
        };
        // (pintados, texeles de la zona del alcance, pintados mas alla del alcance, pintados de un color lejano al del
        // pincel, pintados arriba del centro, pintados abajo del centro, colores distintos)
        let medir = |pixels: &[u8]| {
            let (mut pintados, mut en_zona, mut lejos, mut otro_color) = (0, 0, 0, 0);
            let (mut arriba, mut abajo) = (0, 0);
            let mut colores = HashSet::new();
            for ty in 0..256u32 {
                for tx in 0..256u32 {
                    let x = (tx as f32 / 255.0 - 0.25) / 0.25 * 2.0;
                    let z = (ty as f32 / 255.0 - 0.25) / 0.5 * 2.0;
                    if !(0.0..=2.0).contains(&x) || !(0.0..=2.0).contains(&z) {
                        continue;
                    }
                    let d = ((x - 1.0).powi(2) + (z - 1.0).powi(2)).sqrt();
                    let i = ((ty * 256 + tx) * 4) as usize;
                    if d < alcance - 0.1 {
                        en_zona += 1;
                    }
                    if pixels[i + 3] > 0 {
                        pintados += 1;
                        if d > alcance + 0.1 {
                            lejos += 1;
                        }
                        let c = [pixels[i], pixels[i + 1], pixels[i + 2]];
                        if (0..3).any(|k| (i32::from(c[k]) - i32::from(base[k])).abs() > 15) {
                            otro_color += 1;
                        }
                        colores.insert(c);
                        if z > 1.0 {
                            arriba += 1;
                        } else {
                            abajo += 1;
                        }
                    }
                }
            }
            (pintados, en_zona, lejos, otro_color, arriba, abajo, colores.len())
        };
        // Un toque: pinta hojas sueltas (quedan huecos), de un color cercano al del pincel y sin pasar del alcance.
        let (pintados, en_zona, lejos, otro_color, _, _, _) = medir(&pintar(1));
        assert!(pintados > 5, "pinto pocos texeles: {pintados}");
        assert!(pintados < en_zona, "pintados {pintados} de {en_zona}: sin huecos");
        assert_eq!(lejos, 0, "pinto mas alla del alcance");
        assert_eq!(otro_color, 0, "pinto con un color lejano al del pincel");
        // Varios toques en el mismo sitio (cada uno con su azar): sigue sin pasar del alcance, con variacion de tono y
        // con mas pintura abajo (donde nacen las hojas) que arriba.
        let (mas, _, lejos, otro_color, arriba, abajo, colores) = medir(&pintar(20));
        assert!(mas > pintados, "mas toques no pintan mas: {mas} contra {pintados}");
        assert_eq!(lejos, 0, "pinto mas alla del alcance con varios toques");
        assert_eq!(otro_color, 0);
        assert!(colores > 1, "todos los manojos salen del mismo color");
        assert!(abajo > arriba, "el pasto no nace abajo: abajo {abajo}, arriba {arriba}");
        // Determinista.
        assert_eq!(pintar(7), pintar(7));
    }
}
