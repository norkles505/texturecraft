//! Modo edición: se enciende con el interruptor de la ventana de Ajustes. Mientras está encendido:
//! - aparece una pastilla flotante (tres rayas, nombre e interruptor para apagarlo; se puede arrastrar),
//! - no se puede pintar (ver `UiLayer::edit_mode` y `main.rs`),
//! - la barra inferior deja de funcionar como barra: al tocarla se elige (queda con un contorno azul) y sale encima el
//!   panel de propiedades con su color, transparencia, borde y esquinas. Tocarla otra vez, o tocar el vacío, la suelta.
//!
//! Por ahora solo se puede editar la barra inferior (prueba). Su estilo se guarda solo en un archivo de texto
//! (`BarStyle::config_path`) cada vez que cambia y se suelta el mouse, y se lee al abrir el programa.

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
/// 45 = la mitad de la barra más alta (90): con eso la barra queda como una píldora completa.
const MAX_RADIUS: f32 = 45.0;
/// Cuánto se puede despegar la barra del borde de abajo y cuánto puede difuminarse su sombra, en puntos.
const MAX_LIFT: f32 = 40.0;
const MAX_SHADOW: f32 = 40.0;
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
    /// Un botón de la barra inferior (su id `BTN_*`): se elige tocándolo con la barra en modo edición.
    Button(u8),
}

/// Cuántos selectores de color tiene el panel de la barra: fondo, borde, iconos, botón activo, insignia y línea curva.
const N_PICKERS: usize = 6;
/// Grosor mínimo y máximo de la línea curva del botón activo.
const MIN_CURVE_W: f32 = 1.0;
const MAX_CURVE_W: f32 = 6.0;

/// Lo que se puede activar en cada botón de la barra inferior (se guarda por id `BTN_*`). Todo apagado = el botón de
/// siempre.
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) struct BtnStyle {
    /// Su nombre escrito debajo del icono.
    pub(super) label: bool,
    /// Recuadro redondeado detrás mientras su panel está abierto.
    pub(super) active_bg: bool,
    /// Puntito debajo mientras su panel está abierto.
    pub(super) dot: bool,
    /// Insignia roja en la esquina del icono.
    pub(super) badge: bool,
}

/// Nombre de un botón de la barra (texto bajo su icono y título de su panel).
pub(super) fn btn_name(id: u8) -> &'static str {
    match id {
        BTN_OPTIONS => "Opciones",
        BTN_CUBE => "Mallas",
        BTN_LAYERS => "Capas",
        _ => "Menú",
    }
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
    /// Separación entre la barra y el borde de abajo de la ventana (0 = pegada). Con separación la barra "flota".
    pub(super) lift: f32,
    /// Desenfoque de la sombra de la barra, en puntos (0 = sin sombra).
    pub(super) shadow: u8,
    /// Qué lleva cada botón (texto, recuadro, puntito, insignia), indexado por su id `BTN_*`.
    pub(super) btn: [BtnStyle; 4],
    /// Color de los botones mientras su panel está abierto (icono, texto, puntito y recuadro). Por defecto, el azul de
    /// siempre.
    pub(super) active: [u8; 4],
    /// Color de la insignia de los botones.
    pub(super) badge_color: [u8; 4],
    /// Línea curva de acento que rodea el botón activo (y sigue por el borde de abajo): si se muestra, su color y grosor.
    pub(super) curve_on: bool,
    pub(super) curve_color: [u8; 4],
    pub(super) curve_w: f32,
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
            lift: 0.0,
            shadow: 0,
            btn: [BtnStyle::default(); 4],
            active: ACCENT.to_array(),
            badge_color: [226, 48, 62, 255],
            curve_on: false,
            curve_color: [226, 48, 62, 255],
            curve_w: 1.5,
        }
    }
}

