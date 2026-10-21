//! Barra inferior: botones de opciones, cubo (mallas), capas y menu.

use super::*;

/// Alto de la barra inferior.
pub(super) const BOTTOM_BAR_H: f32 = 40.0;
/// Boton de capas de la barra inferior: tamano del boton y del icono.
pub(super) const LAYERS_BTN_W: f32 = 48.0;
pub(super) const LAYERS_BTN_H: f32 = 36.0;
pub(super) const LAYERS_ICON: f32 = 31.0;
/// Boton de capas de la barra inferior: el icono de hojas apiladas con el numero de capas sobre la hoja de enfrente.
/// Abre el panel de capas; se pinta de azul mientras ese panel esta abierto.
pub(super) fn layers_button(ui: &mut egui::Ui, count: usize, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(LAYERS_BTN_W, LAYERS_BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            Color32::from_gray(205)
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(LAYERS_ICON, LAYERS_ICON));
        egui::Image::new(egui::include_image!("../../assets/icons/layers.svg")).tint(color).paint_at(ui, icon_rect);
        // La hoja de enfrente del icono es el cuadrado de arriba a la izquierda; su centro esta al 41 % del icono.
        let label_pos = icon_rect.min + icon_rect.size() * 0.41;
        let size = if count >= 10 { 10.0 } else { 13.0 };
        ui.painter().text(
            label_pos,
            Align2::CENTER_CENTER,
            count.to_string(),
            FontId::proportional(size),
            Color32::from_gray(30),
        );
    }
    response.on_hover_text("Capas")
}

/// Boton del menu de la barra inferior (flecha hacia la izquierda, a la derecha del de capas). Abre el panel del menu;
/// se pinta de azul mientras ese panel esta abierto. Devuelve la respuesta para que la barra lo abra o cierre.
pub(super) fn menu_button(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(LAYERS_BTN_W, LAYERS_BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            Color32::from_gray(205)
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(LAYERS_ICON, LAYERS_ICON));
        egui::Image::new(egui::include_image!("../../assets/icons/undo.svg")).tint(color).paint_at(ui, icon_rect);
    }
    response.on_hover_text("Menu")
}

/// Boton de opciones de la barra inferior (a la izquierda del cubo): icono de dos interruptores. Se pinta de azul
/// mientras su panel esta abierto. Devuelve la respuesta para que la barra abra o cierre el panel.
pub(super) fn options_button(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(LAYERS_BTN_W, LAYERS_BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            Color32::from_gray(205)
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(LAYERS_ICON, LAYERS_ICON));
        egui::Image::new(egui::include_image!("../../assets/icons/toggles.svg")).tint(color).paint_at(ui, icon_rect);
    }
    response.on_hover_text("Opciones")
}

/// Boton del cubo de la barra inferior (a la izquierda del de capas): muestra cuantas mallas (partes) tiene el modelo.
/// Abre el panel de mallas; se pinta de azul mientras ese panel esta abierto.
pub(super) fn cube_button(ui: &mut egui::Ui, count: usize, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(LAYERS_BTN_W, LAYERS_BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            Color32::from_gray(205)
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(LAYERS_ICON, LAYERS_ICON));
        egui::Image::new(egui::include_image!("../../assets/icons/cube.svg")).tint(color).paint_at(ui, icon_rect);
        // Numero en el centro del cubo, negro con contorno blanco para que se lea sobre las caras y sobre los huecos.
        let pos = icon_rect.min + icon_rect.size() * vec2(0.5, 0.56);
        let font = FontId::proportional(if count >= 10 { 9.0 } else { 11.0 });
        let label = count.to_string();
        let p = ui.painter();
        for off in [
            vec2(-1.0, 0.0),
            vec2(1.0, 0.0),
            vec2(0.0, -1.0),
            vec2(0.0, 1.0),
            vec2(-1.0, -1.0),
            vec2(1.0, -1.0),
            vec2(-1.0, 1.0),
            vec2(1.0, 1.0),
        ] {
            p.text(pos + off, Align2::CENTER_CENTER, &label, font.clone(), Color32::WHITE);
        }
        p.text(pos, Align2::CENTER_CENTER, &label, font, Color32::BLACK);
    }
    response.on_hover_text("Mallas")
}

