//! Pincel ASTILLAS (`BrushType::GpShards`): el GP_BRUSH_01 del paquete de Blender `GP_Brushes_3.blend` (Grease Pencil).
//! Su punta es una fila de cuatro astillas triangulares, largas y finas, que quedan de lado a lado del trazo (angulo
//! -90 grados, alineacion `PATH`).
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 52 (radio 26), el espaciado 7 % del
//! diametro (0.14 del radio), la rotacion con el trazo y la fuerza 1.0 (flujo 1).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 1024 x 1024 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`). Se recorta el espacio vacio de la imagen (la
//!   fila de astillas ocupa 0.82 del ancho y 0.17 del alto) para no perder detalle, pero el borde de cada astilla es
//!   mas blando que el original.
//! - La orientacion exacta (arriba o abajo de cada astilla) no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_astillas.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.8203;
const ALTO: f32 = 0.1738;

/// La punta de las astillas (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de astillas: la fila queda de lado a lado del trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpShards, Giro::DeLado, (ANCHO, ALTO), inv_radius, t0, n, along)
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
        assert_eq!(huella(a), (347_930, 1649));
        assert_eq!(a.data, punta_desde_texto(DATOS).data);
        // Cuatro astillas separadas: en una fila del medio hay cuatro tramos pintados.
        let fila = &a.data[20 * TIP_SIZE..21 * TIP_SIZE];
        let tramos = fila
            .windows(2)
            .filter(|w| w[0] <= 0.5 && w[1] > 0.5)
            .count();
        assert_eq!(tramos, 4, "tramos {tramos}");
        // Las esquinas quedan vacias.
        for i in [0, TIP_SIZE - 1, TIP_SIZE * (TIP_SIZE - 1), TIP_SIZE * TIP_SIZE - 1] {
            assert_eq!(a.data[i], 0.0, "esquina {i}");
        }
    }

    #[test]
    fn el_toque_es_una_fila_ancha_de_lado_a_lado_del_trazo_y_siempre_igual() {
        let radio = 2.0;
        // El trazo va por X: la fila es ancha en Y y baja en X.
        let d = DryDab::new(DryKind::GpShards, 1, Vec3::Z, Vec3::X, radio);
        assert_eq!(d.shift, Vec3::ZERO);
        assert_eq!(d.reach(), 0.0);
        // Pinta de lado a lado (hasta 0.82 del radio) y casi nada a lo largo del trazo (0.17 del radio).
        let lado: f32 = (-20..=20)
            .map(|i| d.alpha(Vec3::new(0.0, i as f32 * 0.04 * radio, 0.0)))
            .sum();
        let largo: f32 = (-20..=20)
            .map(|i| d.alpha(Vec3::new(i as f32 * 0.04 * radio, 0.0, 0.0)))
            .sum();
        assert!(lado > 8.0 && lado > largo, "lado {lado}, largo {largo}");
        assert_eq!(d.alpha(Vec3::new(0.0, 0.9 * radio, 0.0)), 0.0);
        assert_eq!(d.alpha(Vec3::new(0.3 * radio, 0.0, 0.0)), 0.0);
        // Sin azar: todos los toques son iguales.
        let p = Vec3::new(0.1, 0.3, 0.0);
        for dab in 1..50 {
            let e = DryDab::new(DryKind::GpShards, dab, Vec3::Z, Vec3::X, radio);
            assert_eq!(e.alpha(p), d.alpha(p));
        }
        // Con el trazo por Y se gira: ahora es ancha en X.
        let y = DryDab::new(DryKind::GpShards, 1, Vec3::Z, Vec3::Y, radio);
        let lado_y: f32 = (-20..=20)
            .map(|i| y.alpha(Vec3::new(i as f32 * 0.04 * radio, 0.0, 0.0)))
            .sum();
        assert!(lado_y > 8.0, "lado_y {lado_y}");
        // Sin direccion (primer toque) no falla.
        let sin = DryDab::new(DryKind::GpShards, 1, Vec3::Z, Vec3::ZERO, radio);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
    }
}