impl BarStyle {
    /// Un tema completo (colores, forma, tamaño, textos, puntito...) para elegir de un toque. Conserva el orden de los
    /// botones (`order`) que el usuario ya acomodó. `0` = el estilo de siempre.
    pub(super) fn theme(i: usize, order: [u8; 4]) -> Self {
        let base = BarStyle {
            order,
            ..BarStyle::default()
        };
        // Los cuatro botones con lo mismo: texto, recuadro, puntito, insignia.
        let all = |label, active_bg, dot, badge| {
            [BtnStyle {
                label,
                active_bg,
                dot,
                badge,
            }; 4]
        };
        match i {
            // Claro: píldora blanca flotante, iconos grises y el rojo como color activo.
            1 => BarStyle {
                rgb: [250, 250, 250],
                alpha: 245,
                border: [0, 0, 0, 22],
                border_w: 1.0,
                radius: 45,
                icon: [70, 70, 72, 255],
                active: [230, 0, 35, 255],
                width_frac: 0.5,
                height: 60.0,
                lift: 14.0,
                shadow: 24,
                btn: all(true, false, true, false),
                ..base
            },
            // Rojo: píldora roja, iconos blancos y recuadro claro en el botón activo.
            2 => BarStyle {
                rgb: [200, 16, 46],
                alpha: 255,
                border: [255, 255, 255, 0],
                border_w: 0.0,
                radius: 45,
                icon: [255, 255, 255, 200],
                active: [255, 255, 255, 255],
                badge_color: [255, 255, 255, 255],
                width_frac: 0.5,
                height: 60.0,
                lift: 14.0,
                shadow: 24,
                btn: all(true, true, true, false),
                ..base
            },
            // Oscuro: píldora casi negra, iconos grises y el rojo como color activo.
            3 => BarStyle {
                rgb: [38, 38, 40],
                alpha: 255,
                border: [255, 255, 255, 14],
                border_w: 1.0,
                radius: 45,
                icon: [190, 190, 192, 255],
                active: [226, 48, 62, 255],
                width_frac: 0.5,
                height: 60.0,
                lift: 14.0,
                shadow: 28,
                curve_on: true,
                curve_color: [226, 48, 62, 255],
                btn: all(true, false, true, false),
                ..base
            },
            _ => base,
        }
    }

    /// Alto extra de los botones si alguno muestra su nombre debajo del icono (todos crecen igual para quedar alineados).
    pub(super) fn label_extra(&self) -> f32 {
        if self.btn.iter().any(|b| b.label) {
            LABEL_H
        } else {
            0.0
        }
    }

