//! Pincel GOUACHE (`BrushType::GpGouache`): el GP_BRUSH_04 del paquete de Blender `GP_Brushes_3.blend` (Grease Pencil). Su
//! punta es una salpicadura de pintura espesa con vetas y huecos, como un golpe de brocha de gouache; gira con el trazo
//! (angulo 0, alineacion `PATH`). En el .blend usa el material GP_BRUSH_05, cuya imagen es la que se copio.
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 60 (radio 30), el espaciado 6 % del
//! diametro (0.12 del radio), la rotacion con el trazo y la fuerza 1.0 (flujo 1).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 2048 x 2048 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`), una reduccion de 32 veces. Se recorta el
//!   espacio vacio (la mancha ocupa 0.94 del ancho y todo el alto), pero las vetas finas y los huecos pequenos se
//!   pierden o salen blandos.
//! - La orientacion exacta no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_gouache.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.9355;
const ALTO: f32 = 1.0;

/// La punta del gouache (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de gouache: la mancha gira con el trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpGouache, Giro::Adelante, (ANCHO, ALTO), inv_radius, t0, n, along)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::gp_comun::verificar;

    #[test]
    fn la_punta_y_el_toque_son_los_de_blender() {
        verificar(
            DryKind::GpGouache,
            Giro::Adelante,
            (ANCHO, ALTO),
            punta(),
            DATOS,
            (230_283, 1354),
        );
    }
}
