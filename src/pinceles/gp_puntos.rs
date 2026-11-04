//! Pincel TRAMA DE PUNTOS (`BrushType::GpDots`): el GP_BRUSH_07 del paquete de Blender `GP_Brushes_3.blend` (Grease
//! Pencil). Su punta es una trama cuadrada de puntitos de tamanos distintos, estampada con muy poco espacio entre
//! toques; gira con el trazo (angulo 0, alineacion `PATH`).
//!
//! Lo que copia (leido del .blend): la textura (su canal de opacidad), el tamano 134 (radio 67), el espaciado 6 % del
//! diametro (0.12 del radio), la rotacion con el trazo y la fuerza 1.0 (flujo 1).
//!
//! Lo que NO copia:
//! - La presion del lapiz (en Blender el tamano la sigue): aqui solo hay mouse.
//! - El color de la textura: Blender lo mezcla al 100 % con el del trazo, asi que solo importa la opacidad.
//! - Los 1024 x 1024 px de la imagen: la punta mide 64 x 64 (`TIP_SIZE`). Al reducirla, los puntos de la trama quedan de
//!   apenas 1 o 2 px y salen mas tenues y blandos que en Blender. Si se necesita mas fidelidad hay que subir
//!   `TIP_SIZE`, que es comun a todos los pinceles.
//! - La orientacion exacta de la trama no se comprobo contra un render de Blender.

use std::sync::OnceLock;

use glam::Vec3;

use super::gp_comun::{Giro, nuevo as toque, punta_desde_texto};
use super::medio_seco::{DryDab, DryKind};
use super::punta::Punta;

/// La punta guardada (64 lineas de 64 valores hexadecimales).
const DATOS: &str = include_str!("../../assets/pinceles/gp_puntos.txt");
/// Lo que ocupa la imagen recortada, como fraccion del diametro (centrada en el toque).
const ANCHO: f32 = 0.9902;
const ALTO: f32 = 0.9668;

/// La punta de la trama de puntos (se genera una sola vez).
pub(super) fn punta() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punta_desde_texto(DATOS))
}

/// Un toque de trama de puntos: la trama gira con el trazo y siempre es igual (sin azar).
pub(super) fn nuevo(inv_radius: f32, t0: Vec3, n: Vec3, along: Option<Vec3>) -> DryDab {
    toque(DryKind::GpDots, Giro::Adelante, (ANCHO, ALTO), inv_radius, t0, n, along)
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
        assert_eq!(huella(a), (75_908, 1196));
        assert_eq!(a.data, punta_desde_texto(DATOS).data);
        // Es una trama de puntos sueltos: poca pintura repartida por toda la punta, con huecos entre los puntos.
        let n = TIP_SIZE;
        let pintados = a.data.iter().filter(|v| **v > 0.0).count();
        assert!(pintados < n * n / 2, "pintados {pintados}");
        for (x0, y0) in [(0, 0), (32, 0), (0, 32), (32, 32)] {
            let cuarto = (y0..y0 + 32)
                .flat_map(|y| (x0..x0 + 32).map(move |x| a.data[y * n + x]))
                .filter(|v| *v > 0.0)
                .count();
            assert!(cuarto > 100, "cuarto ({x0},{y0}) tiene {cuarto}");
        }
    }

    #[test]
    fn el_toque_cubre_el_cuadrado_del_radio_y_siempre_es_igual() {
        let radio = 2.0;
        let d = DryDab::new(DryKind::GpDots, 1, Vec3::Z, Vec3::X, radio);
        assert_eq!(d.shift, Vec3::ZERO);
        assert_eq!(d.reach(), 0.0);
        // Nada mas alla del cuadrado del radio.
        assert_eq!(d.alpha(Vec3::new(1.01 * radio, 0.0, 0.0)), 0.0);
        assert_eq!(d.alpha(Vec3::new(0.0, 1.01 * radio, 0.0)), 0.0);
        // Dentro hay pintura repartida por todo el cuadrado.
        let mut pintados = 0;
        for i in -20..=20 {
            for j in -20..=20 {
                let a = d.alpha(Vec3::new(i as f32 * 0.05 * radio, j as f32 * 0.05 * radio, 0.0));
                assert!((0.0..=1.0).contains(&a));
                pintados += (a > 0.0) as u32;
            }
        }
        assert!(pintados > 100, "pintados {pintados}");
        // Sin azar: todos los toques son iguales.
        let p = Vec3::new(0.1, 0.3, 0.0);
        for dab in 1..50 {
            let e = DryDab::new(DryKind::GpDots, dab, Vec3::Z, Vec3::X, radio);
            assert_eq!(e.alpha(p), d.alpha(p));
        }
        // Sin direccion (primer toque) no falla.
        let sin = DryDab::new(DryKind::GpDots, 1, Vec3::Z, Vec3::ZERO, radio);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
    }
}
