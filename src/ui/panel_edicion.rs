//! Modo edición: se enciende con el interruptor de la ventana de Ajustes. Mientras está encendido:
//! - aparece una pastilla flotante (tres rayas, nombre e interruptor para apagarlo; se puede arrastrar),
//! - no se puede pintar (ver `UiLayer::edit_mode` y `main.rs`),
//! - la barra inferior deja de funcionar como barra: al tocarla se elige (queda con un contorno azul) y sale encima el
//!   panel de propiedades con su color, transparencia, borde y esquinas. Tocarla otra vez, o tocar el vacío, la suelta.
//!
//! Por ahora solo se puede editar la barra inferior (prueba). Lo elegido no se guarda al cerrar el programa.

use super::*;

/// Ancho del contenido de la pastilla y del panel de propiedades.
const ED_LABEL_W: f32 = 128.0;
const ED_ICON_W: f32 = 30.0;
const ED_ICON: f32 = 26.0;
const ED_TOP: f32 = 12.0;
const ED_START_W: f32 = 250.0;
const PANEL_W: f32 = 250.0;
/// Azul de la parte llena de los deslizadores del panel.
const SLIDER_BLUE: Color32 = Color32::from_rgb(100, 160, 255);
/// Grosor máximo del borde y radio máximo de las esquinas que dejan elegir los deslizadores.
const MAX_BORDER: f32 = 8.0;
const MAX_RADIUS: f32 = 24.0;
/// Identificadores de los botones de la barra inferior (para guardar su orden).
pub(super) const BTN_OPTIONS: u8 = 0;
pub(super) const BTN_CUBE: u8 = 1;
pub(super) const BTN_LAYERS: u8 = 2;
pub(super) const BTN_MENU: u8 = 3;
/// Tamaño mínimo y máximo del icono de los botones.
const MIN_ICON: f32 = 20.0;
const MAX_ICON: f32 = 48.0;
/// Altura máxima de la barra. Los mínimos de altura y anchura los dan los botones (`bar_min_height`, `bar_min_width`).
const MAX_BAR_H: f32 = 90.0;

/// Elemento de la interfaz que se puede elegir en modo edición.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditTarget {
    BottomBar,
}

/// Propiedades de la barra inferior que se pueden cambiar en modo edición.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct BarStyle {
    /// Color de fondo y su opacidad (0 = invisible, 255 = sólido).
    pub(super) rgb: [u8; 3],
    pub(super) alpha: u8,
    /// Borde: color con opacidad (RGBA) y grosor en puntos.
    pub(super) border: [u8; 4],
    pub(super) border_w: f32,
    /// Esquinas redondeadas, en puntos.
    pub(super) radius: u8,
    /// Color de los iconos de los botones (cuando no están con el mouse encima ni con su panel abierto).
    pub(super) icon: [u8; 4],
    /// Lado del icono de cada botón, en puntos (el botón crece con él).
    pub(super) icon_size: f32,
    /// Orden de los botones de izquierda a derecha (ids `BTN_*`); se cambia arrastrándolos en modo edición.
    pub(super) order: [u8; 4],
    /// Tamaño: anchura como fracción de la ventana (0.2 a 1; 1 = todo el ancho) y altura en puntos.
    pub(super) width_frac: f32,
    pub(super) height: f32,
}

impl Default for BarStyle {
    /// El estilo de siempre de las barras (`bar_fill`).
    fn default() -> Self {
        BarStyle {
            rgb: [58, 58, 60],
            alpha: BAR_ALPHA,
            border: [255, 255, 255, 16],
            border_w: 1.0,
            radius: 0,
            icon: [205, 205, 205, 255],
            icon_size: LAYERS_ICON,
            order: [BTN_OPTIONS, BTN_CUBE, BTN_LAYERS, BTN_MENU],
            width_frac: 1.0,
            height: BOTTOM_BAR_H,
        }
    }
}

impl BarStyle {
    pub(super) fn icon_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.icon[0], self.icon[1], self.icon[2], self.icon[3])
    }

    pub(super) fn fill(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.rgb[0], self.rgb[1], self.rgb[2], self.alpha)
    }

    pub(super) fn stroke(&self) -> Stroke {
        let b = self.border;
        Stroke::new(
            self.border_w,
            Color32::from_rgba_unmultiplied(b[0], b[1], b[2], b[3]),
        )
    }

    pub(super) fn corner(&self) -> CornerRadius {
        CornerRadius::same(self.radius)
    }
}

