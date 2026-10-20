//! Estilo compartido de la interfaz: colores, transparencia de las barras y medidas del menu desplegable.

use super::*;

/// Azul del boton activo (como el "Brush" de la referencia).
pub(super) const ACCENT: Color32 = Color32::from_rgb(33, 150, 243);
/// Opacidad del fondo de las barras (0 = invisible, 255 = solido).
pub(super) const BAR_ALPHA: u8 = 150;
/// Fondo de las barras: gris oscuro semitransparente.
pub(super) fn bar_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(58, 58, 60, BAR_ALPHA)
}
/// Panel del menu: fondo gris claro con filas (icono + nombre) separadas por lineas, como el menu de Ibis Paint, y un
/// borde gris con la misma transparencia que las barras (`BAR_ALPHA`).
/// Ancho y alto de cada fila, margen izquierdo, lado del icono, tamano del texto, grosor del borde y colores.
pub(super) const MENU_W: f32 = 200.0;
pub(super) const MENU_ROW_H: f32 = 32.0;
pub(super) const MENU_PAD: f32 = 12.0;
pub(super) const MENU_ICON: f32 = 17.0;
pub(super) const MENU_TEXT_SIZE: f32 = 13.0;
pub(super) const MENU_BORDER: f32 = 8.0;
pub(super) const MENU_FILL: Color32 = Color32::from_gray(232);
pub(super) const MENU_TEXT: Color32 = Color32::from_gray(25);
pub(super) const MENU_LINE: Color32 = Color32::from_gray(175);
/// Borde del panel: el gris de la referencia con la transparencia de la barra.
pub(super) fn menu_border() -> Color32 {
    Color32::from_rgba_unmultiplied(115, 115, 117, BAR_ALPHA)
}
