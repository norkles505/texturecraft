//! Panel de capas (como el de Ibis Paint), con el menu de tres puntos y el desplegable de modos de mezcla.

use super::*;

// ---------------------------------------------------------------------------------------------------------------------
// Panel de capas (como el de Ibis Paint): titulo "Capa", lista con la capa de seleccion fija arriba, tira de botones a
// cada lado, fila "Fondo", caja de recorte / bloqueo alfa / modo de mezcla y barra de opacidad.
// ---------------------------------------------------------------------------------------------------------------------

/// Medidas del panel (en puntos de egui): ancho del recuadro negro, margen del marco, alto del titulo, alto del
/// recuadro negro, ancho de las tiras de botones, alto de la fila "Fondo", de cada capa, de la caja de abajo y de la
/// fila de la opacidad.
pub(super) const LP_W: f32 = 312.0;
pub(super) const LP_PAD: f32 = 6.0;
pub(super) const LP_TITLE_H: f32 = 34.0;
pub(super) const LP_BOX_H: f32 = 330.0;
pub(super) const LP_LEFT_W: f32 = 40.0;
pub(super) const LP_RIGHT_W: f32 = 34.0;
pub(super) const LP_BG_H: f32 = 31.0;
pub(super) const LP_ROW_H: f32 = 70.0;
/// Miniatura de cada capa (tambien la de la capa de seleccion): vertical, como en Ibis Paint.
pub(super) const LP_THUMB_W: f32 = 48.0;
pub(super) const LP_THUMB_H: f32 = 58.0;
pub(super) const LP_CTRL_H: f32 = 34.0;
pub(super) const LP_SLIDER_H: f32 = 36.0;
/// Fondo de la capa elegida y gris de la parte de arriba de la tira izquierda.
pub(super) const LP_SELECTED: Color32 = Color32::from_rgb(205, 224, 247);
pub(super) const LP_STRIP_GRAY: Color32 = Color32::from_gray(100);
/// Gris oscuro del fondo de la lista de capas (lo que se ve donde no hay filas).
pub(super) const LP_LIST_EMPTY: Color32 = Color32::from_gray(84);

/// Desplegable de modos de mezcla: alto de cada modo y de cada titulo de grupo, sangria del texto de los modos (a la
/// izquierda queda el hueco de la palomita) y gris oscuro de los titulos.
pub(super) const BP_W: f32 = 184.0;
pub(super) const BP_ROW_H: f32 = 34.0;
pub(super) const BP_HEAD_H: f32 = 17.0;
pub(super) const BP_TEXT_X: f32 = 34.0;
/// Grosor del marco gris oscuro que rodea la lista y hueco entre el globo y la caja del modo (ahi va la puntita).
pub(super) const BP_BORDER: f32 = 7.0;
pub(super) const BP_GAP: f32 = 12.0;
pub(super) const BP_HEAD_FILL: Color32 = Color32::from_gray(75);

/// Menu de los tres puntos del panel de capas (como el de Ibis Paint): ancho, alto de cada fila y tamano del texto.
/// Cada fila es (nombre, activa); las que no estan activas se ven en gris y no responden al clic. Por ahora ninguna
/// tiene funcion: al hacer clic en una activa solo se cierra el menu.
pub(super) const MORE_W: f32 = 235.0;
pub(super) const MORE_ROW_H: f32 = 36.0;
pub(super) const MORE_TEXT_SIZE: f32 = 14.0;
pub(super) const MORE_ITEMS: [(&str, bool); 5] = [
    ("Selecciona la opacidad", true),
    ("Renombrar capa", true),
    ("Bloquear capa", true),
    ("Iniciar Modo Solo", true),
    ("Guardar como PNG Transparente", true),
];

/// Grupos del desplegable (titulo y sus modos, en el orden de `BlendMode::ALL`). El titulo vacio = sin barra de titulo.
pub(super) const BLEND_GROUPS: [(&str, &[BlendMode]); 6] = [
    ("", &[BlendMode::Normal]),
    (
        "Oscurecer",
        &[BlendMode::Darken, BlendMode::Multiply, BlendMode::ColorBurn, BlendMode::LinearBurn, BlendMode::DarkerColor],
    ),
    (
        "Aclarar",
        &[
            BlendMode::Lighten,
            BlendMode::Screen,
            BlendMode::ColorDodge,
            BlendMode::LinearDodge,
            BlendMode::Add,
            BlendMode::LighterColor,
        ],
    ),
    (
        "Contraste",
        &[
            BlendMode::Overlay,
            BlendMode::SoftLight,
            BlendMode::HardLight,
            BlendMode::VividLight,
            BlendMode::LinearLight,
            BlendMode::PinLight,
            BlendMode::HardMix,
        ],
    ),
    (
        "Diferencia",
        &[BlendMode::Invert, BlendMode::Difference, BlendMode::Exclusion, BlendMode::Subtract, BlendMode::Divide],
    ),
    ("Color", &[BlendMode::Hue, BlendMode::Saturation, BlendMode::Color, BlendMode::Luminosity]),
];

