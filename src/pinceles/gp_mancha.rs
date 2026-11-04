//! Pincel MANCHA (`BrushType::GpBlot`): el GP_BRUSH_08 del paquete de Blender `GP_Brushes_3.blend` (Grease Pencil). Su
//! punta es una mancha redondeada con grano y huecos, pensada para estamparse: el espaciado es mayor que el tamano, asi
//! que deja manchas sueltas a lo largo del trazo. Gira con el trazo (angulo 0, alineacion `PATH`).
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 52 (radio 26), el espaciado 122 % del
//! diametro (2.44 del radio), la rotacion con el trazo y la fuerza 1.0 (flujo 1).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 512 x 512 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`). Se recorta el espacio vacio (la mancha ocupa
//!   0.86 del ancho y 0.80 del alto), pero el grano fino sale mas blando que el original.
//! - La orientacion exacta no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_mancha.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.8633;
const ALTO: f32 = 0.8008;

/// La punta de la mancha (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de mancha: la mancha gira con el trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpBlot, Giro::Adelante, (ANCHO, ALTO), inv_radius, t0, n, along)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::gp_comun::verificar;

    #[test]
    fn la_punta_y_el_toque_son_los_de_blender() {
        verificar(
            DryKind::GpBlot,
            Giro::Adelante,
            (ANCHO, ALTO),
            punta(),
            DATOS,
            (474_766, 2561),
        );
    }
}