    pub(super) fn active_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.active[0], self.active[1], self.active[2], self.active[3])
    }

    pub(super) fn badge_color32(&self) -> Color32 {
        let b = self.badge_color;
        Color32::from_rgba_unmultiplied(b[0], b[1], b[2], b[3])
    }

    pub(super) fn curve_color32(&self) -> Color32 {
        let c = self.curve_color;
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    /// Margen horizontal entre el borde de la barra y sus botones: 8 y, con esquinas muy redondeadas, más, para que los
    /// botones de los extremos no se salgan de la curva.
    pub(super) fn margin_x(&self) -> f32 {
        8.0 + (self.radius as f32 * 0.45).round()
    }

    /// La sombra de la barra (para dibujarla justo antes que el fondo). Cae un poco hacia abajo.
    pub(super) fn shadow_shape(&self, rect: Rect) -> egui::epaint::RectShape {
        egui::Shadow {
            offset: [0, 3],
            blur: self.shadow,
            spread: 0,
            color: Color32::from_black_alpha(120),
        }
        .as_shape(rect, self.corner())
    }

    /// Archivo donde se guarda el estilo: `$XDG_CONFIG_HOME/texturecraft/interfaz.cfg` (o `~/.config/...`).
    fn config_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
        Some(base.join("texturecraft").join("interfaz.cfg"))
    }

    /// Lee el estilo guardado; si no hay archivo (o algo no se entiende) queda el de siempre en esa parte.
    pub(super) fn load() -> Self {
        match Self::config_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(text) => Self::from_text(&text),
            None => Self::default(),
        }
    }

    /// Guarda el estilo (un error al escribir solo se avisa en el log: la interfaz sigue funcionando).
    pub(super) fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, self.to_text()) {
            log::warn!("no se pudo guardar la interfaz en {}: {e}", path.display());
        }
    }

    /// Una línea `nombre=valor,valor` por propiedad.
    fn to_text(&self) -> String {
        let (c, b, i, o, a) = (self.rgb, self.border, self.icon, self.order, self.active);
        let mut t = format!(
            "rgb={},{},{}\nalpha={}\nborder={},{},{},{}\nborder_w={}\nradius={}\nicon={},{},{},{}\nicon_size={}\n\
             order={},{},{},{}\nwidth_frac={}\nheight={}\nlift={}\nshadow={}\nactive={},{},{},{}\n",
            c[0], c[1], c[2], self.alpha, b[0], b[1], b[2], b[3], self.border_w, self.radius, i[0], i[1], i[2], i[3],
            self.icon_size, o[0], o[1], o[2], o[3], self.width_frac, self.height, self.lift, self.shadow,
            a[0], a[1], a[2], a[3],
        );
        let bc = self.badge_color;
        t.push_str(&format!("badge_color={},{},{},{}\n", bc[0], bc[1], bc[2], bc[3]));
        let cc = self.curve_color;
        t.push_str(&format!(
            "curve={},{}\ncurve_color={},{},{},{}\n",
            self.curve_on as u8, self.curve_w, cc[0], cc[1], cc[2], cc[3]
        ));
        // Un botón por línea: texto, recuadro, puntito, insignia (1 = encendido).
        for (k, s) in self.btn.iter().enumerate() {
            t.push_str(&format!(
                "btn{k}={},{},{},{}\n",
                s.label as u8, s.active_bg as u8, s.dot as u8, s.badge as u8
            ));
        }
        t
    }

    /// Lo contrario de `to_text`. Cada valor se limita a su rango, así un archivo a medio editar no rompe la barra.
    fn from_text(text: &str) -> Self {
        let mut s = BarStyle::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let n: Vec<f32> = value.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            let byte = |i: usize| n[i].clamp(0.0, 255.0) as u8;
            match (key.trim(), n.len()) {
                ("rgb", 3) => s.rgb = [byte(0), byte(1), byte(2)],
                ("alpha", 1) => s.alpha = byte(0),
                ("border", 4) => s.border = [byte(0), byte(1), byte(2), byte(3)],
                ("border_w", 1) => s.border_w = n[0].clamp(0.0, MAX_BORDER),
                ("radius", 1) => s.radius = n[0].clamp(0.0, MAX_RADIUS) as u8,
                ("icon", 4) => s.icon = [byte(0), byte(1), byte(2), byte(3)],
                ("icon_size", 1) => s.icon_size = n[0].clamp(MIN_ICON, MAX_ICON),
                ("order", 4) => {
                    // Tiene que ser una permutación de los cuatro botones; si no, se queda el orden de siempre.
                    let o = [byte(0), byte(1), byte(2), byte(3)];
                    let mut seen = [false; 4];
                    if o.iter().all(|&x| x < 4 && !std::mem::replace(&mut seen[x as usize], true)) {
                        s.order = o;
                    }
                }
                ("width_frac", 1) => s.width_frac = n[0].clamp(0.0, 1.0),
                ("height", 1) => s.height = n[0].clamp(0.0, MAX_BAR_H),
                ("lift", 1) => s.lift = n[0].clamp(0.0, MAX_LIFT),
                ("shadow", 1) => s.shadow = n[0].clamp(0.0, MAX_SHADOW) as u8,
                ("active", 4) => s.active = [byte(0), byte(1), byte(2), byte(3)],
                ("badge_color", 4) => s.badge_color = [byte(0), byte(1), byte(2), byte(3)],
                ("curve", 2) => {
                    s.curve_on = n[0] > 0.5;
                    s.curve_w = n[1].clamp(MIN_CURVE_W, MAX_CURVE_W);
                }
                ("curve_color", 4) => s.curve_color = [byte(0), byte(1), byte(2), byte(3)],
                (k, 4) if k.starts_with("btn") => {
                    if let Ok(i) = k[3..].parse::<usize>() {
                        if i < 4 {
                            s.btn[i] = BtnStyle {
                                label: n[0] > 0.5,
                                active_bg: n[1] > 0.5,
                                dot: n[2] > 0.5,
                                badge: n[3] > 0.5,
                            };
                        }
                    }
                }
                _ => {}
            }
        }
        s
    }

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
    // Guarda el estilo de la barra en cuanto cambia y se suelta el mouse (no en cada cuadro de un arrastre). Va antes del
    // `return` de abajo porque también hay que guardar lo que se cambió justo antes de apagar el modo edición.
    if options.bottom_bar != options.bottom_bar_saved && !ctx.input(|i| i.pointer.any_down()) {
        options.bottom_bar.save();
        options.bottom_bar_saved = options.bottom_bar;
    }
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
    // Si hay un botón elegido: dónde está cada hueco, para dibujarle su contorno (mismas cuentas que más abajo).
    let selected_btn = match options.edit_selected {
        Some(EditTarget::Button(b)) => Some(b),
        _ => None,
    };
    let style0 = options.bottom_bar;
    let btn0 = btn_size(style0.icon_size);
    let btn_h0 = btn0.y + style0.label_extra();
    let step0 = btn0.x + 8.0;
    let start_x0 = bar.max.x - style0.margin_x() - style0.border_w - (4.0 * btn0.x + 3.0 * 8.0);
    #[allow(deprecated)]
    let id = egui::Id::new("edit_bottom_bar");
    let mut clicked = false;
    let mut click_pos: Option<egui::Pos2> = None;
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
            if let Some(b) = selected_btn {
                // Botón elegido: el contorno azul rodea solo ese botón.
                let slot = style0.order.iter().position(|&o| o == b).unwrap_or(0);
                let r = Rect::from_min_size(
                    egui::pos2(start_x0 + slot as f32 * step0, bar.center().y - btn_h0 / 2.0),
                    vec2(btn0.x, btn_h0),
                );
                ui.painter().rect_stroke(
                    r,
                    CornerRadius::same(10),
                    Stroke::new(2.0, ACCENT),
                    StrokeKind::Inside,
                );
            } else if selected {
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
            click_pos = response.interact_pointer_pos();
            drag_started = response.drag_started();
            dragging = response.dragged();
            drag_stopped = response.drag_stopped();
            // Con la barra elegida sus botones se pueden agarrar y mover.
            if response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            } else if (selected || selected_btn.is_some()) && response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        });

    // Arrastrar los botones para acomodarlos: se agarra el que está bajo el mouse y, mientras se arrastra, va ocupando la
    // posición (hueco) donde está el mouse; los demás se corren. El orden queda en `bottom_bar.order`.
    let style = options.bottom_bar;
    let btn = btn_size(style.icon_size);
    let step = btn.x + 8.0;
    // Los botones van pegados a la derecha, dentro del margen de 8 y del borde.
    let start_x = bar.max.x - style.margin_x() - style.border_w - (4.0 * btn.x + 3.0 * 8.0);
    let slot_of = |x: f32| -> usize { (((x - start_x) / step).floor().max(0.0) as usize).min(3) };
    #[allow(deprecated)]
    let drag_id = egui::Id::new("edit_bar_drag");
    if drag_started {
        if let Some(p) = ctx.input(|i| i.pointer.press_origin()) {
            let grabbed = style.order[slot_of(p.x)];
            ctx.data_mut(|d| d.insert_temp(drag_id, Some(grabbed)));
            // Agarrar un botón para moverlo también lo elige.
            options.edit_selected = Some(EditTarget::Button(grabbed));
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
                let hb = vec2(btn.x, btn.y + style.label_extra());
                let hi = Rect::from_min_size(
                    egui::pos2(start_x + target as f32 * step, bar.center().y - hb.y / 2.0),
                    hb,
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
        // Un clic sobre un botón elige ese botón; en cualquier otro punto de la barra, la barra. Volver a tocar lo que ya
        // estaba elegido lo suelta.
        let btn_h = btn.y + style.label_extra();
        let on_button = click_pos.and_then(|p| {
            let slot = ((p.x - start_x) / step).floor();
            let inside = slot >= 0.0
                && slot < 4.0
                && (p.x - start_x - slot * step) <= btn.x
                && (p.y - bar.center().y).abs() <= btn_h / 2.0;
            inside.then(|| style.order[slot as usize])
        });
        let target = match on_button {
            Some(b) => EditTarget::Button(b),
            None => EditTarget::BottomBar,
        };
        options.edit_selected = if options.edit_selected == Some(target) {
            None
        } else {
            Some(target)
        };
    }
    match options.edit_selected {
        Some(EditTarget::BottomBar) => bar_properties(ctx, options, bar),
        Some(EditTarget::Button(b)) => {
            // El panel del botón no usa los selectores de color de la barra: se cierran.
            close_pickers(ctx);
            button_properties(ctx, options, b);
        }
        // Soltada la barra, sus selectores de color también se cierran.
        None => close_pickers(ctx),
    }
}

/// Posición del panel de propiedades (la misma para el de la barra y el de un botón): encima de la barra más alta
/// posible (con su separación de abajo), centrado como la barra. Si siguiera a `bar.top()`, el panel subiría y bajaría
/// mientras se arrastran Altura, Grosor o Separación (la barra cambia de tamaño o de lugar) y los deslizadores se
/// moverían bajo el mouse: eso era el parpadeo. Mientras el mouse está apretado se conserva la altura del cuadro
/// anterior; al soltar, el panel se acomoda a la barra nueva.
fn props_anchor(ctx: &egui::Context, lift: f32) -> egui::Pos2 {
    let screen = ctx.content_rect();
    #[allow(deprecated)]
    let y_key = egui::Id::new("edit_bottom_bar_props_y");
    let live_y = screen.bottom() - lift - MAX_BAR_H - 10.0;
    let y = if ctx.input(|i| i.pointer.any_down()) {
        ctx.data(|d| d.get_temp::<f32>(y_key)).unwrap_or(live_y)
    } else {
        live_y
    };
    ctx.data_mut(|d| d.insert_temp(y_key, y));
    egui::pos2(screen.center().x, y)
}

/// Fila del panel con el nombre a la izquierda y un interruptor a la derecha.
fn toggle_row(ui: &mut egui::Ui, label: &str, on: &mut bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let (r, _) = ui.allocate_exact_size(vec2(PANEL_W - TOGGLE_W, TOGGLE_H), Sense::hover());
        ui.painter().text(
            r.left_center(),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(14.0),
            Color32::WHITE,
        );
        toggle_switch(ui, on);
    });
}