/// Pastilla flotante: se dibuja si el modo edición está encendido. Su interruptor es el mismo `options.edit_mode`.
pub(super) fn edit_mode_pill(ctx: &egui::Context, options: &mut OptionsPanel) {
    if !options.edit_mode {
        // Al salir del modo edición no queda nada elegido.
        options.edit_selected = None;
        close_pickers(ctx);
        return;
    }
    #[allow(deprecated)]
    let id = egui::Id::new("edit_mode_pill");
    let start = egui::pos2(ctx.content_rect().center().x - ED_START_W / 2.0, ED_TOP);
    egui::Area::new(id)
        .default_pos(start)
        .movable(true)
        .constrain(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                .corner_radius(CornerRadius::same(22))
                .inner_margin(Margin::symmetric(16, 9))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        let (icon, _) =
                            ui.allocate_exact_size(vec2(ED_ICON_W, TOGGLE_H), Sense::hover());
                        egui::Image::new(egui::include_image!(
                            "../../assets/icons/menu_hamburger.svg"
                        ))
                        .tint(Color32::from_gray(235))
                        .paint_at(
                            ui,
                            Rect::from_center_size(icon.center(), vec2(ED_ICON, ED_ICON)),
                        );
                        let (label, _) =
                            ui.allocate_exact_size(vec2(ED_LABEL_W, TOGGLE_H), Sense::hover());
                        ui.painter().text(
                            label.left_center(),
                            Align2::LEFT_CENTER,
                            "Modo edición",
                            FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                        toggle_switch(ui, &mut options.edit_mode);
                    });
                });
        });
}

/// Modo edición sobre la barra inferior (`bar` = su rectángulo en pantalla): una capa encima que se traga los clics (así
/// los botones de la barra no se activan), la elige o la suelta, y el panel de propiedades cuando está elegida.
pub(super) fn edit_bar(ctx: &egui::Context, options: &mut OptionsPanel, bar: Rect) {
    if !options.edit_mode {
        return;
    }
    let selected = options.edit_selected == Some(EditTarget::BottomBar);
    #[allow(deprecated)]
    let id = egui::Id::new("edit_bottom_bar");
    let mut clicked = false;
    let (mut drag_started, mut dragging, mut drag_stopped) = (false, false, false);
    let options_corner = options.bottom_bar.corner();
    // Sin `constrain`: sigue a la barra exacta de este cuadro (con constrain usaría el tamaño del cuadro anterior).
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(bar.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(bar.size(), Sense::click_and_drag());
            // El contorno copia las esquinas redondeadas de la barra para que la siga exacta.
            let corner = options_corner;
            if selected {
                ui.painter().rect_stroke(
                    rect,
                    corner,
                    Stroke::new(2.0, ACCENT),
                    StrokeKind::Inside,
                );
            } else if response.hovered() {
                ui.painter().rect_stroke(
                    rect,
                    corner,
                    Stroke::new(2.0, Color32::from_white_alpha(110)),
                    StrokeKind::Inside,
                );
            }
            clicked = response.clicked();
            drag_started = response.drag_started();
            dragging = response.dragged();
            drag_stopped = response.drag_stopped();
            // Con la barra elegida sus botones se pueden agarrar y mover.
            if response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if selected && response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        });

    // Arrastrar los botones para acomodarlos: se agarra el que está bajo el mouse y, mientras se arrastra, va ocupando la
    // posición (hueco) donde está el mouse; los demás se corren. El orden queda en `bottom_bar.order`.
    let style = options.bottom_bar;
    let btn = btn_size(style.icon_size);
    let step = btn.x + 8.0;
    // Los botones van pegados a la derecha, dentro del margen de 8 y del borde.
    let start_x = bar.max.x - 8.0 - style.border_w - (4.0 * btn.x + 3.0 * 8.0);
    let slot_of = |x: f32| -> usize { (((x - start_x) / step).floor().max(0.0) as usize).min(3) };
    #[allow(deprecated)]
    let drag_id = egui::Id::new("edit_bar_drag");
    if drag_started {
        if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
            ctx.data_mut(|d| d.insert_temp(drag_id, Some(style.order[slot_of(p.x)])));
            options.edit_selected = Some(EditTarget::BottomBar);
        }
    }
    let held: Option<u8> = ctx.data(|d| d.get_temp::<Option<u8>>(drag_id)).flatten();
    if let (true, Some(b)) = (dragging, held) {
        if let Some(p) = ctx.input(|i| i.pointer.latest_pos()) {
            let target = slot_of(p.x);
            let mut order = options.bottom_bar.order;
            if let Some(cur) = order.iter().position(|&o| o == b) {
                if cur < target {
                    for i in cur..target {
                        order[i] = order[i + 1];
                    }
                } else if cur > target {
                    for i in ((target + 1)..=cur).rev() {
                        order[i] = order[i - 1];
                    }
                }
                order[target] = b;
                options.bottom_bar.order = order;
                // Resalta el hueco donde está el botón que se arrastra.
                #[allow(deprecated)]
                let hi_layer =
                    egui::LayerId::new(egui::Order::Foreground, egui::Id::new("edit_bar_drag_hi"));
                let hi = Rect::from_min_size(
                    egui::pos2(start_x + target as f32 * step, bar.center().y - btn.y / 2.0),
                    btn,
                );
                ctx.layer_painter(hi_layer).rect_filled(
                    hi,
                    CornerRadius::same(6),
                    Color32::from_white_alpha(40),
                );
            }
        }
        ctx.request_repaint();
    }
    if drag_stopped {
        ctx.data_mut(|d| d.insert_temp(drag_id, None::<u8>));
    }
    if clicked {
        options.edit_selected = if selected {
            None
        } else {
            Some(EditTarget::BottomBar)
        };
    }
    if options.edit_selected == Some(EditTarget::BottomBar) {
        bar_properties(ctx, options, bar);
    } else {
        // Soltada la barra, sus selectores de color también se cierran.
        close_pickers(ctx);
    }
}

