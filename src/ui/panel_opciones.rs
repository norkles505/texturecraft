//! Panel de opciones (interruptores) que abre el boton de la barra inferior.

use super::*;

/// Tamano del interruptor del panel de opciones.
pub(super) const TOGGLE_W: f32 = 46.0;
pub(super) const TOGGLE_H: f32 = 26.0;

/// Interruptor: pastilla con perilla blanca. Apagado = gris con la perilla a la izquierda; encendido = azul con la
/// perilla a la derecha (la perilla se desliza con una pequena animacion).
pub(super) fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(TOGGLE_W, TOGGLE_H), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let p = ui.painter();
    let track = Color32::from_gray(150).lerp_to_gamma(ACCENT, t);
    p.rect_filled(rect, CornerRadius::same((TOGGLE_H / 2.0) as u8), track);
    let r = TOGGLE_H / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + TOGGLE_H / 2.0)..=(rect.right() - TOGGLE_H / 2.0), t);
    p.circle_filled(egui::pos2(x, rect.center().y), r, Color32::WHITE);
}

/// Ancho de la etiqueta de cada fila del panel de opciones: asi todos los interruptores quedan alineados a la derecha.
const OPTION_LABEL_W: f32 = 170.0;

/// Una fila del panel de opciones: el nombre a la izquierda y el interruptor a la derecha.
fn option_row(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 28.0;
        let (rect, _) = ui.allocate_exact_size(vec2(OPTION_LABEL_W, TOGGLE_H), Sense::hover());
        ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, label, FontId::proportional(15.0), Color32::WHITE);
        toggle_switch(ui, on);
    });
}

/// Panel flotante del boton de opciones: aparece justo encima del boton y se cierra al hacer clic fuera de el.
pub(super) fn options_panel(ctx: &egui::Context, options: &mut OptionsPanel, button: Rect) {
    if !options.open {
        return;
    }
    #[allow(deprecated)]
    let id = egui::Id::new("options_panel");
    let out = egui::Area::new(id)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(button.center().x, button.top() - 10.0))
        .sense(Sense::click())
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 12.0;
                    option_row(ui, "Ventana de referencia", &mut options.reference_window);
                    // "Acumular pasada": encendido (por defecto) = volver a pasar por el mismo sitio suma, como
                    // siempre; apagado = el trazo no pasa de la opacidad, como en photocraft (`DabOptions::ceiling`).
                    let mut accumulate = !options.pass_ceiling;
                    option_row(ui, "Acumular pasada", &mut accumulate);
                    options.pass_ceiling = !accumulate;
                });
        });
    // Un clic fuera del panel (y fuera del boton, que ya lo abre/cierra) lo cierra.
    let panel = out.response.rect;
    let pressed_outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !panel.contains(p) && !button.contains(p));
    if pressed_outside {
        options.open = false;
    }
}
