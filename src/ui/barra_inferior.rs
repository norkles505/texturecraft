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
pub(super) fn layers_button(
    ui: &mut egui::Ui,
    count: usize,
    open: bool,
    normal: Color32,
    icon: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(btn_size(icon), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            normal
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(icon, icon));
        egui::Image::new(egui::include_image!("../../assets/icons/layers.svg"))
            .tint(color)
            .paint_at(ui, icon_rect);
        // La hoja de enfrente del icono es el cuadrado de arriba a la izquierda; su centro esta al 41 % del icono.
        let label_pos = icon_rect.min + icon_rect.size() * 0.41;
        // El número escala con el icono (con el normal, 31, mide 13 o 10).
        let size = (if count >= 10 { 10.0 } else { 13.0 }) * icon / LAYERS_ICON;
        outlined_number(
            ui.painter(),
            label_pos,
            count.to_string(),
            FontId::proportional(size),
        );
    }
    response.on_hover_text("Capas")
}

/// Número negro con contorno blanco (el del cubo y el de capas), para que se lea sobre cualquier fondo.
fn outlined_number(p: &egui::Painter, pos: egui::Pos2, label: String, font: FontId) {
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
        p.text(
            pos + off,
            Align2::CENTER_CENTER,
            &label,
            font.clone(),
            Color32::WHITE,
        );
    }
    p.text(pos, Align2::CENTER_CENTER, &label, font, Color32::BLACK);
}

/// Boton del menu de la barra inferior (flecha hacia la izquierda, a la derecha del de capas). Abre el panel del menu;
/// se pinta de azul mientras ese panel esta abierto. Devuelve la respuesta para que la barra lo abra o cierre.
pub(super) fn menu_button(
    ui: &mut egui::Ui,
    open: bool,
    normal: Color32,
    icon: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(btn_size(icon), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            normal
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(icon, icon));
        egui::Image::new(egui::include_image!("../../assets/icons/undo.svg"))
            .tint(color)
            .paint_at(ui, icon_rect);
    }
    response.on_hover_text("Menu")
}

/// Boton de opciones de la barra inferior (a la izquierda del cubo): icono de dos interruptores. Se pinta de azul
/// mientras su panel esta abierto. Devuelve la respuesta para que la barra abra o cierre el panel.
pub(super) fn options_button(
    ui: &mut egui::Ui,
    open: bool,
    normal: Color32,
    icon: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(btn_size(icon), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            normal
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(icon, icon));
        egui::Image::new(egui::include_image!("../../assets/icons/toggles.svg"))
            .tint(color)
            .paint_at(ui, icon_rect);
    }
    response.on_hover_text("Opciones")
}

/// Boton del cubo de la barra inferior (a la izquierda del de capas): muestra cuantas mallas (partes) tiene el modelo.
/// Abre el panel de mallas; se pinta de azul mientras ese panel esta abierto.
pub(super) fn cube_button(
    ui: &mut egui::Ui,
    count: usize,
    open: bool,
    normal: Color32,
    icon: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(btn_size(icon), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            normal
        };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(icon, icon));
        egui::Image::new(egui::include_image!("../../assets/icons/cube.svg"))
            .tint(color)
            .paint_at(ui, icon_rect);
        // Numero en el centro del cubo, negro con contorno blanco para que se lea sobre las caras y sobre los huecos.
        let pos = icon_rect.min + icon_rect.size() * vec2(0.5, 0.56);
        let font =
            FontId::proportional((if count >= 10 { 9.0 } else { 11.0 }) * icon / LAYERS_ICON);
        outlined_number(ui.painter(), pos, count.to_string(), font);
    }
    response.on_hover_text("Mallas")
}

/// Anchura mínima de la barra: justo lo que ocupan sus cuatro botones con la separación normal (8) entre ellos, el
/// margen de 8 a cada lado y el borde. Más angosta que esto los botones no caben; más ancha deja un vacío.
pub(super) fn bar_min_width(border_w: f32, icon: f32) -> f32 {
    4.0 * btn_size(icon).x + 3.0 * 8.0 + 16.0 + 2.0 * border_w
}

/// Tamaño del botón para un icono de lado `icon`: con el icono normal (31) da los 48x36 de siempre y crece con él.
pub(super) fn btn_size(icon: f32) -> egui::Vec2 {
    vec2(
        icon + (LAYERS_BTN_W - LAYERS_ICON),
        icon + (LAYERS_BTN_H - LAYERS_ICON),
    )
}

