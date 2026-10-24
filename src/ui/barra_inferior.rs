//! Barra inferior: botones de opciones, cubo (mallas), capas y menu.

use super::*;

/// Alto de la barra inferior.
pub(super) const BOTTOM_BAR_H: f32 = 40.0;
/// Boton de capas de la barra inferior: tamano del boton y del icono.
pub(super) const LAYERS_BTN_W: f32 = 48.0;
pub(super) const LAYERS_BTN_H: f32 = 36.0;
pub(super) const LAYERS_ICON: f32 = 31.0;
/// Alto extra que se le da a TODOS los botones cuando alguno muestra su nombre debajo del icono.
pub(super) const LABEL_H: f32 = 16.0;

/// Como se dibuja un boton de la barra: lo que el modo edicion deja cambiar (color, tamano, texto, recuadro, puntito e
/// insignia). Se arma con `btn_look`.
#[derive(Clone, Copy)]
pub(super) struct BtnLook {
    /// Color del icono en reposo (con el mouse encima es blanco; abierto, `active`).
    pub(super) normal: Color32,
    /// Lado del icono.
    pub(super) icon: f32,
    /// Color del boton mientras su panel esta abierto (icono, texto, puntito y recuadro).
    pub(super) active: Color32,
    /// Texto debajo del icono, si el boton lo muestra.
    pub(super) label: Option<&'static str>,
    /// Alto extra del boton para el texto (el mismo en todos los botones de la barra).
    pub(super) extra_h: f32,
    /// Recuadro redondeado detras mientras esta abierto.
    pub(super) active_bg: bool,
    /// Puntito debajo mientras esta abierto.
    pub(super) dot: bool,
    /// Insignia roja en la esquina del icono.
    pub(super) badge: bool,
    /// Color de la insignia.
    pub(super) badge_color: Color32,
}

/// El aspecto del boton `id` (uno de los `BTN_*`) segun el estilo de la barra.
pub(super) fn btn_look(style: &BarStyle, id: u8) -> BtnLook {
    let b = style.btn[id as usize];
    BtnLook {
        normal: style.icon_color(),
        icon: style.icon_size,
        active: style.active_color(),
        label: b.label.then(|| btn_name(id)),
        extra_h: style.label_extra(),
        active_bg: b.active_bg,
        dot: b.dot,
        badge: b.badge,
        badge_color: style.badge_color32(),
    }
}

/// Reserva el boton y dibuja lo que tienen en comun los cuatro: fondo con el mouse encima, recuadro de activo, icono,
/// texto, puntito e insignia. Devuelve la respuesta y el rectangulo del icono (para que cada boton ponga sus extras).
fn button_chrome(
    ui: &mut egui::Ui,
    look: BtnLook,
    open: bool,
    icon: egui::ImageSource<'static>,
) -> (egui::Response, Rect) {
    let base = btn_size(look.icon);
    let (rect, response) =
        ui.allocate_exact_size(vec2(base.x, base.y + look.extra_h), Sense::click());
    let mut icon_rect = Rect::from_center_size(rect.center(), vec2(look.icon, look.icon));
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if open && look.active_bg {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(10),
                look.active.gamma_multiply(0.25),
            );
        } else if hovered {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if open {
            look.active
        } else if hovered {
            Color32::WHITE
        } else {
            look.normal
        };
        // Con texto, el icono va arriba y el texto debajo; sin texto, el icono queda centrado en el boton.
        let cy = if look.label.is_some() {
            rect.min.y + base.y / 2.0
        } else {
            rect.center().y
        };
        icon_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, cy),
            vec2(look.icon, look.icon),
        );
        egui::Image::new(icon).tint(color).paint_at(ui, icon_rect);
        if let Some(text) = look.label {
            ui.painter().text(
                egui::pos2(rect.center().x, rect.bottom() - 11.0),
                Align2::CENTER_CENTER,
                text,
                FontId::proportional(10.0),
                color,
            );
        }
        if open && look.dot {
            ui.painter().circle_filled(
                egui::pos2(rect.center().x, rect.bottom() - 2.5),
                2.0,
                look.active,
            );
        }
        if look.badge {
            ui.painter().circle_filled(
                icon_rect.right_top() + vec2(-3.0, 3.0),
                4.5,
                look.badge_color,
            );
        }
    }
    (response, icon_rect)
}

