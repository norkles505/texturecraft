//! Pincel ARCO (`BrushType::GpArc`): el GP_BRUSH_03 del paquete de Blender `GP_Brushes_3.blend` (Grease Pencil). Su punta
//! es una pincelada curva, como un arco de borde irregular, que queda de lado a lado del trazo (angulo -90 grados,
//! alineacion `PATH`).
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 82 (radio 41), el espaciado 11 % del
//! diametro (0.22 del radio), la rotacion con el trazo y la fuerza 1.0 (flujo 1).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 1024 x 1024 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`). Se recorta el espacio vacio de la imagen (el
//!   arco ocupa 0.85 del ancho y 0.44 del alto) para no perder detalle, pero el borde irregular es mas blando que el
//!   original.
//! - La orientacion exacta (hacia donde abre el arco) no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_arco.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.8457;
const ALTO: f32 = 0.4355;

/// La punta del arco (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de arco: la pincelada queda de lado a lado del trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpArc, Giro::DeLado, (ANCHO, ALTO), inv_radius, t0, n, along)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::punta::TIP_SIZE;

    /// Suma de los valores (0..255) y cantidad de texeles con pintura de la punta original: si cambia algo al copiar el
    /// archivo de datos, estos numeros no coinciden.
    fn huella(p: &Punta) -> (u32, usize) {
        let suma = p.data.iter().map(|v| (v * 255.0).round() as u32).sum();
        (suma, p.data.iter().filter(|v| **v > 0.0).count())
    }

    #[test]
    fn la_punta_es_la_de_blender_y_es_determinista() {
        let a = punta();
        assert_eq!(a.data.len(), TIP_SIZE * TIP_SIZE);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        assert_eq!(huella(a), (213_229, 1004));
        assert_eq!(a.data, punta_desde_texto(DATOS).data);
        // Es un arco: el centro de la punta queda vacio y arriba hay pintura en todo lo ancho.
        let n = TIP_SIZE;
        assert_eq!(a.data[(n / 2 + 8) * n + n / 2], 0.0);
        let arriba = a.data[8 * n..12 * n].iter().filter(|v| **v > 0.5).count();
        assert!(arriba > 80, "arriba {arriba}");
        // Las esquinas quedan vacias.
        for i in [0, n - 1, n * (n - 1), n * n - 1] {
            assert_eq!(a.data[i], 0.0, "esquina {i}");
        }
    }

    #[test]
    fn el_toque_es_ancho_de_lado_a_lado_del_trazo_y_siempre_igual() {
        let radio = 2.0;
        // El trazo va por X: el arco es ancho en Y.
        let d = DryDab::new(DryKind::GpArc, 1, Vec3::Z, Vec3::X, radio);
        assert_eq!(d.shift, Vec3::ZERO);
        assert_eq!(d.reach(), 0.0);
        // Nada mas alla del ancho (0.85 del radio) ni del alto (0.44 del radio).
        assert_eq!(d.alpha(Vec3::new(0.0, 0.9 * radio, 0.0)), 0.0);
        assert_eq!(d.alpha(Vec3::new(0.5 * radio, 0.0, 0.0)), 0.0);
        // Pinta algo de lado a lado.
        let lado: f32 = (-21..=21)
            .map(|i| d.alpha(Vec3::new(0.0, i as f32 * 0.04 * radio, 0.0)))
            .sum();
        assert!(lado > 3.0, "lado {lado}");
        // Sin azar: todos los toques son iguales.
        let p = Vec3::new(0.1, 0.3, 0.0);
        for dab in 1..50 {
            let e = DryDab::new(DryKind::GpArc, dab, Vec3::Z, Vec3::X, radio);
            assert_eq!(e.alpha(p), d.alpha(p));
        }
        // Sin direccion (primer toque) no falla.
        let sin = DryDab::new(DryKind::GpArc, 1, Vec3::Z, Vec3::ZERO, radio);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
    }
}