/// Altura mínima de la barra: la de los botones más el borde arriba y abajo.
pub(super) fn bar_min_height(border_w: f32, icon: f32) -> f32 {
    btn_size(icon).y + 2.0 * border_w
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
    // Estilo de la barra: el del modo edición (por defecto, el de siempre).
    let style = options.bottom_bar;
    // Anchura de la barra: una fracción de la ventana (con un mínimo para que quepan los cuatro botones).
    let bar_w = (width * style.width_frac)
        .max(bar_min_width(style.border_w, style.icon_size))
        .min(width)
        .round();
    // Lo que mide por dentro: sin el margen de 8 a cada lado ni el borde.
    let inner_w = bar_w - 16.0 - 2.0 * style.border_w;
    // Color de los iconos de los botones (apagados; al pasar el mouse o con su panel abierto cambian como siempre).
    let icon_color = style.icon_color();
    // Posición de la barra: centrada abajo, calculada con el tamaño de ESTE cuadro. Con `anchor`, egui la colocaba con el
    // tamaño del cuadro anterior, y al cambiar la anchura o la altura (deslizadores del modo edición) la barra saltaba
    // un cuadro fuera de lugar y parpadeaba. El borde suma su grosor a cada lado; el margen vertical es 0.
    // La altura elegida es la TOTAL (borde incluido): subir el grosor del borde no agranda la barra, se come el interior.
    // Solo si ya no caben los botones (36 + borde arriba y abajo) la barra crece lo justo.
    let bar_h = style
        .height
        .max(bar_min_height(style.border_w, style.icon_size));
    let inner_h = bar_h - 2.0 * style.border_w;
    let screen = ui.ctx().content_rect();
    let bar_pos = egui::pos2(
        (screen.center().x - bar_w / 2.0).round(),
        (screen.bottom() - bar_h).round(),
    );
    // `constrain(false)`: por defecto el Area se "encierra" en la pantalla usando su tamaño del cuadro ANTERIOR. Al achicar
    // la barra (anchura o altura), ese tamaño viejo se salía de la pantalla y empujaba la barra un cuadro: parpadeo.
    // Rectángulo exacto de la barra: de aquí salen el fondo, los botones y el contorno azul del modo edición, así que
    // siempre coinciden (antes el contorno usaba el tamaño que devolvía el `Frame`, que podía diferir).
    let bar_rect = Rect::from_min_size(bar_pos, vec2(bar_w, bar_h));
    let _ = egui::Area::new(egui::Id::new("bottombar"))
        .fixed_pos(bar_pos)
        .constrain(false)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(bar_rect.size(), Sense::hover());
            ui.painter().rect(
                rect,
                style.corner(),
                style.fill(),
                style.stroke(),
                StrokeKind::Inside,
            );
            // Zona de los botones: dentro del borde y del margen de 8 a cada lado.
            let inner = Rect::from_min_size(
                rect.min + vec2(8.0 + style.border_w, style.border_w),
                vec2(inner_w, inner_h),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                // De derecha a izquierda: el primer elemento queda pegado al borde derecho. La fila mide justo lo
                // que la barra (ancho y alto fijos): si se le dejara lo disponible, que es lo que midio la barra el
                // cuadro anterior por estar anclada abajo, la barra nunca volveria a achicarse.
                ui.allocate_ui_with_layout(
                    vec2(inner_w, inner_h),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // Los botones se dibujan en el orden guardado (`style.order` va de izquierda a derecha; la fila
                        // es de derecha a izquierda, de ahí el `rev`).
                        for id in style.order.iter().rev().copied() {
                            match id {
                                BTN_MENU => {
                                    let m = menu_button(
                                        ui,
                                        options.menu_open,
                                        icon_color,
                                        style.icon_size,
                                    );
                                    if m.clicked() {
                                        options.menu_open = !options.menu_open;
                                        options.open = false;
                                        options.layers_open = false;
                                        options.meshes_open = false;
                                        options.blend_open = false;
                                    }
                                    menu_rect = Some(m.rect);
                                }
                                BTN_LAYERS => {
                                    let l = layers_button(
                                        ui,
                                        stack.layers.len(),
                                        options.layers_open,
                                        icon_color,
                                        style.icon_size,
                                    );
                                    if l.clicked() {
                                        options.layers_open = !options.layers_open;
                                        options.menu_open = false;
                                        options.open = false;
                                        options.meshes_open = false;
                                        options.blend_open = false;
                                    }
                                    layers_rect = Some(l.rect);
                                }
                                BTN_CUBE => {
                                    let c = cube_button(
                                        ui,
                                        parts.len(),
                                        options.meshes_open,
                                        icon_color,
                                        style.icon_size,
                                    );
                                    if c.clicked() {
                                        options.meshes_open = !options.meshes_open;
                                        options.menu_open = false;
                                        options.open = false;
                                        options.layers_open = false;
                                        options.blend_open = false;
                                    }
                                    mesh_rect = Some(c.rect);
                                }
                                BTN_OPTIONS => {
                                    let r = options_button(
                                        ui,
                                        options.open,
                                        icon_color,
                                        style.icon_size,
                                    );
                                    if r.clicked() {
                                        options.open = !options.open;
                                        options.menu_open = false;
                                        options.layers_open = false;
                                        options.meshes_open = false;
                                        options.blend_open = false;
                                    }
                                    options_rect = Some(r.rect);
                                }
                                _ => {}
                            }
                        }
                    },
                );
            });
        });
    // Modo edición: capa que elige la barra y panel con sus propiedades (debajo de las ventanas de más abajo).
    edit_bar(ui.ctx(), options, bar_rect);
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
