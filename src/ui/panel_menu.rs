//! Panel del menu de la flecha (Como utilizar, Ajustes, Guardar imagen...).

use super::*;

/// Una fila del menu: nombre e icono (`None` = todavia sin icono; el hueco se deja igual). Para ponerle icono basta
/// con `icon: Some(egui::include_image!("../../assets/icons/archivo.svg"))`.
pub(super) struct MenuItem {
    label: &'static str,
    icon: Option<egui::ImageSource<'static>>,
}

/// Filas del menu, de arriba abajo. Por ahora al hacer clic solo se cierra el panel (todavia sin funcion).
pub(super) fn menu_items() -> [MenuItem; 4] {
    [
        MenuItem { label: "Cómo utilizar", icon: Some(egui::include_image!("../../assets/icons/menu_help.svg")) },
        MenuItem { label: "Ajustes", icon: Some(egui::include_image!("../../assets/icons/menu_settings.svg")) },
        MenuItem { label: "Guardar imagen", icon: Some(egui::include_image!("../../assets/icons/menu_save.svg")) },
        MenuItem { label: "Regresar a mi galería", icon: Some(egui::include_image!("../../assets/icons/menu_gallery.svg")) },
    ]
}

/// Panel flotante del boton del menu: aparece encima del boton, pegado a su borde derecho, y se cierra al hacer clic
/// fuera de el o en una fila.
pub(super) fn menu_panel(ctx: &egui::Context, options: &mut OptionsPanel, button: Rect) {
    if !options.menu_open {
        return;
    }
    #[allow(deprecated)]
    let id = egui::Id::new("menu_panel");
    let mut close = false;
    let out = egui::Area::new(id)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(egui::pos2(button.right(), button.top() - 10.0))
        .sense(Sense::click())
        .show(ctx, |ui| {
            // Marco de afuera = el borde (gris con la transparencia de la barra); adentro, el panel gris claro.
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(18))
                .inner_margin(Margin::same(MENU_BORDER as i8))
                .show(ui, |ui| {
                    Frame::new()
                        .fill(MENU_FILL)
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(0, 3))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            let items = menu_items();
                            let n = items.len();
                            for (i, item) in items.iter().enumerate() {
                                let (rect, response) =
                                    ui.allocate_exact_size(vec2(MENU_W, MENU_ROW_H), Sense::click());
                                let p = ui.painter();
                                // Hover de fila entera, como el del desplegable de modos de mezcla: de borde a borde y sin
                                // esquinas redondeadas (solo la primera y la ultima siguen la curva del marco).
                                if response.hovered() {
                                    let r = 7;
                                    let corners = CornerRadius {
                                        nw: if i == 0 { r } else { 0 },
                                        ne: if i == 0 { r } else { 0 },
                                        sw: if i + 1 == n { r } else { 0 },
                                        se: if i + 1 == n { r } else { 0 },
                                    };
                                    p.rect_filled(rect, corners, Color32::from_black_alpha(14));
                                }
                                if let Some(icon) = &item.icon {
                                    let icon_rect = Rect::from_center_size(
                                        egui::pos2(rect.left() + MENU_PAD + MENU_ICON * 0.5, rect.center().y),
                                        vec2(MENU_ICON, MENU_ICON),
                                    );
                                    egui::Image::new(icon.clone()).tint(MENU_TEXT).paint_at(ui, icon_rect);
                                }
                                ui.painter().text(
                                    egui::pos2(rect.left() + MENU_PAD + MENU_ICON + 10.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    item.label,
                                    FontId::proportional(MENU_TEXT_SIZE),
                                    MENU_TEXT,
                                );
                                if i + 1 < n {
                                    ui.painter().hline(
                                        rect.left() + 2.0..=rect.right() - 2.0,
                                        rect.bottom() - 0.5,
                                        Stroke::new(1.0, MENU_LINE),
                                    );
                                }
                                if response.clicked() {
                                    close = true;
                                }
                            }
                        });
                });
        });
    // Un clic fuera del panel (y fuera del boton, que ya lo abre/cierra) lo cierra.
    let panel = out.response.rect;
    let pressed_outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !panel.contains(p) && !button.contains(p));
    if close || pressed_outside {
        options.menu_open = false;
    }
}
