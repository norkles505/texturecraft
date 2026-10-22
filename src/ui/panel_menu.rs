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
        MenuItem {
            label: "Cómo utilizar",
            icon: Some(egui::include_image!("../../assets/icons/menu_help.svg")),
        },
        MenuItem {
            label: "Ajustes",
            icon: Some(egui::include_image!("../../assets/icons/menu_settings.svg")),
        },
        MenuItem {
            label: "Guardar imagen",
            icon: Some(egui::include_image!("../../assets/icons/menu_save.svg")),
        },
        MenuItem {
            label: "Regresar a mi galería",
            icon: Some(egui::include_image!("../../assets/icons/menu_gallery.svg")),
        },
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
    let mut open_settings = false;
    let mut open_gallery = false;
    let mut save_image = false;
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
                                let (rect, response) = ui
                                    .allocate_exact_size(vec2(MENU_W, MENU_ROW_H), Sense::click());
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
                                        egui::pos2(
                                            rect.left() + MENU_PAD + MENU_ICON * 0.5,
                                            rect.center().y,
                                        ),
                                        vec2(MENU_ICON, MENU_ICON),
                                    );
                                    egui::Image::new(icon.clone())
                                        .tint(MENU_TEXT)
                                        .paint_at(ui, icon_rect);
                                }
                                ui.painter().text(
                                    egui::pos2(
                                        rect.left() + MENU_PAD + MENU_ICON + 10.0,
                                        rect.center().y,
                                    ),
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
                                    if item.label == "Ajustes" {
                                        open_settings = true;
                                    }
                                    if item.label == "Regresar a mi galería" {
                                        open_gallery = true;
                                    }
                                    if item.label == "Guardar imagen" {
                                        save_image = true;
                                    }
                                }
                            }
                        });
                });
        });
    // Un clic fuera del panel (y fuera del boton, que ya lo abre/cierra) lo cierra.
    let panel = out.response.rect;
    let pressed_outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !panel.contains(p) && !button.contains(p));
    if close || pressed_outside {
        options.menu_open = false;
    }
    if open_settings {
        options.settings_open = true;
    }
    if open_gallery {
        options.open_gallery = true;
    }
    if save_image {
        options.save_image = true;
    }
}

/// Panel de Ajustes: ventana centrada en la pantalla (sobre un fondo oscurecido que bloquea el resto de la interfaz).
/// Arriba el titulo y la palomita para cerrar; abajo el cuerpo oscuro con los ajustes. Tambien se cierra con Esc o con
/// un clic en el fondo oscurecido.
const SET_W: f32 = 320.0;
const SET_H: f32 = 440.0;
const SET_MARGIN: f32 = 8.0;
const SET_HEADER_H: f32 = 40.0;

pub(super) fn settings_panel(ctx: &egui::Context, options: &mut OptionsPanel) {
    if !options.settings_open {
        return;
    }
    // Si la ventana es chica, el panel se encoge para que siempre quepa.
    let screen = ctx.content_rect();
    let w = SET_W.min(screen.width() - 24.0);
    let h = SET_H.min(screen.height() - 24.0 - BOTTOM_BAR_H);
    let content = vec2(w - SET_MARGIN * 2.0, h - SET_MARGIN * 2.0);
    #[allow(deprecated)]
    let id = egui::Id::new("settings_panel");
    let mut close = false;
    let modal = egui::Modal::new(id)
        .backdrop_color(Color32::from_black_alpha(90))
        .frame(
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(18))
                .inner_margin(Margin::same(SET_MARGIN as i8)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            ui.set_min_size(content);
            // Cabecera: titulo a la izquierda (se dibuja dos veces con medio punto de diferencia para que parezca
            // negrita) y la palomita a la derecha.
            let (head, _) = ui.allocate_exact_size(vec2(content.x, SET_HEADER_H), Sense::hover());
            let title_pos = egui::pos2(head.left() + 10.0, head.center().y);
            for dx in [0.0, 0.6] {
                ui.painter().text(
                    title_pos + vec2(dx, 0.0),
                    Align2::LEFT_CENTER,
                    "Ajustes",
                    FontId::proportional(19.0),
                    Color32::WHITE,
                );
            }
            let ok_rect = Rect::from_center_size(
                egui::pos2(head.right() - 22.0, head.center().y),
                vec2(34.0, 34.0),
            );
            let ok = ui.interact(ok_rect, ui.id().with("settings_ok"), Sense::click());
            let color = if ok.hovered() {
                Color32::WHITE
            } else {
                Color32::from_gray(225)
            };
            let c = ok_rect.center();
            ui.painter().add(egui::Shape::line(
                vec![
                    c + vec2(-7.0, 0.5),
                    c + vec2(-2.5, 5.0),
                    c + vec2(7.0, -5.5),
                ],
                Stroke::new(2.0, color),
            ));
            if ok.clicked() {
                close = true;
            }
            // Cuerpo: aqui van los ajustes. Cada fila lleva el nombre a la izquierda y el interruptor a la derecha.
            Frame::new()
                .fill(Color32::from_black_alpha(50))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    ui.set_min_size(vec2(content.x - 20.0, content.y - SET_HEADER_H - 20.0));
                    let row_w = content.x - 20.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let (label, _) = ui
                            .allocate_exact_size(vec2(row_w - TOGGLE_W, TOGGLE_H), Sense::hover());
                        ui.painter().text(
                            label.left_center(),
                            Align2::LEFT_CENTER,
                            "Modo edición",
                            FontId::proportional(15.0),
                            Color32::WHITE,
                        );
                        toggle_switch(ui, &mut options.edit_mode);
                    });
                });
        });
    if close || modal.should_close() {
        options.settings_open = false;
    }
}