/// Panel de propiedades de UN botón de la barra inferior (el que se tocó en modo edición): sale justo encima de la barra.
fn button_properties(ctx: &egui::Context, options: &mut OptionsPanel, btn_id: u8) {
    let mut s = options.bottom_bar.btn[btn_id as usize];
    #[allow(deprecated)]
    let id = egui::Id::new("edit_bottom_bar_button_props");
    let out = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(props_anchor(ctx, options.bottom_bar.lift))
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    let (head, _) = ui.allocate_exact_size(vec2(PANEL_W, 24.0), Sense::hover());
                    for dx in [0.0, 0.6] {
                        ui.painter().text(
                            head.left_center() + vec2(dx, 0.0),
                            Align2::LEFT_CENTER,
                            format!("Botón: {}", btn_name(btn_id)),
                            FontId::proportional(16.0),
                            Color32::WHITE,
                        );
                    }
                    ui.add_space(2.0);
                    toggle_row(ui, "Texto debajo del icono", &mut s.label);
                    toggle_row(ui, "Recuadro al activarse", &mut s.active_bg);
                    toggle_row(ui, "Puntito al activarse", &mut s.dot);
                    toggle_row(ui, "Insignia roja", &mut s.badge);
                });
        });
    options.bottom_bar.btn[btn_id as usize] = s;

    // Un clic en el vacío (ni la barra, ni este panel) suelta el botón.
    let panel = out.response.rect;
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