/// Barra inferior: ocupa todo el ancho de la ventana, pegada abajo, con el mismo estilo que la barra de herramientas.
/// Su contenido va dentro del `with_layout` de abajo (a la derecha: capas y, a su izquierda, el cubo).
pub(super) fn bottom_bar(
    ui: &mut egui::Ui,
    stack: &mut LayerStack,
    parts: &mut [MeshPart],
    selected_part: &mut usize,
    options: &mut OptionsPanel,
) -> (bool, bool) {
    // Si el selector de archivos del boton Importar ya devolvio algo, queda listo para `UiLayer::take_import`.
    poll_mesh_import(ui.ctx(), options);
    let width = ui.ctx().content_rect().width();
    // Donde quedo el boton de opciones: el panel se abre justo encima de el.
    let mut options_rect = None;
    // Lo mismo para el boton del menu (la flecha) y el de capas.
    let mut menu_rect = None;
    let mut layers_rect = None;
    let mut mesh_rect = None;
    egui::Area::new(egui::Id::new("bottombar"))
        .anchor(Align2::LEFT_BOTTOM, vec2(0.0, 0.0))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                .inner_margin(Margin::symmetric(8, 0))
                .show(ui, |ui| {
                    // El borde suma 1 px a cada lado y el margen 8, por eso se restan 18 para ocupar justo el ancho.
                    ui.set_min_size(vec2(width - 18.0, BOTTOM_BAR_H));
                    // De derecha a izquierda: el primer elemento queda pegado al borde derecho.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let m = menu_button(ui, options.menu_open);
                        if m.clicked() {
                            options.menu_open = !options.menu_open;
                            options.open = false;
                            options.layers_open = false;
                            options.meshes_open = false;
                            options.blend_open = false;
                        }
                        menu_rect = Some(m.rect);
                        let l = layers_button(ui, stack.layers.len(), options.layers_open);
                        if l.clicked() {
                            options.layers_open = !options.layers_open;
                            options.menu_open = false;
                            options.open = false;
                            options.meshes_open = false;
                            options.blend_open = false;
                        }
                        layers_rect = Some(l.rect);
                        let c = cube_button(ui, parts.len(), options.meshes_open);
                        if c.clicked() {
                            options.meshes_open = !options.meshes_open;
                            options.menu_open = false;
                            options.open = false;
                            options.layers_open = false;
                            options.blend_open = false;
                        }
                        mesh_rect = Some(c.rect);
                        let r = options_button(ui, options.open);
                        if r.clicked() {
                            options.open = !options.open;
                            options.menu_open = false;
                            options.layers_open = false;
                            options.meshes_open = false;
                            options.blend_open = false;
                        }
                        options_rect = Some(r.rect);
                    });
                });
        });
    if let Some(button) = options_rect {
        options_panel(ui.ctx(), options, button);
    }
    if let Some(button) = menu_rect {
        menu_panel(ui.ctx(), options, button);
    }
    // Ventana de Ajustes (centrada; no depende de ningun boton).
    settings_panel(ui.ctx(), options);
    // El panel de mallas tambien cambia el fondo de la pieza activa (recompone su textura, igual que el de capas).
    let (parts_dirty, mesh_bg_dirty) = match mesh_rect {
        Some(button) => meshes_panel(ui.ctx(), options, parts, selected_part, stack, button),
        None => (false, false),
    };
    let layers_dirty = match layers_rect {
        Some(button) => layers_panel(ui.ctx(), options, stack, button),
        None => false,
    } || mesh_bg_dirty;
    (layers_dirty, parts_dirty)
}