/// Fila del panel con el nombre a la izquierda y, a la derecha, una muestra del color (con su opacidad sobre un fondo de
/// cuadros). Al tocarla se abre el selector de color de ese elemento. Devuelve true si se tocó. `active` = su selector
/// está abierto (la muestra queda con un aro azul).
fn swatch_row(ui: &mut egui::Ui, label: &str, rgba: [u8; 4], active: bool) -> bool {
    let (row, _) = ui.allocate_exact_size(vec2(PANEL_W, TOGGLE_H), Sense::hover());
    ui.painter().text(
        row.left_center(),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        Color32::WHITE,
    );
    let sw = Rect::from_center_size(
        egui::pos2(row.right() - 22.0, row.center().y),
        vec2(44.0, 22.0),
    );
    let c = Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]);
    let p = ui.painter();
    pill_checker(p, sw, 3.5);
    pill_gradient(p, sw, &[c, c]);
    if active {
        p.rect_stroke(
            sw.expand(1.0),
            CornerRadius::same(12),
            Stroke::new(2.0, ACCENT),
            StrokeKind::Outside,
        );
    }
    ui.interact(sw, ui.id().with(label), Sense::click())
        .clicked()
}

/// Los tres selectores de color de la barra (fondo, borde, iconos) viven en la memoria de egui entre cuadros.
fn pickers_key() -> egui::Id {
    #[allow(deprecated)]
    let id = egui::Id::new("bar_color_pickers");
    id
}

fn picker_id(i: usize) -> egui::Id {
    #[allow(deprecated)]
    let id = egui::Id::new(("bar_color_picker", i));
    id
}

/// Abre el selector `i` y cierra los otros; si ya estaba abierto, lo cierra.
fn toggle_picker(pickers: &mut [super::panel_color::ColorPanel; 3], i: usize) {
    let was = pickers[i].open;
    for p in pickers.iter_mut() {
        p.open = false;
    }
    pickers[i].open = !was;
}

/// Cierra los selectores (al soltar la barra o salir del modo edición).
fn close_pickers(ctx: &egui::Context) {
    let key = pickers_key();
    if let Some(mut p) = ctx.data(|d| d.get_temp::<[super::panel_color::ColorPanel; 3]>(key)) {
        if p.iter().any(|x| x.open) {
            for x in p.iter_mut() {
                x.open = false;
            }
            ctx.data_mut(|d| d.insert_temp(key, p));
        }
    }
}

/// Título de un deslizador: el nombre a la izquierda y el valor a la derecha.
fn slider_head(ui: &mut egui::Ui, label: &str, value: String) {
    let (head, _) = ui.allocate_exact_size(vec2(PANEL_W, 20.0), Sense::hover());
    ui.painter().text(
        head.left_center(),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.0),
        Color32::WHITE,
    );
    ui.painter().text(
        head.right_center(),
        Align2::RIGHT_CENTER,
        value,
        FontId::proportional(13.0),
        Color32::WHITE,
    );
}

