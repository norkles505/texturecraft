//! Cursor propio sobre el modelo: circulo del pincel y chincheta del cuentagotas.

use super::*;

/// Cursor propio que sigue al raton sobre el modelo (como en Ibis Paint). `pos` y `radius` van en pixeles fisicos de la
/// ventana; `on_model` = el cursor esta sobre el modelo (si no, el circulo se ve mas tenue con el ultimo tamano conocido).
/// `pin` = en vez del circulo del pincel, la chincheta del cuentagotas.
#[derive(Clone, Copy)]
pub struct BrushRing {
    pub pos: [f32; 2],
    pub radius: f32,
    pub on_model: bool,
    pub pin: bool,
    /// Solo con `pin`: color que hay bajo la punta, para rellenar el aro (`None` = aro hueco).
    pub color: Option<[u8; 3]>,
}

/// Cursor del cuentagotas: un aro con una punta hacia abajo, como una chincheta de mapa. La punta es el pixel
/// exacto que se toma (cae justo en `tip`) y el aro se rellena con el color que hay bajo la punta, o queda hueco si
/// ahi no hay nada que tomar. Oculta la flecha del sistema.
pub(super) fn eyedropper_pin(ctx: &egui::Context, tip: egui::Pos2, fill: Option<[u8; 3]>) {
    let r = 14.0;
    let center = tip + vec2(0.0, -(r + 9.0));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("eyedropper_pin"),
    ));
    let light = Color32::from_gray(235);
    let dark = Color32::from_black_alpha(170);
    // La punta es un triangulo (se dibuja con bordes suavizados, a diferencia de una malla suelta). Su lado de arriba
    // queda DENTRO del grosor del aro (nunca asoma al hueco), asi que el aro sigue hueco o con su color limpio.
    let half = 7.0;
    let top_y = r - 0.6;
    let tri = |dy: f32, grow: f32| {
        vec![
            tip + vec2(0.0, grow),
            center + vec2(half + grow, top_y + dy),
            center + vec2(-(half + grow), top_y + dy),
        ]
    };
    // Contorno oscuro (para que se vea sobre colores claros), solo por FUERA del aro y de la punta. Va primero; luego
    // el relleno de color y al final lo claro encima.
    painter.circle_stroke(center, r + 1.4, Stroke::new(2.2, dark));
    painter.add(egui::Shape::convex_polygon(
        tri(0.6, 1.5),
        dark,
        Stroke::NONE,
    ));
    // Relleno del aro con el color de abajo.
    if let Some(c) = fill {
        painter.circle_filled(center, r, Color32::from_rgb(c[0], c[1], c[2]));
    }
    painter.circle_stroke(center, r, Stroke::new(2.4, light));
    painter.add(egui::Shape::convex_polygon(
        tri(0.0, 0.0),
        light,
        Stroke::NONE,
    ));
    ctx.set_cursor_icon(egui::CursorIcon::None);
}

/// Dibuja el cursor propio sobre el modelo: el circulo del pincel (aro negro con aro blanco encima, para que se vea
/// sobre cualquier color) o la chincheta del cuentagotas, y oculta la flecha. Si el cursor esta sobre la interfaz
/// (barra, paneles, ventana de referencia) no hace nada.
pub(super) fn brush_ring(ctx: &egui::Context, ring: BrushRing) {
    if ctx.egui_wants_pointer_input() {
        return;
    }
    let ppp = ctx.pixels_per_point();
    let center = egui::pos2(ring.pos[0] / ppp, ring.pos[1] / ppp);
    if ring.pin {
        eyedropper_pin(ctx, center, ring.color);
        return;
    }
    let radius = ring.radius / ppp;
    let alpha = if ring.on_model { 255 } else { 110 };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("brush_ring"),
    ));
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(3.0, Color32::from_black_alpha(alpha / 2)),
    );
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.5, Color32::from_white_alpha(alpha)),
    );
    // Puntito en el centro, para saber exactamente donde cae el toque.
    painter.circle_filled(center, 2.0, Color32::from_black_alpha(alpha / 2));
    painter.circle_filled(center, 1.0, Color32::from_white_alpha(alpha));
    ctx.set_cursor_icon(egui::CursorIcon::None);
}