/// Boton de capas de la barra inferior: el icono de hojas apiladas con el numero de capas sobre la hoja de enfrente.
/// Abre el panel de capas; se pinta de azul mientras ese panel esta abierto.
pub(super) fn layers_button(
    ui: &mut egui::Ui,
    count: usize,
    open: bool,
    look: BtnLook,
) -> egui::Response {
    let (response, icon_rect) = button_chrome(
        ui,
        look,
        open,
        egui::include_image!("../../assets/icons/layers.svg"),
    );
    if ui.is_rect_visible(response.rect) {
        // La hoja de enfrente del icono es el cuadrado de arriba a la izquierda; su centro esta al 41 % del icono.
        let label_pos = icon_rect.min + icon_rect.size() * 0.41;
        // El número escala con el icono (con el normal, 31, mide 13 o 10).
        let size = (if count >= 10 { 10.0 } else { 13.0 }) * look.icon / LAYERS_ICON;
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
pub(super) fn menu_button(ui: &mut egui::Ui, open: bool, look: BtnLook) -> egui::Response {
    let (response, _) = button_chrome(
        ui,
        look,
        open,
        egui::include_image!("../../assets/icons/undo.svg"),
    );
    response.on_hover_text("Menu")
}

/// Boton de opciones de la barra inferior (a la izquierda del cubo): icono de dos interruptores. Se pinta de azul
/// mientras su panel esta abierto. Devuelve la respuesta para que la barra abra o cierre el panel.
pub(super) fn options_button(ui: &mut egui::Ui, open: bool, look: BtnLook) -> egui::Response {
    let (response, _) = button_chrome(
        ui,
        look,
        open,
        egui::include_image!("../../assets/icons/toggles.svg"),
    );
    response.on_hover_text("Opciones")
}

/// Boton del cubo de la barra inferior (a la izquierda del de capas): muestra cuantas mallas (partes) tiene el modelo.
/// Abre el panel de mallas; se pinta de azul mientras ese panel esta abierto.
pub(super) fn cube_button(
    ui: &mut egui::Ui,
    count: usize,
    open: bool,
    look: BtnLook,
) -> egui::Response {
    let (response, icon_rect) = button_chrome(
        ui,
        look,
        open,
        egui::include_image!("../../assets/icons/cube.svg"),
    );
    if ui.is_rect_visible(response.rect) {
        // Numero en el centro del cubo, negro con contorno blanco para que se lea sobre las caras y sobre los huecos.
        let pos = icon_rect.min + icon_rect.size() * vec2(0.5, 0.56);
        let font = FontId::proportional(
            (if count >= 10 { 9.0 } else { 11.0 }) * look.icon / LAYERS_ICON,
        );
        outlined_number(ui.painter(), pos, count.to_string(), font);
    }
    response.on_hover_text("Mallas")
}

/// Línea curva de acento: sale del borde de abajo a la izquierda del botón activo `b`, sube en una S suave, pasa por
/// arriba del botón, baja y sigue por el borde de abajo hacia la derecha apagándose. `bar` = rectángulo de la barra.
fn paint_curve(p: &egui::Painter, bar: Rect, b: Rect, st: &BarStyle) {
    let color = st.curve_color32();
    let w = st.curve_w;
    // Altura de la línea abajo y arriba (dentro del borde de la barra).
    let y_b = bar.bottom() - st.border_w - 7.0 - w / 2.0;
    let y_t = bar.top() + st.border_w + 6.0 + w / 2.0;
    if y_b - y_t < 8.0 {
        return;
    }
    // Límites horizontales: la línea de abajo no puede salirse de la curva de las esquinas de la píldora. A la altura
    // `d` sobre el borde de abajo, el borde de la barra está `inset` más adentro que su lado.
    let rad = st.radius as f32;
    let d = bar.bottom() - y_b;
    let inset = if rad > d {
        rad - (rad * rad - (rad - d) * (rad - d)).max(0.0).sqrt()
    } else {
        0.0
    };
    let lo = bar.left() + inset + 6.0;
    let hi = bar.right() - inset - 6.0;
    // Lóbulo en forma de pestaña: lados casi verticales, esquinas de arriba redondeadas y, abajo, empalmes cóncavos que
    // lo unen con la línea del borde. Todo en un solo trazo (uniones lisas, sin huecos entre segmentos).
    let pad = 8.0;
    // Si el botón está en la punta de la barra, el lóbulo se acerca al botón en vez de pasarse del borde.
    let x_l = (b.left() - pad).max(lo + 4.0);
    let x_r = (b.right() + pad).min(hi - 4.0);
    // Radio de las esquinas de arriba y de cada empalme de abajo (el del lado de la punta se achica si no hay sitio).
    let r = ((y_b - y_t) / 2.0).min((x_r - x_l) / 2.0).min(18.0).max(1.0);
    let rl = (x_l - lo).clamp(0.0, r);
    let rr = (hi - x_r).clamp(0.0, r);
    let xc = x_r + rr;
    // Arco de radio `rad_` con centro `c`, de `from` a `to` grados (el eje y de la pantalla apunta hacia abajo).
    let arc = |pts: &mut Vec<egui::Pos2>, c: egui::Pos2, rad_: f32, from: f32, to: f32| {
        const K: usize = 10;
        for i in 0..=K {
            let a = (from + (to - from) * i as f32 / K as f32).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * rad_);
        }
    };
    let mut lobe: Vec<egui::Pos2> = Vec::new();
    arc(&mut lobe, egui::pos2(x_l - rl, y_b - rl), rl, 90.0, 0.0);
    arc(&mut lobe, egui::pos2(x_l + r, y_t + r), r, 180.0, 270.0);
    arc(&mut lobe, egui::pos2(x_r - r, y_t + r), r, 270.0, 360.0);
    arc(&mut lobe, egui::pos2(x_r + rr, y_b - rr), rr, 180.0, 90.0);
    let x_in = x_l - rl;
    let xs = (x_in - 40.0).max(lo);
    let xe = hi;
    // Tres pasadas: dos trazos anchos y casi transparentes (el brillo suave) y, encima, la línea nítida.
    for (sw, am) in [(w + 6.0, 0.08), (w + 3.0, 0.18), (w, 1.0)] {
        p.add(egui::epaint::PathShape::line(
            lobe.clone(),
            Stroke::new(sw, color.gamma_multiply(am)),
        ));
        // Entrada corta por la izquierda: la línea aparece de a poco antes de subir.
        if x_in > xs {
            const L: usize = 12;
            let mut prev = egui::pos2(xs, y_b);
            for i in 1..=L {
                let cur = egui::pos2(xs + (x_in - xs) * i as f32 / L as f32, y_b);
                let alpha = (i as f32 - 0.5) / L as f32 * am;
                p.line_segment([prev, cur], Stroke::new(sw, color.gamma_multiply(alpha)));
                prev = cur;
            }
        }
        // Cola por el borde de abajo, que se apaga hacia la derecha.
        if xe > xc {
            const M: usize = 28;
            let mut prev = egui::pos2(xc, y_b);
            for i in 1..=M {
                let cur = egui::pos2(xc + (xe - xc) * i as f32 / M as f32, y_b);
                let alpha = (1.0 - (i as f32 - 0.5) / M as f32) * am;
                p.line_segment([prev, cur], Stroke::new(sw, color.gamma_multiply(alpha)));
                prev = cur;
            }
        }
    }
}

/// Anchura mínima de la barra: justo lo que ocupan sus cuatro botones con la separación normal (8) entre ellos, el
/// margen de 8 a cada lado y el borde. Más angosta que esto los botones no caben; más ancha deja un vacío.
pub(super) fn bar_min_width(border_w: f32, icon: f32) -> f32 {
    bar_min_width_m(border_w, icon, 8.0)
}

/// Lo mismo con el margen horizontal elegido (`BarStyle::margin_x`: crece con las esquinas redondeadas).
pub(super) fn bar_min_width_m(border_w: f32, icon: f32, margin_x: f32) -> f32 {
    4.0 * btn_size(icon).x + 3.0 * 8.0 + 2.0 * margin_x + 2.0 * border_w
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
    bar_min_height_l(border_w, icon, 0.0)
}

/// Lo mismo con el alto extra que piden los botones que muestran su nombre debajo (`BarStyle::label_extra`).
pub(super) fn bar_min_height_l(border_w: f32, icon: f32, label_extra: f32) -> f32 {
    btn_size(icon).y + label_extra + 2.0 * border_w
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
        .max(bar_min_width_m(
            style.border_w,
            style.icon_size,
            style.margin_x(),
        ))
        .min(width)
        .round();
    // Lo que mide por dentro: sin el margen a cada lado ni el borde.
    let inner_w = bar_w - 2.0 * style.margin_x() - 2.0 * style.border_w;
    // Posición de la barra: centrada abajo, calculada con el tamaño de ESTE cuadro. Con `anchor`, egui la colocaba con el
    // tamaño del cuadro anterior, y al cambiar la anchura o la altura (deslizadores del modo edición) la barra saltaba
    // un cuadro fuera de lugar y parpadeaba. El borde suma su grosor a cada lado; el margen vertical es 0.
    // La altura elegida es la TOTAL (borde incluido): subir el grosor del borde no agranda la barra, se come el interior.
    // Solo si ya no caben los botones (36 + texto + borde arriba y abajo) la barra crece lo justo.
    let bar_h = style.height.max(bar_min_height_l(
        style.border_w,
        style.icon_size,
        style.label_extra(),
    ));
    let inner_h = bar_h - 2.0 * style.border_w;
    let screen = ui.ctx().content_rect();
    let bar_pos = egui::pos2(
        (screen.center().x - bar_w / 2.0).round(),
        (screen.bottom() - style.lift - bar_h).round(),
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
            // Sombra: se dibuja primero para que quede debajo del fondo.
            if style.shadow > 0 {
                ui.painter().add(style.shadow_shape(rect));
            }
            ui.painter().rect(
                rect,
                style.corner(),
                style.fill(),
                style.stroke(),
                StrokeKind::Inside,
            );
            // Zona de los botones: dentro del borde y del margen a cada lado.
            let inner = Rect::from_min_size(
                rect.min + vec2(style.margin_x() + style.border_w, style.border_w),
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
                            let look = btn_look(&style, id);
                            match id {
                                BTN_MENU => {
                                    let m = menu_button(ui, options.menu_open, look);
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
                                        look,
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
                                        look,
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
                                    let r = options_button(ui, options.open, look);
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
            // Línea curva de acento alrededor del botón que tiene su panel abierto (si no hay ninguno, no se dibuja).
            if style.curve_on {
                let active_rect = if options.open {
                    options_rect
                } else if options.menu_open {
                    menu_rect
                } else if options.layers_open {
                    layers_rect
                } else if options.meshes_open {
                    mesh_rect
                } else {
                    None
                };
                if let Some(b) = active_rect {
                    paint_curve(ui.painter(), rect, b, &style);
                }
            }
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