/// Deslizador gris con la parte llena en azul (0..1). Devuelve true si cambió.
fn blue_slider(ui: &mut egui::Ui, id: &str, frac: &mut f32) -> bool {
    let (row, _) = ui.allocate_exact_size(vec2(PANEL_W, 28.0), Sense::hover());
    let f0 = *frac;
    let e = slider_row_with(
        ui,
        row,
        ui.id().with(id),
        "",
        0.0,
        frac,
        0.01,
        move |p, r| {
            pill_gradient(p, r, &[Color32::from_gray(205), Color32::from_gray(205)]);
            let w = r.width() * f0;
            if w >= r.height() {
                pill_gradient(
                    p,
                    Rect::from_min_max(r.min, egui::pos2(r.left() + w, r.max.y)),
                    &[SLIDER_BLUE, SLIDER_BLUE],
                );
            }
        },
    );
    e.changed
}

/// Panel de propiedades de la barra inferior: sale justo encima de ella. Los cambios se ven al instante.
fn bar_properties(ctx: &egui::Context, options: &mut OptionsPanel, _bar: Rect) {
    let mut s = options.bottom_bar;
    let mut reset = false;
    // Selectores de color: se sacan de la memoria, se usan en este cuadro y se vuelven a guardar al final.
    let pk = pickers_key();
    let mut pickers: [super::panel_color::ColorPanel; 3] =
        ctx.data(|d| d.get_temp(pk)).unwrap_or_default();
    #[allow(deprecated)]
    let id = egui::Id::new("edit_bottom_bar_props");
    // Posición FIJA: encima de la barra más alta y más gruesa posible. Si siguiera a `bar.top()`, el panel subiría y
    // bajaría mientras se arrastran Altura o Grosor del borde (la barra cambia de tamaño), y los deslizadores se
    // moverían bajo el mouse: eso era el parpadeo. Centrado en la ventana, igual que la barra.
    let screen = ctx.content_rect();
    let panel_anchor = egui::pos2(screen.center().x, screen.bottom() - MAX_BAR_H - 10.0);
    let out = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(panel_anchor)
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;

                    // Título y "Restablecer".
                    let (head, _) = ui.allocate_exact_size(vec2(PANEL_W, 24.0), Sense::hover());
                    for dx in [0.0, 0.6] {
                        ui.painter().text(
                            head.left_center() + vec2(dx, 0.0),
                            Align2::LEFT_CENTER,
                            "Barra inferior",
                            FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                    }
                    let reset_rect =
                        Rect::from_min_max(egui::pos2(head.right() - 90.0, head.top()), head.max);
                    let r = ui.interact(reset_rect, ui.id().with("reset"), Sense::click());
                    let c = if r.hovered() {
                        Color32::WHITE
                    } else {
                        Color32::from_gray(190)
                    };
                    ui.painter().text(
                        reset_rect.right_center(),
                        Align2::RIGHT_CENTER,
                        "Restablecer",
                        FontId::proportional(13.0),
                        c,
                    );
                    reset = r.clicked();
                    ui.add_space(4.0);

                    // Tamaño: anchura (% de la ventana) y altura.
                    // El mínimo es lo que miden los botones, como fracción de la ventana de este cuadro.
                    let min_frac = (bar_min_width(s.border_w, s.icon_size)
                        / ctx.content_rect().width().max(1.0))
                    .clamp(0.0, 0.99);
                    // Lo guardado NO se corrige aquí: solo se muestra con el mínimo aplicado. Si se guardara el mínimo de
                    // este momento (que depende del grosor del borde), al bajar luego el borde la barra se quedaría más
                    // ancha que los botones y dejaría un vacío. Un 0 guardado significa "lo mínimo que caben los botones".
                    let shown_w = s.width_frac.max(min_frac);
                    let mut wf = ((shown_w - min_frac) / (1.0 - min_frac)).clamp(0.0, 1.0);
                    slider_head(
                        ui,
                        "Anchura",
                        format!("{}%", (shown_w * 100.0).round() as i32),
                    );
                    if blue_slider(ui, "anchura", &mut wf) {
                        s.width_frac = if wf < 0.005 {
                            0.0
                        } else {
                            min_frac + wf * (1.0 - min_frac)
                        };
                    }
                    // La altura es la total (borde incluido); el mínimo lo dan los botones más el borde.
                    let min_h = bar_min_height(s.border_w, s.icon_size);
                    let shown_h = s.height.max(min_h);
                    let mut hf = (shown_h - min_h) / (MAX_BAR_H - min_h);
                    slider_head(ui, "Altura", format!("{} px", shown_h.round() as i32));
                    if blue_slider(ui, "altura", &mut hf) {
                        s.height = (min_h + hf * (MAX_BAR_H - min_h)).round();
                    }

                    // Color de fondo (con su opacidad: el selector trae la barra de opacidad, así que ya no hay un
                    // deslizador de transparencia aparte).
                    if swatch_row(
                        ui,
                        "Color",
                        [s.rgb[0], s.rgb[1], s.rgb[2], s.alpha],
                        pickers[0].open,
                    ) {
                        toggle_picker(&mut pickers, 0);
                    }

                    // Color de los iconos de los botones.
                    if swatch_row(ui, "Color de los iconos", s.icon, pickers[2].open) {
                        toggle_picker(&mut pickers, 2);
                    }

                    // Tamaño de los iconos: el botón crece con él (y con él, los mínimos de la barra).
                    let mut isz = (s.icon_size - MIN_ICON) / (MAX_ICON - MIN_ICON);
                    slider_head(
                        ui,
                        "Tamaño de los iconos",
                        format!("{} px", s.icon_size.round() as i32),
                    );
                    if blue_slider(ui, "iconos", &mut isz) {
                        s.icon_size = (MIN_ICON + isz * (MAX_ICON - MIN_ICON)).round();
                    }

                    // Borde: color (con su opacidad) y grosor.
                    if swatch_row(ui, "Color del borde", s.border, pickers[1].open) {
                        toggle_picker(&mut pickers, 1);
                    }
                    let mut bw = s.border_w / MAX_BORDER;
                    slider_head(ui, "Grosor del borde", format!("{:.1} px", s.border_w));
                    if blue_slider(ui, "borde", &mut bw) {
                        s.border_w = (bw * MAX_BORDER * 2.0).round() / 2.0;
                    }

                    // Esquinas redondeadas.
                    let mut rad = s.radius as f32 / MAX_RADIUS;
                    slider_head(ui, "Esquinas", format!("{} px", s.radius));
                    if blue_slider(ui, "esquinas", &mut rad) {
                        s.radius = (rad * MAX_RADIUS).round() as u8;
                    }
                });
        });
    if reset {
        s = BarStyle::default();
    }
    let panel = out.response.rect;

    // Selector del color elegido: sale a la izquierda del panel, alineado por abajo. Cada uno trabaja con un color y su
    // opacidad (0..1), que aquí se pasan de y a los bytes de la barra.
    let place = |i: usize| {
        Some((
            picker_id(i),
            egui::pos2(panel.left() - 8.0, panel.bottom()),
            Align2::RIGHT_BOTTOM,
        ))
    };
    let mut col = s.rgb;
    let mut op = s.alpha as f32 / 255.0;
    if pickers[0].show_in(ctx, &mut col, &mut op, place(0)) {
        s.rgb = col;
        s.alpha = (op * 255.0).round() as u8;
    }
    let mut col = [s.border[0], s.border[1], s.border[2]];
    let mut op = s.border[3] as f32 / 255.0;
    if pickers[1].show_in(ctx, &mut col, &mut op, place(1)) {
        s.border = [col[0], col[1], col[2], (op * 255.0).round() as u8];
    }
    let mut col = [s.icon[0], s.icon[1], s.icon[2]];
    let mut op = s.icon[3] as f32 / 255.0;
    if pickers[2].show_in(ctx, &mut col, &mut op, place(2)) {
        s.icon = [col[0], col[1], col[2], (op * 255.0).round() as u8];
    }
    ctx.data_mut(|d| d.insert_temp(pk, pickers));
    options.bottom_bar = s;

    // Un clic en el vacío (ni la barra, ni este panel, ni su selector de color) suelta la barra.
    if ctx.input(|i| i.pointer.primary_pressed()) {
        if let Some(p) = ctx.input(|i| i.pointer.interact_pos()) {
            let vacio = ctx
                .layer_id_at(p)
                .is_none_or(|l| l.order == egui::Order::Background);
            if vacio && !panel.contains(p) {
                options.edit_selected = None;
            }
        }
    }
}
