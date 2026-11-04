//! Lo que tienen en comun los pinceles portados del paquete de Blender `GP_Brushes_3.blend` (Grease Pencil):
//! ASTILLAS (`gp_astillas.rs`), ARCO (`gp_arco.rs`) y TRAMA DE PUNTOS (`gp_puntos.rs`). Cada uno guarda su punta en un
//! archivo de texto de `assets/pinceles/` y arma su toque en su propio archivo; aqui solo estan la lectura de esa punta y
//! la orientacion del toque.
//!
//! Los tres son pinceles de Blender en modo `DOTS` con la textura estampada a lo largo del trazo: la textura es una
//! mascara de opacidad (el material mezcla el color del trazo al 100 %, `mix_stroke_factor` 1.0), gira siguiendo la
//! direccion del trazo (alineacion `PATH`) y no lleva azar, papel ni dispersion.

use glam::Vec3;

use super::medio_seco::{DryDab, DryKind};
use super::punta::{Punta, TIP_SIZE};

/// Lee una punta guardada como texto: `TIP_SIZE` lineas de `TIP_SIZE` valores de dos cifras hexadecimales (00 = nada,
/// ff = pinta del todo), de arriba a abajo. Las lineas en blanco se ignoran.
pub(super) fn punta_desde_texto(texto: &str) -> Punta {
    let mut data = Vec::with_capacity(TIP_SIZE * TIP_SIZE);
    for linea in texto.lines().map(str::trim).filter(|l| !l.is_empty()) {
        for par in linea.as_bytes().chunks_exact(2) {
            let v = std::str::from_utf8(par)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .unwrap_or(0);
            data.push(f32::from(v) / 255.0);
        }
    }
    assert_eq!(
        data.len(),
        TIP_SIZE * TIP_SIZE,
        "la punta de Blender no mide {TIP_SIZE} x {TIP_SIZE}"
    );
    Punta::new(data)
}

/// Como gira la punta respecto de la direccion del trazo (el angulo del pincel de Blender con alineacion `PATH`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Giro {
    /// Angulo 0: la punta sube hacia donde va el trazo.
    Adelante,
    /// Angulo -90 grados: el ancho de la imagen queda de lado a lado del trazo y su arriba apunta hacia donde va el
    /// trazo (como un peine de puntas que se arrastra).
    DeLado,
}

/// Un toque de estos pinceles. La punta recortada de la imagen de Blender ocupa `ancho` x `alto` (fracciones del
/// diametro, centradas en el toque): se agranda para que llene el bitmap de la punta (`inv_radius / ancho` y redondez
/// `alto / ancho`). `along` es la direccion del trazo en el plano de la superficie (sin ella, el primer toque, usa la base
/// fija `t0`).
///
/// La orientacion se dedujo de como Blender estampa la textura (su eje x gira con el trazo y el angulo del pincel lo
/// vuelve a girar; el de arriba de la imagen es su eje y). No se comprobo contra un render de Blender: si salen
/// volteadas, basta con cambiar el signo de `b` aqui.
pub(super) fn nuevo(
    kind: DryKind,
    giro: Giro,
    (ancho, alto): (f32, f32),
    inv_radius: f32,
    t0: Vec3,
    n: Vec3,
    along: Option<Vec3>,
) -> DryDab {
    let a = along.unwrap_or(t0);
    // `t` va hacia la derecha de la imagen y `b` hacia abajo (las filas del bitmap bajan al crecer v).
    let (t, b) = match giro {
        Giro::Adelante => (a, a.cross(n)),
        Giro::DeLado => (a.cross(n), -a),
    };
    DryDab {
        kind,
        t,
        b,
        inv_radius: inv_radius / ancho,
        roundness: alto / ancho,
        shift: Vec3::ZERO,
        follaje: None,
    }
}

