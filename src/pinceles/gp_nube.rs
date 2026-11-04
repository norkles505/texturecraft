//! Pincel NUBE (`BrushType::GpCloud`): el GP_BRUSH_02 del paquete de Blender `GP_Brushes_3.blend` (Grease Pencil). Su
//! punta es una mancha difusa de borde deshilachado, como una nube de pintura, que queda de lado a lado del trazo
//! (angulo -90 grados, alineacion `PATH`).
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 132 (radio 66), el espaciado 38 % del
//! diametro (0.76 del radio), la rotacion con el trazo y la fuerza 1.0 (flujo 1).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 1024 x 1024 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`). Se recorta el espacio vacio (la nube ocupa
//!   0.93 del ancho y todo el alto), pero el borde deshilachado sale mas blando que el original.
//! - La orientacion exacta no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_nube.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.9258;
const ALTO: f32 = 1.0;

/// La punta de la nube (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de nube: la mancha queda de lado a lado del trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpCloud, Giro::DeLado, (ANCHO, ALTO), inv_radius, t0, n, along)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::gp_comun::verificar;

    #[test]
    fn la_punta_y_el_toque_son_los_de_blender() {
        verificar(
            DryKind::GpCloud,
            Giro::DeLado,
            (ANCHO, ALTO),
            punta(),
            DATOS,
            (357_270, 2122),
        );
    }
}
