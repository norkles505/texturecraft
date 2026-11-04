//! Pincel TINTA RUGOSA (`BrushType::GpInk`): el "Ink Pen Rough.001" del paquete de Blender `GP_Brushes_3.blend` (Grease
//! Pencil), una version del pincel de tinta rugosa de Blender con el material GP_BRUSH_06. Su punta es una pincelada
//! de tinta con vetas y salpicaduras; gira con el trazo (angulo 0, alineacion `PATH`) y se estampa con mucho espacio
//! entre toques.
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 90 (radio 45), el espaciado 100 % del
//! diametro (2.0 del radio), la rotacion con el trazo y la fuerza 0.3757 (flujo inicial 0.3757).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 1458 x 1458 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`). La pincelada ocupa casi todo el cuadrado
//!   (0.999 del ancho y 0.97 del alto); las vetas finas y las salpicaduras pequenas salen blandas o se pierden.
//! - La orientacion exacta no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_tinta.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.9986;
const ALTO: f32 = 0.9726;

/// La punta de la tinta rugosa (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de tinta rugosa: la pincelada gira con el trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpInk, Giro::Adelante, (ANCHO, ALTO), inv_radius, t0, n, along)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::gp_comun::verificar;

    #[test]
    fn la_punta_y_el_toque_son_los_de_blender() {
        verificar(
            DryKind::GpInk,
            Giro::Adelante,
            (ANCHO, ALTO),
            punta(),
            DATOS,
            (414_153, 2233),
        );
    }
}