/// Los cuatro selectores de color de la barra (fondo, borde, iconos, botón activo) viven en la memoria de egui entre
/// cuadros.
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
fn toggle_picker(pickers: &mut [super::panel_color::ColorPanel; N_PICKERS], i: usize) {
    let was = pickers[i].open;
    for p in pickers.iter_mut() {
        p.open = false;
    }
    pickers[i].open = !was;
}

/// Cierra los selectores (al soltar la barra o salir del modo edición).
fn close_pickers(ctx: &egui::Context) {
    let key = pickers_key();
    if let Some(mut p) = ctx.data(|d| d.get_temp::<[super::panel_color::ColorPanel; N_PICKERS]>(key)) {
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

/// Nombres de los temas (el índice es el que recibe `BarStyle::theme`).
const THEME_NAMES: [&str; 4] = ["Original", "Claro", "Rojo", "Oscuro"];

/// Fila de temas: cuatro botoncitos. Devuelve el índice del que se tocó.
fn theme_row(ui: &mut egui::Ui) -> Option<usize> {
    let (row, _) = ui.allocate_exact_size(vec2(PANEL_W, 26.0), Sense::hover());
    let gap = 6.0;
    let w = (PANEL_W - gap * 3.0) / 4.0;
    let mut picked = None;
    for (i, name) in THEME_NAMES.iter().enumerate() {
        let r = Rect::from_min_size(
            egui::pos2(row.left() + i as f32 * (w + gap), row.top()),
            vec2(w, row.height()),
        );
        let resp = ui.interact(r, ui.id().with(("tema", i)), Sense::click());
        let fill = if resp.hovered() {
            Color32::from_white_alpha(46)
        } else {
            Color32::from_white_alpha(22)
        };
        ui.painter().rect_filled(r, CornerRadius::same(13), fill);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            *name,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
        if resp.clicked() {
            picked = Some(i);
        }
    }
    picked
}

/// Pestañas del panel de la barra: agrupan las propiedades para que el panel no crezca sin fin al agregar más.
const TAB_NAMES: [&str; 3] = ["Forma", "Color", "Botones"];

/// Fila de pestañas (la elegida va resaltada). Devuelve la que se tocó.
fn tab_row(ui: &mut egui::Ui, names: &[&str], current: usize) -> Option<usize> {
    let (row, _) = ui.allocate_exact_size(vec2(PANEL_W, 28.0), Sense::hover());
    let gap = 6.0;
    let w = (PANEL_W - gap * (names.len() as f32 - 1.0)) / names.len() as f32;
    let mut picked = None;
    for (i, name) in names.iter().enumerate() {
        let r = Rect::from_min_size(
            egui::pos2(row.left() + i as f32 * (w + gap), row.top()),
            vec2(w, row.height()),
        );
        let resp = ui.interact(r, ui.id().with(("pestana", i)), Sense::click());
        let fill = if i == current {
            Color32::from_white_alpha(70)
        } else if resp.hovered() {
            Color32::from_white_alpha(40)
        } else {
            Color32::from_white_alpha(18)
        };
        ui.painter().rect_filled(r, CornerRadius::same(14), fill);
        ui.painter().text(
            r.center(),
            Align2::CENTER_CENTER,
            *name,
            FontId::proportional(13.0),
            Color32::WHITE,
        );
        if resp.clicked() {
            picked = Some(i);
        }
    }
    picked
}

/// Panel de propiedades de la barra inferior: sale justo encima de ella. Los cambios se ven al instante.
fn bar_properties(ctx: &egui::Context, options: &mut OptionsPanel, _bar: Rect) {
    let mut s = options.bottom_bar;
    let mut reset = false;
    let mut theme: Option<usize> = None;
    // Pestaña abierta (se recuerda entre cuadros).
    #[allow(deprecated)]
    let tab_key = egui::Id::new("bar_props_tab");
    let mut tab: usize = ctx.data(|d| d.get_temp(tab_key)).unwrap_or(0).min(TAB_NAMES.len() - 1);
    // Selectores de color: se sacan de la memoria, se usan en este cuadro y se vuelven a guardar al final.
    let pk = pickers_key();
    let mut pickers: [super::panel_color::ColorPanel; N_PICKERS] =
        ctx.data(|d| d.get_temp(pk)).unwrap_or_default();
    #[allow(deprecated)]
    let id = egui::Id::new("edit_bottom_bar_props");
    // Posición del panel: ver `props_anchor`.
    let panel_anchor = props_anchor(ctx, s.lift);
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

                    // Temas: estilos completos para elegir de un toque (lo que se cambie después se puede afinar).
                    slider_head(ui, "Tema", String::new());
                    theme = theme_row(ui);
                    ui.add_space(4.0);

                    // Pestañas. Al cambiar de pestaña se cierran los selectores de color (quedarían huérfanos).
                    if let Some(t) = tab_row(ui, &TAB_NAMES, tab) {
                        if t != tab {
                            tab = t;
                            for p in pickers.iter_mut() {
                                p.open = false;
                            }
                        }
                    }
                    ui.add_space(2.0);

                    // ── Pestaña Forma: anchura, altura, esquinas, separación y sombra ──
                    if tab == 0 {
                    // Tamaño: anchura (% de la ventana) y altura.
                    // El mínimo es lo que miden los botones, como fracción de la ventana de este cuadro.
                    let min_frac = (bar_min_width_m(s.border_w, s.icon_size, s.margin_x())
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
                    let min_h = bar_min_height_l(s.border_w, s.icon_size, s.label_extra());
                    let shown_h = s.height.max(min_h);
                    let mut hf = (shown_h - min_h) / (MAX_BAR_H - min_h);
                    slider_head(ui, "Altura", format!("{} px", shown_h.round() as i32));
                    if blue_slider(ui, "altura", &mut hf) {
                        s.height = (min_h + hf * (MAX_BAR_H - min_h)).round();
                    }
                    }

                    // ── Pestaña Color: fondo y borde ──
                    if tab == 1 {

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

                    }

                    // ── Pestaña Botones: colores de los iconos, activo e insignia, y tamaño de los iconos ──
                    if tab == 2 {
                    // Color de los iconos de los botones.
                    if swatch_row(ui, "Color de los iconos", s.icon, pickers[2].open) {
                        toggle_picker(&mut pickers, 2);
                    }

                    // Color de los botones mientras su panel está abierto (icono, texto, puntito y recuadro).
                    if swatch_row(ui, "Color del botón activo", s.active, pickers[3].open) {
                        toggle_picker(&mut pickers, 3);
                    }

                    // Color de la insignia de los botones.
                    if swatch_row(ui, "Color de la insignia", s.badge_color, pickers[4].open) {
                        toggle_picker(&mut pickers, 4);
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

                    // Línea curva de acento alrededor del botón activo (color y grosor solo si está encendida).
                    toggle_row(ui, "Línea curva del activo", &mut s.curve_on);
                    if s.curve_on {
                        if swatch_row(ui, "Color de la línea", s.curve_color, pickers[5].open) {
                            toggle_picker(&mut pickers, 5);
                        }
                        let mut cw = (s.curve_w - MIN_CURVE_W) / (MAX_CURVE_W - MIN_CURVE_W);
                        slider_head(ui, "Grosor de la línea", format!("{:.1} px", s.curve_w));
                        if blue_slider(ui, "curva_grosor", &mut cw) {
                            s.curve_w = ((MIN_CURVE_W + cw * (MAX_CURVE_W - MIN_CURVE_W)) * 2.0).round() / 2.0;
                        }
                    }

                    }
                    if tab == 1 {
                    // Borde: color (con su opacidad) y grosor.
                    if swatch_row(ui, "Color del borde", s.border, pickers[1].open) {
                        toggle_picker(&mut pickers, 1);
                    }
                    let mut bw = s.border_w / MAX_BORDER;
                    slider_head(ui, "Grosor del borde", format!("{:.1} px", s.border_w));
                    if blue_slider(ui, "borde", &mut bw) {
                        s.border_w = (bw * MAX_BORDER * 2.0).round() / 2.0;
                    }

                    }
                    if tab == 0 {
                    // Esquinas redondeadas.
                    let mut rad = s.radius as f32 / MAX_RADIUS;
                    slider_head(ui, "Esquinas", format!("{} px", s.radius));
                    if blue_slider(ui, "esquinas", &mut rad) {
                        s.radius = (rad * MAX_RADIUS).round() as u8;
                    }

                    // Separación del borde de abajo de la ventana: con ella la barra queda flotando.
                    let mut lf = s.lift / MAX_LIFT;
                    slider_head(
                        ui,
                        "Separación de abajo",
                        format!("{} px", s.lift.round() as i32),
                    );
                    if blue_slider(ui, "separacion", &mut lf) {
                        s.lift = (lf * MAX_LIFT).round();
                    }

                    // Sombra bajo la barra (0 = sin sombra).
                    let mut sh = s.shadow as f32 / MAX_SHADOW;
                    slider_head(ui, "Sombra", format!("{} px", s.shadow));
                    if blue_slider(ui, "sombra", &mut sh) {
                        s.shadow = (sh * MAX_SHADOW).round() as u8;
                    }
                    }
                });
        });
    ctx.data_mut(|d| d.insert_temp(tab_key, tab));
    if reset {
        s = BarStyle::default();
    }
    if let Some(i) = theme {
        s = BarStyle::theme(i, s.order);
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
    let mut col = [s.active[0], s.active[1], s.active[2]];
    let mut op = s.active[3] as f32 / 255.0;
    if pickers[3].show_in(ctx, &mut col, &mut op, place(3)) {
        s.active = [col[0], col[1], col[2], (op * 255.0).round() as u8];
    }
    let mut col = [s.badge_color[0], s.badge_color[1], s.badge_color[2]];
    let mut op = s.badge_color[3] as f32 / 255.0;
    if pickers[4].show_in(ctx, &mut col, &mut op, place(4)) {
        s.badge_color = [col[0], col[1], col[2], (op * 255.0).round() as u8];
    }
    let mut col = [s.curve_color[0], s.curve_color[1], s.curve_color[2]];
    let mut op = s.curve_color[3] as f32 / 255.0;
    if pickers[5].show_in(ctx, &mut col, &mut op, place(5)) {
        s.curve_color = [col[0], col[1], col[2], (op * 255.0).round() as u8];
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