/// Rellena `rect` con un tablero de ajedrez de casillas de lado `cell` (colores `a` y `b`).
pub(super) fn paint_checker(p: &egui::Painter, rect: Rect, cell: f32, a: Color32, b: Color32) {
    p.rect_filled(rect, CornerRadius::same(0), a);
    let cols = (rect.width() / cell).ceil() as i32;
    let rows = (rect.height() / cell).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            if (x + y) % 2 == 1 {
                let r = Rect::from_min_size(rect.min + vec2(x as f32 * cell, y as f32 * cell), vec2(cell, cell));
                p.rect_filled(r.intersect(rect), CornerRadius::same(0), b);
            }
        }
    }
}

/// Degradado vertical de `top` a `bottom` sobre `rect` (insinua que la lista sigue fuera de la vista). Las esquinas de
/// abajo (`round_bottom`) o las de arriba se redondean con radio `r`, para no salirse del marco redondeado.
pub(super) fn paint_fade(p: &egui::Painter, rect: Rect, top: Color32, bottom: Color32, r: f32, round_bottom: bool) {
    const STEPS: usize = 24;
    let mut mesh = egui::Mesh::default();
    for i in 0..=STEPS {
        let t = i as f32 / STEPS as f32;
        let y = rect.top() + rect.height() * t;
        // Cuanto se mete el borde por la curva de la esquina a esta altura (0 si ya no hay curva).
        let d = if round_bottom { rect.bottom() - y } else { y - rect.top() };
        let inset = if d < r { r - (r * r - (r - d) * (r - d)).max(0.0).sqrt() } else { 0.0 };
        let col = top.lerp_to_gamma(bottom, t);
        mesh.colored_vertex(egui::pos2(rect.left() + inset, y), col);
        mesh.colored_vertex(egui::pos2(rect.right() - inset, y), col);
    }
    for i in 0..STEPS as u32 {
        let a = i * 2;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 3, a + 2);
    }
    p.add(egui::Shape::mesh(mesh));
}

/// Nombre del modo de mezcla para mostrar, en español (los mismos que usa el desplegable).
pub(super) fn blend_label(mode: BlendMode) -> String {
    match mode {
        BlendMode::Normal => "Normal",
        BlendMode::Darken => "Oscurecer",
        BlendMode::Multiply => "Multiplicar",
        BlendMode::ColorBurn => "Subexponer color",
        BlendMode::LinearBurn => "Subexposición lineal",
        BlendMode::DarkerColor => "Color más oscuro",
        BlendMode::Lighten => "Aclarar",
        BlendMode::Screen => "Trama",
        BlendMode::ColorDodge => "Sobreexponer color",
        BlendMode::LinearDodge => "Sobreexposición lineal",
        BlendMode::Add => "Añadir",
        BlendMode::LighterColor => "Color más claro",
        BlendMode::Overlay => "Superponer",
        BlendMode::SoftLight => "Luz suave",
        BlendMode::HardLight => "Luz fuerte",
        BlendMode::VividLight => "Luz intensa",
        BlendMode::LinearLight => "Luz lineal",
        BlendMode::PinLight => "Luz focal",
        BlendMode::HardMix => "Mezcla fuerte",
        BlendMode::Invert => "Invertir",
        BlendMode::Difference => "Diferencia",
        BlendMode::Exclusion => "Exclusión",
        BlendMode::Subtract => "Restar",
        BlendMode::Divide => "Dividir",
        BlendMode::Hue => "Tono",
        BlendMode::Saturation => "Saturación",
        BlendMode::Color => "Color",
        BlendMode::Luminosity => "Luminosidad",
    }
    .to_string()
}

/// Boton de icono de los paneles oscuros: zona de clic de `hit`, icono de lado `size` centrado en `center`, tenido de
/// azul si esta activo, de blanco al pasar el cursor y de gris claro si no.
pub(super) fn panel_icon(
    ui: &egui::Ui,
    id: egui::Id,
    center: egui::Pos2,
    hit: egui::Vec2,
    size: f32,
    icon: egui::ImageSource<'static>,
    active: bool,
) -> egui::Response {
    let r = ui.interact(Rect::from_center_size(center, hit), id, Sense::click());
    let color = if active {
        ACCENT
    } else if r.hovered() {
        Color32::WHITE
    } else {
        Color32::from_gray(205)
    };
    egui::Image::new(icon).tint(color).paint_at(ui, Rect::from_center_size(center, vec2(size, size)));
    r
}