/// Las pruebas que comparten los pinceles de Blender: la punta es la del archivo de datos (`huella` = suma de los valores
/// de 0 a 255 y cantidad de texeles con pintura, para detectar cualquier cambio al copiar el archivo), y el toque no se
/// mueve, no llega mas alla de su recorte, pinta dentro de el y es siempre igual (sin azar).
#[cfg(test)]
pub(super) fn verificar(
    kind: DryKind,
    giro: Giro,
    (ancho, alto): (f32, f32),
    punta: &Punta,
    datos: &str,
    huella: (u32, usize),
) {
    assert_eq!(punta.data.len(), TIP_SIZE * TIP_SIZE);
    assert!(punta.data.iter().all(|v| (0.0..=1.0).contains(v)));
    let suma: u32 = punta.data.iter().map(|v| (v * 255.0).round() as u32).sum();
    let pintados = punta.data.iter().filter(|v| **v > 0.0).count();
    assert_eq!((suma, pintados), huella);
    assert_eq!(punta.data, punta_desde_texto(datos).data);

    let radio = 2.0;
    let d = DryDab::new(kind, 1, Vec3::Z, Vec3::X, radio);
    assert_eq!(d.shift, Vec3::ZERO);
    assert_eq!(d.reach(), 0.0);
    // Cuanto se extiende el recorte a lo largo de X y de Y, en radios (el trazo va por X).
    let (ex, ey) = match giro {
        Giro::Adelante => (ancho, alto),
        Giro::DeLado => (alto, ancho),
    };
    assert_eq!(d.alpha(Vec3::new((ex + 0.02) * radio, 0.0, 0.0)), 0.0);
    assert_eq!(d.alpha(Vec3::new(-(ex + 0.02) * radio, 0.0, 0.0)), 0.0);
    assert_eq!(d.alpha(Vec3::new(0.0, (ey + 0.02) * radio, 0.0)), 0.0);
    assert_eq!(d.alpha(Vec3::new(0.0, -(ey + 0.02) * radio, 0.0)), 0.0);
    // Dentro del recorte hay pintura repartida y siempre entre 0 y 1.
    let mut con_pintura = 0;
    for i in -20..=20 {
        for j in -20..=20 {
            let p = Vec3::new(
                i as f32 / 20.0 * 0.98 * ex * radio,
                j as f32 / 20.0 * 0.98 * ey * radio,
                0.0,
            );
            let a = d.alpha(p);
            assert!((0.0..=1.0).contains(&a));
            con_pintura += (a > 0.0) as u32;
        }
    }
    assert!(con_pintura > 300, "con pintura {con_pintura}");
    // Sin azar: todos los toques son iguales.
    let p = Vec3::new(0.1 * ex * radio, 0.3 * ey * radio, 0.0);
    for dab in 1..50 {
        let e = DryDab::new(kind, dab, Vec3::Z, Vec3::X, radio);
        assert_eq!(e.alpha(p), d.alpha(p));
    }
    // Sin direccion (primer toque) no falla.
    let sin = DryDab::new(kind, 1, Vec3::Z, Vec3::ZERO, radio);
    assert!((0.0..=1.0).contains(&sin.alpha(p)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_punta_se_lee_del_texto() {
        let mut texto = String::new();
        for y in 0..TIP_SIZE {
            for x in 0..TIP_SIZE {
                texto.push_str(if x == y { "ff" } else { "00" });
            }
            texto.push('\n');
        }
        // Una linea en blanco al final no cambia nada.
        texto.push('\n');
        let p = punta_desde_texto(&texto);
        assert_eq!(p.data.len(), TIP_SIZE * TIP_SIZE);
        assert_eq!(p.data[0], 1.0);
        assert_eq!(p.data[1], 0.0);
        assert_eq!(p.data[TIP_SIZE + 1], 1.0);
    }

    #[test]
    fn el_giro_orienta_la_punta_respecto_del_trazo() {
        let n = Vec3::Z;
        let a = Vec3::X;
        // De lado: el eje ancho (t) queda de lado a lado del trazo y el alto (b) a lo largo de el.
        let d = nuevo(DryKind::GpArc, Giro::DeLado, (0.8, 0.4), 0.5, Vec3::X, n, Some(a));
        assert!(d.t.dot(a).abs() < 1e-6 && d.b.dot(a).abs() > 0.999);
        // Adelante: el eje ancho va a lo largo del trazo.
        let d = nuevo(DryKind::GpDots, Giro::Adelante, (1.0, 1.0), 0.5, Vec3::X, n, Some(a));
        assert!(d.t.dot(a) > 0.999 && d.b.dot(a).abs() < 1e-6);
        // Sin direccion (primer toque) usa la base fija y no falla.
        let d = nuevo(DryKind::GpDots, Giro::DeLado, (1.0, 1.0), 0.5, Vec3::Y, n, None);
        assert!(d.t.is_finite() && d.b.is_finite());
    }

    #[test]
    fn el_recorte_se_agranda_para_llenar_el_bitmap() {
        // Con ancho 0.5 y alto 0.25 la mitad del diametro en X cae justo en el borde de la punta.
        let d = nuevo(DryKind::GpArc, Giro::Adelante, (0.5, 0.25), 1.0, Vec3::X, Vec3::Z, Some(Vec3::X));
        assert!((d.inv_radius - 2.0).abs() < 1e-6);
        assert!((d.roundness - 0.5).abs() < 1e-6);
    }
}