/// Panel flotante del boton de capas: aparece encima del boton, pegado a su borde derecho, y se cierra al hacer clic
/// fuera de el. Lee y cambia las capas de `stack`; devuelve true si hay que recomponer la textura (ojo, opacidad,
/// modo de mezcla, recorte o fondo).
pub(super) fn layers_panel(ctx: &egui::Context, options: &mut OptionsPanel, stack: &mut LayerStack, button: Rect) -> bool {
    if !options.layers_open {
        options.more_open = false;
        return false;
    }
    let mut recompose = false;
    // Donde quedo el boton de los tres puntos (el menu se abre pegado a el).
    let mut dots_rect: Option<Rect> = None;
    // Donde quedo el desplegable de modos de mezcla (el desplegable se abre encima de el).
    let mut dropdown: Option<Rect> = None;
    #[allow(deprecated)]
    let id = egui::Id::new("layers_panel");
    let out = egui::Area::new(id)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(egui::pos2(button.right(), button.top() - 10.0))
        .sense(Sense::click())
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(LP_PAD as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let ui_id = ui.scope_id();
                    let p = ui.painter().clone();

                    // Titulo.
                    let (title, _) = ui.allocate_exact_size(vec2(LP_W, LP_TITLE_H), Sense::hover());
                    p.text(
                        egui::pos2(title.left() + 6.0, title.center().y),
                        Align2::LEFT_CENTER,
                        "Capa",
                        FontId::proportional(15.0),
                        Color32::WHITE,
                    );

                    // Recuadro negro: tira izquierda | lista | tira derecha.
                    let (bx, _) = ui.allocate_exact_size(vec2(LP_W, LP_BOX_H), Sense::hover());
                    p.rect_filled(bx, CornerRadius::same(8), Color32::BLACK);
                    let left = Rect::from_min_size(bx.min, vec2(LP_LEFT_W, LP_BOX_H));
                    let right = Rect::from_min_max(egui::pos2(bx.right() - LP_RIGHT_W, bx.top()), bx.max);
                    let list = Rect::from_min_max(
                        egui::pos2(left.right(), bx.top()),
                        egui::pos2(right.left(), bx.bottom() - 1.0),
                    );
                    let gray_h = LP_BOX_H - 100.0;
                    p.rect_filled(
                        Rect::from_min_size(left.min, vec2(LP_LEFT_W, gray_h)),
                        CornerRadius { nw: 8, ne: 0, sw: 0, se: 0 },
                        LP_STRIP_GRAY,
                    );
                    p.rect_filled(list, CornerRadius::same(0), LP_LIST_EMPTY);

                    // Tira izquierda: nueva capa (funciona), duplicar y carpeta (todavia sin funcion).
                    let lx = left.center().x;
                    let ly = |k: f32| bx.bottom() - 18.0 - 33.0 * k;
                    let hit_l = vec2(LP_LEFT_W, 32.0);
                    let r = panel_icon(
                        ui,
                        ui_id.with("lp_add"),
                        egui::pos2(lx, ly(2.0)),
                        hit_l,
                        22.0,
                        egui::include_image!("../../assets/icons/layer_add.svg"),
                        false,
                    );
                    if r.on_hover_text("Nueva capa").clicked() {
                        stack.add_layer();
                    }
                    panel_icon(
                        ui,
                        ui_id.with("lp_copy"),
                        egui::pos2(lx, ly(1.0)),
                        hit_l,
                        22.0,
                        egui::include_image!("../../assets/icons/layer_copy.svg"),
                        false,
                    )
                    .on_hover_text("Duplicar capa (todavia sin funcion)");
                    panel_icon(
                        ui,
                        ui_id.with("lp_folder"),
                        egui::pos2(lx, ly(0.0)),
                        hit_l,
                        28.0,
                        egui::include_image!("../../assets/icons/layer_folder.svg"),
                        false,
                    )
                    .on_hover_text("Carpeta nueva (todavia sin funcion)");

                    // Tira derecha (de arriba abajo): cuadricula, circulo, combinar, borrar y mas. Todavia sin funcion.
                    let rx = right.center().x;
                    let ry = |k: f32| bx.bottom() - 15.0 - 27.0 * k;
                    let hit_r = vec2(LP_RIGHT_W, 26.0);
                    panel_icon(
                        ui,
                        ui_id.with("lp_checker"),
                        egui::pos2(rx, ry(4.0)),
                        hit_r,
                        25.0,
                        egui::include_image!("../../assets/icons/layer_checker.svg"),
                        false,
                    )
                    .on_hover_text("Limpiar capa (todavia sin funcion)");
                    panel_icon(
                        ui,
                        ui_id.with("lp_half"),
                        egui::pos2(rx, ry(3.0)),
                        hit_r,
                        19.0,
                        egui::include_image!("../../assets/icons/layer_invert.svg"),
                        false,
                    )
                    .on_hover_text("Invertir colores (todavia sin funcion)");
                    panel_icon(
                        ui,
                        ui_id.with("lp_merge"),
                        egui::pos2(rx, ry(2.0)),
                        hit_r,
                        18.0,
                        egui::include_image!("../../assets/icons/layer_merge.svg"),
                        false,
                    )
                    .on_hover_text("Combinar hacia abajo (todavia sin funcion)");
                    panel_icon(
                        ui,
                        ui_id.with("lp_trash"),
                        egui::pos2(rx, ry(1.0)),
                        hit_r,
                        26.0,
                        egui::include_image!("../../assets/icons/layer_trash.svg"),
                        false,
                    )
                    .on_hover_text("Borrar capa (todavia sin funcion)");
                    let dots = ui.interact(
                        Rect::from_center_size(egui::pos2(rx, ry(0.0)), hit_r),
                        ui_id.with("lp_dots"),
                        Sense::click(),
                    );
                    let dots_color = if dots.hovered() || options.more_open { Color32::WHITE } else { Color32::from_gray(205) };
                    for k in -1..=1 {
                        p.circle_filled(egui::pos2(rx, ry(0.0) + k as f32 * 6.5), 2.2, dots_color);
                    }
                    dots_rect = Some(dots.rect);
                    if dots.clone().on_hover_text("Mas opciones").clicked() {
                        options.more_open = !options.more_open;
                        options.blend_open = false;
                    }

                    // Fila "Fondo" (abajo de la lista) y zona de la lista. Como en Ibis Paint, si todo cabe la pila de filas
                    // (capa de seleccion, capas y "Fondo") se pega abajo y arriba queda el fondo oscuro vacio; si no cabe,
                    // la capa de seleccion queda fija arriba y las capas se desplazan entre ella y "Fondo".
                    let bg_row = Rect::from_min_max(egui::pos2(list.left(), list.bottom() - LP_BG_H), list.max);
                    let layers_h = stack.layers.len() as f32 * LP_ROW_H;
                    let fits = LP_ROW_H + layers_h + LP_BG_H <= list.height();
                    let (sel_top, area_top) = if fits {
                        (bg_row.top() - layers_h - LP_ROW_H, bg_row.top() - layers_h)
                    } else {
                        (list.top(), list.top() + LP_ROW_H)
                    };
                    let area = Rect::from_min_max(egui::pos2(list.left(), area_top), egui::pos2(list.right(), bg_row.top()));
                    // Fondo claro solo donde hay filas.
                    p.rect_filled(
                        Rect::from_min_max(egui::pos2(list.left(), sel_top), list.max),
                        CornerRadius::same(0),
                        MENU_FILL,
                    );

                    // Capa de seleccion: arriba de las capas, todavia sin funcion (siempre "No seleccionado").
                    let sel_row = Rect::from_min_size(egui::pos2(list.left(), sel_top), vec2(list.width(), LP_ROW_H));
                    let th = Rect::from_min_size(
                        egui::pos2(sel_row.left() + 6.0, sel_row.center().y - LP_THUMB_H * 0.5),
                        vec2(LP_THUMB_W, LP_THUMB_H),
                    );
                    paint_checker(&p, th, 6.0, Color32::from_rgb(255, 205, 210), Color32::from_rgb(246, 176, 186));
                    p.rect_stroke(th, CornerRadius::same(2), Stroke::new(1.0, Color32::from_gray(170)), StrokeKind::Inside);
                    let tx = th.right() + 8.0;
                    p.text(
                        egui::pos2(tx, sel_row.top() + 13.0),
                        Align2::LEFT_CENTER,
                        "Capa de selección",
                        FontId::proportional(14.0),
                        MENU_TEXT,
                    );
                    let dash = Rect::from_min_size(egui::pos2(tx, sel_row.top() + 27.0), vec2(20.0, 15.0));
                    let pts = [dash.left_top(), dash.right_top(), dash.right_bottom(), dash.left_bottom(), dash.left_top()];
                    p.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, Color32::from_gray(70)), 2.0, 2.0));
                    p.text(
                        egui::pos2(tx, sel_row.top() + LP_ROW_H - 13.0),
                        Align2::LEFT_CENTER,
                        "No seleccionado",
                        FontId::proportional(13.0),
                        MENU_TEXT,
                    );
                    p.hline(sel_row.x_range(), sel_row.bottom() - 0.5, Stroke::new(1.0, MENU_LINE));

                    // Lista de capas: la de mas arriba primero. La rueda la desplaza.
                    let n = stack.layers.len();
                    let max_scroll = if fits { 0.0 } else { (n as f32 * LP_ROW_H - area.height()).max(0.0) };
                    let hovering_list = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|m| area.contains(m));
                    if hovering_list && !options.blend_open {
                        options.layers_scroll -= ctx.input(|i| i.smooth_scroll_delta.y);
                    }
                    options.layers_scroll = options.layers_scroll.clamp(0.0, max_scroll);
                    let scroll = options.layers_scroll;
                    let pa = p.with_clip_rect(area);
                    // Que lo que se dibuje con `ui` (los iconos) tambien se recorte a la lista.
                    let old_clip = ui.clip_rect();
                    ui.set_clip_rect(area.intersect(old_clip));
                    for d in 0..n {
                        let idx = n - 1 - d;
                        let row = Rect::from_min_size(
                            egui::pos2(area.left(), area.top() - scroll + d as f32 * LP_ROW_H),
                            vec2(area.width(), LP_ROW_H),
                        );
                        if row.bottom() < area.top() || row.top() > area.bottom() {
                            continue;
                        }
                        let rr = ui.interact(row.intersect(area), ui_id.with(("lrow", d)), Sense::click());
                        if rr.clicked() {
                            stack.selected = idx;
                        }
                        let th = Rect::from_min_size(
                            egui::pos2(row.left() + 6.0, row.center().y - LP_THUMB_H * 0.5),
                            vec2(LP_THUMB_W, LP_THUMB_H),
                        );
                        let eye_c = egui::pos2(th.right() + 26.0, row.center().y);
                        // El ojo se crea despues de la fila para tener prioridad donde se solapan.
                        let eye = ui.interact(
                            Rect::from_center_size(eye_c, vec2(26.0, 26.0)).intersect(area),
                            ui_id.with(("leye", d)),
                            Sense::click(),
                        );
                        if eye.clicked() {
                            stack.layers[idx].visible = !stack.layers[idx].visible;
                            recompose = true;
                        }
                        let selected = stack.selected == idx;
                        let l = &stack.layers[idx];
                        if selected {
                            pa.rect_filled(row, CornerRadius::same(0), LP_SELECTED);
                        } else if rr.hovered() {
                            pa.rect_filled(row, CornerRadius::same(0), Color32::from_black_alpha(10));
                        }
                        paint_checker(&pa, th, 6.0, Color32::from_gray(250), Color32::from_gray(222));
                        let border = if selected {
                            Stroke::new(2.0, ACCENT)
                        } else {
                            Stroke::new(1.0, Color32::from_gray(170))
                        };
                        pa.rect_stroke(th, CornerRadius::same(3), border, StrokeKind::Inside);
                        pa.text(
                            egui::pos2(th.right() + 8.0, row.top() + 13.0),
                            Align2::LEFT_CENTER,
                            &l.name,
                            FontId::proportional(16.0),
                            MENU_TEXT,
                        );
                        let eye_color = if l.visible { MENU_TEXT } else { Color32::from_gray(175) };
                        egui::Image::new(egui::include_image!("../../assets/icons/layer_visible.svg"))
                            .tint(eye_color)
                            .paint_at(ui, Rect::from_center_size(eye_c, vec2(24.0, 24.0)));
                        // Porcentaje y modo de mezcla: centrados en la misma columna (el porcentaje a media altura de la
                        // fila y el modo abajo), como en Ibis Paint.
                        let col_x = row.right() - 62.0;
                        pa.text(
                            egui::pos2(col_x, row.center().y),
                            Align2::CENTER_CENTER,
                            format!("{}%", l.opacity),
                            FontId::proportional(16.0),
                            MENU_TEXT,
                        );
                        pa.text(
                            egui::pos2(col_x, row.bottom() - 13.0),
                            Align2::CENTER_CENTER,
                            blend_label(l.blend),
                            // Los nombres largos ("Sobreexposición lineal") llevan la letra mas chica para no pisar las rayas.
                            FontId::proportional(if l.blend.eq(&BlendMode::Normal) || blend_label(l.blend).chars().count() <= 11 { 14.0 } else { 11.0 }),
                            MENU_TEXT,
                        );
                        // Icono de las tres rayas del extremo derecho (solo adorno por ahora: todavia no se reordena).
                        // Es `assets/icons/layer_drag.svg`; basta con reemplazar el archivo para cambiarlo.
                        egui::Image::new(egui::include_image!("../../assets/icons/layer_drag.svg"))
                            .tint(Color32::from_gray(150))
                            .paint_at(ui, Rect::from_center_size(egui::pos2(row.right() - 20.0, row.center().y), vec2(16.0, 16.0)));
                        pa.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, MENU_LINE));
                    }
                    ui.set_clip_rect(old_clip);
                    // Barrita de desplazamiento, solo si la lista no cabe.
                    if max_scroll > 0.0 {
                        let track_h = area.height() - 8.0;
                        let bar_h = (track_h * area.height() / (n as f32 * LP_ROW_H)).max(16.0);
                        let y = area.top() + 4.0 + (track_h - bar_h) * (scroll / max_scroll);
                        let bar = Rect::from_min_size(egui::pos2(area.right() - 4.0, y), vec2(3.0, bar_h));
                        p.rect_filled(bar, CornerRadius::same(2), Color32::from_black_alpha(70));
                    }

                    // Fila "Fondo": blanco, ajedrez claro, ajedrez oscuro y sin fondo (transparente al exportar).
                    p.hline(bg_row.x_range(), bg_row.top(), Stroke::new(1.0, MENU_LINE));
                    p.text(
                        egui::pos2(bg_row.left() + 8.0, bg_row.center().y),
                        Align2::LEFT_CENTER,
                        "Fondo",
                        FontId::proportional(16.0),
                        MENU_TEXT,
                    );
                    let kinds = [Background::White, Background::Checker, Background::CheckerDark, Background::None];
                    let s = 21.0;
                    for (i, kind) in kinds.iter().copied().enumerate() {
                        let x = bg_row.right() - 6.0 - s - (3 - i) as f32 * (s + 4.0);
                        let sw = Rect::from_min_size(egui::pos2(x, bg_row.center().y - s * 0.5), vec2(s, s));
                        let r = ui.interact(sw, ui_id.with(("lbg", i)), Sense::click());
                        if r.clicked() && stack.background != kind {
                            stack.background = kind;
                            recompose = true;
                        }
                        match kind {
                            Background::White => {
                                p.rect_filled(sw, CornerRadius::same(2), Color32::WHITE);
                            }
                            Background::Checker => {
                                paint_checker(&p, sw, 5.0, Color32::WHITE, Color32::from_gray(222));
                            }
                            Background::CheckerDark => {
                                paint_checker(&p, sw, 5.0, Color32::from_gray(90), Color32::from_gray(55));
                            }
                            Background::None => {
                                p.rect_filled(sw, CornerRadius::same(2), Color32::WHITE);
                                p.line_segment([sw.left_bottom(), sw.right_top()], Stroke::new(1.0, Color32::from_gray(60)));
                            }
                        }
                        let border = if stack.background == kind {
                            Stroke::new(2.0, ACCENT)
                        } else {
                            Stroke::new(1.0, Color32::from_gray(150))
                        };
                        p.rect_stroke(sw, CornerRadius::same(2), border, StrokeKind::Inside);
                    }

                    // Caja de abajo: recortar a la capa de abajo, bloqueo alfa y modo de mezcla de la capa elegida.
                    ui.add_space(4.0);
                    let (ctrl, _) = ui.allocate_exact_size(vec2(LP_W, LP_CTRL_H), Sense::hover());
                    p.rect_filled(ctrl, CornerRadius::same(8), Color32::BLACK);
                    let sel = stack.selected;
                    let clip_on = stack.layers[sel].clip_to_below;
                    let lock_on = stack.layers[sel].alpha_lock;
                    let r = panel_icon(
                        ui,
                        ui_id.with("lp_clip"),
                        egui::pos2(ctrl.left() + 37.0, ctrl.center().y),
                        vec2(44.0, 30.0),
                        20.0,
                        egui::include_image!("../../assets/icons/layer_clip.svg"),
                        clip_on,
                    );
                    if r.on_hover_text("Recortar a la capa de abajo").clicked() {
                        stack.layers[sel].clip_to_below = !clip_on;
                        recompose = true;
                    }
                    let r = panel_icon(
                        ui,
                        ui_id.with("lp_alpha"),
                        egui::pos2(ctrl.left() + 101.0, ctrl.center().y),
                        vec2(44.0, 30.0),
                        20.0,
                        egui::include_image!("../../assets/icons/layer_alpha.svg"),
                        lock_on,
                    );
                    if r.on_hover_text("Bloqueo alfa (solo se pinta donde ya hay pintura)").clicked() {
                        stack.layers[sel].alpha_lock = !lock_on;
                    }
                    // Desplegable del modo de mezcla.
                    let dd = Rect::from_min_max(
                        egui::pos2(ctrl.left() + 138.0, ctrl.center().y - 13.0),
                        egui::pos2(ctrl.right() - 7.0, ctrl.center().y + 13.0),
                    );
                    let r = ui.interact(dd, ui_id.with("lp_blend"), Sense::click());
                    let dd_fill = if r.hovered() { Color32::from_gray(245) } else { Color32::WHITE };
                    p.rect_filled(dd, CornerRadius::same(4), dd_fill);
                    // Los nombres largos ("Sobreexposición lineal") llevan la letra mas chica para no pisar la flechita.
                    let dd_label = blend_label(stack.layers[sel].blend);
                    let dd_size = if dd_label.chars().count() > 16 { 11.0 } else { 13.0 };
                    p.text(
                        egui::pos2(dd.left() + 10.0, dd.center().y),
                        Align2::LEFT_CENTER,
                        dd_label,
                        FontId::proportional(dd_size),
                        Color32::from_gray(30),
                    );
                    let c = egui::pos2(dd.right() - 14.0, dd.center().y + 1.0);
                    let chevron = Stroke::new(1.6, Color32::from_gray(40));
                    p.line_segment([c + vec2(-4.5, 2.0), c + vec2(0.0, -2.5)], chevron);
                    p.line_segment([c + vec2(0.0, -2.5), c + vec2(4.5, 2.0)], chevron);
                    if r.clicked() {
                        options.blend_open = !options.blend_open;
                        options.more_open = false;
                        options.blend_scroll = 0.0;
                    }
                    dropdown = Some(dd);

                    // Opacidad de la capa elegida: el mismo deslizador que usa el panel de color (`slider_row`):
                    // etiqueta, boton -, barra en pastilla con tablero y degradado, boton + y perilla blanca.
                    ui.add_space(4.0);
                    let (sl, _) = ui.allocate_exact_size(vec2(LP_W, LP_SLIDER_H), Sense::hover());
                    let opacity = stack.layers[sel].opacity;
                    let mut frac = opacity as f32 / 100.0;
                    let label = format!("{opacity}%");
                    let e = slider_row(
                        ui,
                        sl.shrink2(vec2(6.0, 0.0)),
                        ui_id.with("lp_opacity"),
                        &label,
                        &mut frac,
                        0.01,
                        |p, r| {
                            pill_checker(p, r, 3.5);
                            pill_gradient(p, r, &[Color32::TRANSPARENT, Color32::from_gray(25)]);
                        },
                    );
                    if e.changed {
                        let v = (frac * 100.0).round() as u8;
                        if v != opacity {
                            stack.layers[sel].opacity = v;
                            recompose = true;
                        }
                    }
                });
        });

    // Desplegable de modos de mezcla (como el de Ibis Paint): globo gris oscuro con una puntita que apunta a la caja del
    // modo y, dentro, la lista clara agrupada (Oscurecer, Aclarar...) con barras de titulo oscuras, filas separadas por
    // lineas, palomita azul en el modo actual y desplazamiento con la rueda. Va pegado al borde derecho del panel.
    let mut popup_rect: Option<Rect> = None;
    if options.blend_open {
        if let Some(dd) = dropdown {
            let current = stack.layers[stack.selected].blend;
            let total_h: f32 = BLEND_GROUPS
                .iter()
                .map(|&(title, modes)| (if title.is_empty() { 0.0 } else { BP_HEAD_H }) + modes.len() as f32 * BP_ROW_H)
                .sum();
            // Alto visible: lo que cabe encima de la caja del modo (hasta 500 puntos); si el contenido es mas alto, se desplaza.
            let view_h = total_h.min((dd.top() - 40.0).clamp(150.0, 500.0));
            let max_scroll = (total_h - view_h).max(0.0);
            let pw = BP_W;
            let panel_right = out.response.rect.right();
            let mut pick: Option<BlendMode> = None;
            #[allow(deprecated)]
            let pid = egui::Id::new("layers_blend_popup");
            let po = egui::Area::new(pid)
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(panel_right, dd.top() - BP_GAP))
                .sense(Sense::click())
                .show(ctx, |ui| {
                    // Marco de afuera = el borde gris oscuro semitransparente (el de las barras); adentro, la lista clara.
                    let outer = Frame::new()
                        .fill(bar_fill())
                        .corner_radius(CornerRadius::same(14))
                        .inner_margin(Margin::same(BP_BORDER as i8))
                        .show(ui, |ui| {
                            Frame::new()
                                .fill(MENU_FILL)
                                .corner_radius(CornerRadius::same(8))
                                .inner_margin(Margin::symmetric(0, 3))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                                    let ui_id = ui.scope_id();
                                    let (area, _) = ui.allocate_exact_size(vec2(pw, view_h), Sense::hover());
                                    // La rueda desplaza la lista mientras el cursor esta sobre ella.
                                    if ctx.input(|i| i.pointer.hover_pos()).is_some_and(|m| area.contains(m)) {
                                        options.blend_scroll -= ctx.input(|i| i.smooth_scroll_delta.y);
                                    }
                                    options.blend_scroll = options.blend_scroll.clamp(0.0, max_scroll);
                                    let scroll = options.blend_scroll;
                                    // Todo se dibuja recortado a la lista.
                                    let pa = ui.painter().with_clip_rect(area);
                                    let mut y = area.top() - scroll;
                                    for (title, modes) in BLEND_GROUPS {
                                        if !title.is_empty() {
                                            let head = Rect::from_min_size(egui::pos2(area.left(), y), vec2(pw, BP_HEAD_H));
                                            if head.bottom() >= area.top() && head.top() <= area.bottom() {
                                                pa.rect_filled(head, CornerRadius::same(0), BP_HEAD_FILL);
                                                pa.text(
                                                    egui::pos2(head.left() + 9.0, head.center().y),
                                                    Align2::LEFT_CENTER,
                                                    title,
                                                    FontId::proportional(10.5),
                                                    Color32::WHITE,
                                                );
                                            }
                                            y += BP_HEAD_H;
                                        }
                                        for (k, &m) in modes.iter().enumerate() {
                                            let row = Rect::from_min_size(egui::pos2(area.left(), y), vec2(pw, BP_ROW_H));
                                            y += BP_ROW_H;
                                            if row.bottom() < area.top() || row.top() > area.bottom() {
                                                continue;
                                            }
                                            let r =
                                                ui.interact(row.intersect(area), ui_id.with(("lbm", m as u8)), Sense::click());
                                            if r.hovered() {
                                                pa.rect_filled(row, CornerRadius::same(0), Color32::from_black_alpha(14));
                                            }
                                            // Palomita azul en el modo que tiene la capa ahora.
                                            if m == current {
                                                let c = egui::pos2(row.left() + 16.0, row.center().y);
                                                let tick = Stroke::new(2.4, ACCENT);
                                                pa.line_segment([c + vec2(-7.0, 0.5), c + vec2(-2.5, 5.0)], tick);
                                                pa.line_segment([c + vec2(-2.5, 5.0), c + vec2(7.5, -6.0)], tick);
                                            }
                                            let label = blend_label(m);
                                            // Los nombres largos ("Sobreexposición lineal") llevan la letra un poco mas chica.
                                            let size = if label.chars().count() > 19 { 12.5 } else { 14.0 };
                                            pa.text(
                                                egui::pos2(row.left() + BP_TEXT_X, row.center().y),
                                                Align2::LEFT_CENTER,
                                                label,
                                                FontId::proportional(size),
                                                MENU_TEXT,
                                            );
                                            // Linea entre modos del mismo grupo (casi de borde a borde; no despues del ultimo).
                                            if k + 1 < modes.len() {
                                                pa.hline(
                                                    row.left() + 2.0..=row.right() - 2.0,
                                                    row.bottom() - 0.5,
                                                    Stroke::new(1.0, MENU_LINE),
                                                );
                                            }
                                            if r.clicked() {
                                                pick = Some(m);
                                            }
                                        }
                                    }
                                    // Difuminado abajo (y arriba si ya se desplazo) cuando la lista sigue fuera de la vista.
                                    // El difuminado cubre tambien los 3 px de margen del marco (arriba y abajo): si no, ahi
                                    // quedaba una franja clara sin sombrear. Sigue las esquinas redondeadas del marco.
                                    let fade = Color32::from_black_alpha(70);
                                    let painter = ui.painter().clone();
                                    if scroll < max_scroll - 0.5 {
                                        let r = Rect::from_min_max(
                                            egui::pos2(area.left(), area.bottom() - 22.0),
                                            egui::pos2(area.right(), area.bottom() + 3.0),
                                        );
                                        paint_fade(&painter, r, Color32::TRANSPARENT, fade, 8.0, true);
                                    }
                                    if scroll > 0.5 {
                                        let r = Rect::from_min_max(
                                            egui::pos2(area.left(), area.top() - 3.0),
                                            egui::pos2(area.right(), area.top() + 22.0),
                                        );
                                        paint_fade(&painter, r, fade, Color32::TRANSPARENT, 8.0, false);
                                    }
                                    // Barrita de desplazamiento, solo si la lista no cabe.
                                    if max_scroll > 0.0 {
                                        let track_h = area.height() - 8.0;
                                        let bar_h = (track_h * view_h / total_h).max(16.0);
                                        let by = area.top() + 4.0 + (track_h - bar_h) * (scroll / max_scroll);
                                        let bar = Rect::from_min_size(egui::pos2(area.right() - 6.0, by), vec2(3.0, bar_h));
                                        ui.painter().rect_filled(bar, CornerRadius::same(2), Color32::from_black_alpha(90));
                                    }
                                });
                        });
                    // Puntita del globo, apuntando al centro de la caja del modo.
                    let b = outer.response.rect.bottom();
                    let cx = dd.center().x;
                    ui.painter().with_clip_rect(ui.ctx().content_rect()).add(egui::Shape::convex_polygon(
                        vec![egui::pos2(cx - 8.0, b - 0.5), egui::pos2(cx + 8.0, b - 0.5), egui::pos2(cx, b + 9.0)],
                        bar_fill(),
                        Stroke::NONE,
                    ));
                });
            if let Some(m) = pick {
                let s = stack.selected;
                stack.layers[s].blend = m;
                recompose = true;
                options.blend_open = false;
            }
            popup_rect = Some(po.response.rect);
        }
    }

    // Menu de los tres puntos (como el de Ibis Paint): panel gris claro con filas separadas por lineas y el mismo borde
    // semitransparente que los demas paneles. Sale pegado al borde derecho del panel de capas, hacia arriba desde el
    // boton. "Rasterizar" va en gris (desactivada). Por ahora las filas solo cierran el menu.
    let mut more_rect: Option<Rect> = None;
    if options.more_open {
        if let Some(dots) = dots_rect {
            let mut close = false;
            #[allow(deprecated)]
            let mid = egui::Id::new("layers_more_menu");
            let mo = egui::Area::new(mid)
                .pivot(Align2::RIGHT_BOTTOM)
                .fixed_pos(egui::pos2(out.response.rect.right() - 2.0, dots.bottom() + 2.0))
                .sense(Sense::click())
                .show(ctx, |ui| {
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
                                    let n = MORE_ITEMS.len();
                                    for (i, (label, enabled)) in MORE_ITEMS.iter().enumerate() {
                                        let sense = if *enabled { Sense::click() } else { Sense::hover() };
                                        let (rect, response) = ui.allocate_exact_size(vec2(MORE_W, MORE_ROW_H), sense);
                                        let p = ui.painter();
                                        // Hover como el del desplegable de modos de mezcla: la fila entera, de borde a
                                        // borde y sin esquinas redondeadas. Solo la primera y la ultima redondean sus
                                        // esquinas de afuera, para seguir la curva del marco.
                                        if *enabled && response.hovered() {
                                            let r = 7;
                                            let corners = CornerRadius {
                                                nw: if i == 0 { r } else { 0 },
                                                ne: if i == 0 { r } else { 0 },
                                                sw: if i + 1 == n { r } else { 0 },
                                                se: if i + 1 == n { r } else { 0 },
                                            };
                                            p.rect_filled(rect, corners, Color32::from_black_alpha(14));
                                        }
                                        let color = if *enabled { MENU_TEXT } else { Color32::from_gray(150) };
                                        p.text(
                                            egui::pos2(rect.left() + MENU_PAD, rect.center().y),
                                            Align2::LEFT_CENTER,
                                            *label,
                                            FontId::proportional(MORE_TEXT_SIZE),
                                            color,
                                        );
                                        if i + 1 < n {
                                            p.hline(rect.left() + 2.0..=rect.right() - 2.0, rect.bottom() - 0.5, Stroke::new(1.0, MENU_LINE));
                                        }
                                        if *enabled && response.clicked() {
                                            close = true;
                                        }
                                    }
                                });
                        });
                });
            more_rect = Some(mo.response.rect);
            if close {
                options.more_open = false;
            }
        }
    }

    // Clic fuera: cierra el desplegable; fuera del panel y de su boton (que ya lo abre/cierra), cierra todo.
    let panel = out.response.rect;
    let (pressed, pos) = ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.interact_pos()));
    if pressed {
        if let Some(pt) = pos {
            let in_popup = popup_rect.is_some_and(|r| r.contains(pt));
            let in_dropdown = dropdown.is_some_and(|r| r.contains(pt));
            let in_more = more_rect.is_some_and(|r| r.contains(pt));
            let in_dots = dots_rect.is_some_and(|r| r.contains(pt));
            if options.blend_open && !in_popup && !in_dropdown {
                options.blend_open = false;
            }
            if options.more_open && !in_more && !in_dots {
                options.more_open = false;
            }
            if !panel.contains(pt) && !in_popup && !in_more && !button.contains(pt) {
                options.layers_open = false;
                options.blend_open = false;
                options.more_open = false;
            }
        }
    }
    recompose
}
