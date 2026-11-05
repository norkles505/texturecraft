//! Panel de reglas (el que abre el boton de las reglas de la barra inferior): titulo "Regla" y una barra negra con
//! "OFF" a la izquierda y los tipos de regla a la derecha (recta, circulo y elipse).
//!
//! Elegir una regla la pone en pantalla (linea, aro u ovalo con sus agarraderas) y guia el trazo: ver la seccion
//! "Reglas" mas abajo. Los iconos de los tres tipos son SVG (`assets/icons/regla_*.svg`), como en los demas botones.

use super::*;

/// Medidas del panel (en puntos de egui): ancho (el mismo que el de mallas), margen del marco, alto del titulo y de la
/// barra negra, y ancho de cada celda de la barra.
const RP_W: f32 = 252.0;
const RP_PAD: f32 = 6.0;
const RP_TITLE_H: f32 = 34.0;
const RP_BAR_H: f32 = 44.0;
const RP_CELL_W: f32 = 46.0;
/// Ancho de la celda "OFF" (a la izquierda de la barra).
const RP_OFF_W: f32 = 64.0;

/// Regla elegida en el panel. `Off` = sin regla.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum RuleKind {
    #[default]
    Off,
    /// Regla recta.
    Line,
    /// Regla circular.
    Circle,
    /// Regla elíptica.
    Ellipse,
}

/// Los tres tipos de regla, de izquierda a derecha en la barra, con el texto que sale al pasar el mouse.
const RULES: [(RuleKind, &str); 3] = [
    (RuleKind::Line, "Regla recta"),
    (RuleKind::Circle, "Círculo"),
    (RuleKind::Ellipse, "Elipse"),
];

/// Lado del icono SVG de cada tipo de regla (en puntos de egui).
const RP_ICON: f32 = 24.0;

/// Icono SVG de un tipo de regla, centrado en `c` y teñido con `color`.
fn paint_rule_icon(ui: &egui::Ui, kind: RuleKind, c: egui::Pos2, color: Color32) {
    let icon = match kind {
        RuleKind::Line => egui::include_image!("../../assets/icons/regla_recta.svg"),
        RuleKind::Circle => egui::include_image!("../../assets/icons/regla_circulo.svg"),
        RuleKind::Ellipse => egui::include_image!("../../assets/icons/regla_elipse.svg"),
        RuleKind::Off => return,
    };
    egui::Image::new(icon)
        .tint(color)
        .paint_at(ui, Rect::from_center_size(c, vec2(RP_ICON, RP_ICON)));
}

/// Panel flotante del boton de reglas: aparece justo encima del boton y se cierra al hacer clic fuera de el.
pub(super) fn rules_panel(ctx: &egui::Context, options: &mut OptionsPanel, button: Rect) {
    if !options.rules_open {
        return;
    }
    #[allow(deprecated)]
    let id = egui::Id::new("rules_panel");
    let out = egui::Area::new(id)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(button.center().x, button.top() - 10.0))
        .sense(Sense::click())
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(RP_PAD as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let ui_id = ui.scope_id();
                    let p = ui.painter().clone();

                    // Titulo.
                    let (title, _) = ui.allocate_exact_size(vec2(RP_W, RP_TITLE_H), Sense::hover());
                    p.text(
                        egui::pos2(title.left() + 6.0, title.center().y),
                        Align2::LEFT_CENTER,
                        "Regla",
                        FontId::proportional(15.0),
                        Color32::WHITE,
                    );

                    // Barra negra: "OFF" a la izquierda y los tres tipos de regla pegados a la derecha. El elegido (o
                    // "OFF" si no hay regla) va en azul; con el mouse encima, en blanco.
                    let (bar, _) = ui.allocate_exact_size(vec2(RP_W, RP_BAR_H), Sense::hover());
                    p.rect_filled(bar, CornerRadius::same(8), Color32::BLACK);
                    let color_for = |selected: bool, hovered: bool| {
                        if selected {
                            ACCENT
                        } else if hovered {
                            Color32::WHITE
                        } else {
                            Color32::from_gray(215)
                        }
                    };

                    let off = Rect::from_min_size(bar.min, vec2(RP_OFF_W, RP_BAR_H));
                    let r = ui.interact(off, ui_id.with("regla_off"), Sense::click());
                    p.text(
                        egui::pos2(off.left() + 14.0, off.center().y),
                        Align2::LEFT_CENTER,
                        "OFF",
                        FontId::proportional(17.0),
                        color_for(options.rule == RuleKind::Off, r.hovered()),
                    );
                    if r.on_hover_text("Sin regla").clicked() {
                        options.rule = RuleKind::Off;
                    }

                    for (i, (kind, tip)) in RULES.iter().copied().enumerate() {
                        let x = bar.right() - 6.0 - (RULES.len() - i) as f32 * RP_CELL_W;
                        let cell =
                            Rect::from_min_size(egui::pos2(x, bar.top()), vec2(RP_CELL_W, RP_BAR_H));
                        let r = ui.interact(cell, ui_id.with(("regla", i)), Sense::click());
                        let color = color_for(options.rule == kind, r.hovered());
                        paint_rule_icon(ui, kind, cell.center(), color);
                        if r.on_hover_text(tip).clicked() {
                            options.rule = kind;
                        }
                    }

                    // Simetria: cada letra enciende el espejo del trazo en ese plano del modelo (el modelo esta
                    // centrado en el origen). Con dos o tres encendidos se pinta en todas las combinaciones.
                    ui.add_space(8.0);
                    let (sym_title, _) = ui.allocate_exact_size(vec2(RP_W, RP_TITLE_H), Sense::hover());
                    p.text(
                        egui::pos2(sym_title.left() + 6.0, sym_title.center().y),
                        Align2::LEFT_CENTER,
                        "Simetría",
                        FontId::proportional(15.0),
                        Color32::WHITE,
                    );
                    let (sym_bar, _) = ui.allocate_exact_size(vec2(RP_W, RP_BAR_H), Sense::hover());
                    p.rect_filled(sym_bar, CornerRadius::same(8), Color32::BLACK);
                    for (i, name) in ["X", "Y", "Z"].iter().enumerate() {
                        let cell = Rect::from_min_size(
                            egui::pos2(sym_bar.left() + 6.0 + i as f32 * RP_CELL_W, sym_bar.top()),
                            vec2(RP_CELL_W, RP_BAR_H),
                        );
                        let r = ui.interact(cell, ui_id.with(("simetria", i)), Sense::click());
                        let on = options.symmetry[i];
                        p.text(
                            cell.center(),
                            Align2::CENTER_CENTER,
                            *name,
                            FontId::proportional(20.0),
                            color_for(on, r.hovered()),
                        );
                        if r.on_hover_text(format!("Espejo en {name}")).clicked() {
                            options.symmetry[i] = !on;
                        }
                    }
                });
        });
    // Un clic fuera del panel (y fuera del boton, que ya lo abre/cierra) lo cierra.
    let panel = out.response.rect;
    let pressed_outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !panel.contains(p) && !button.contains(p));
    if pressed_outside {
        options.rules_open = false;
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Reglas (como en Ibis Paint)
//
// La regla vive en la pantalla, no en la textura. Hay tres: una linea infinita (dos puntas y un punto en el medio), un
// circulo (su centro y un punto del aro) y una elipse (su centro y las puntas de sus dos ejes), todos con agarraderas
// que se arrastran. Si un trazo EMPIEZA cerca de la regla, todo el trazo se pega a ella (el punto del raton se proyecta
// sobre la regla antes de lanzar el rayo al modelo); si empieza lejos, se pinta normal. Las medidas van en puntos de
// egui; el raton de `main.rs` va en pixeles fisicos, y `UiLayer` convierte con `pixels_per_point`.
// ---------------------------------------------------------------------------------------------------------------------

/// Distancia (en puntos) a la regla desde la que un trazo que empieza se pega a ella.
const RULE_SNAP: f32 = 40.0;
/// Lado del cuadro que se puede agarrar de cada agarradera, y radio del circulo que se dibuja dentro.
const RULE_HANDLE: f32 = 36.0;
const RULE_HANDLE_R: f32 = 11.0;
/// Cada cuantos pixeles se corrige el recorrido del raton en una regla curva (si no, un tramo recto del raton cortaria
/// por dentro del aro en vez de seguirlo).
const RULE_STEP_PX: f32 = 6.0;
/// Si el punto pegado a una regla curva se mueve mas que esto (en pixeles) en un solo paso de `RULE_STEP_PX`, es un
/// salto: el raton cruzo la zona del centro, donde el punto mas cercano de la regla pasa de un lado al otro. Ese tramo
/// no se pinta (si no, sale una cuerda recta de lado a lado). Con el raton cerca del aro un paso mueve el punto unos
/// 6 px; solo en el cuarto interior de la regla se pasa de 24.
const RULE_JUMP_PX: f32 = 4.0 * RULE_STEP_PX;
/// Radio minimo (en puntos) de un circulo o de un semieje de la elipse al arrastrar sus agarraderas. Debe ser mayor que
/// `RULE_HANDLE`: si no, la agarradera del medio tapa a las de las puntas y ya no se puede volver a agrandar la regla.
const RULE_MIN_R: f32 = 40.0;

/// Elipse de la regla: su centro, el semieje mayor (del centro a su punta; su direccion es el giro de la elipse) y el
/// largo del semieje menor (perpendicular al mayor).
#[derive(Clone, Copy)]
pub(super) struct RulerEllipse {
    c: egui::Pos2,
    a: egui::Vec2,
    ry: f32,
}

impl RulerEllipse {
    /// Direccion unitaria del eje mayor y la del menor (perpendicular).
    fn axes(&self) -> (egui::Vec2, egui::Vec2) {
        let u = if self.a.length() < 1.0 {
            vec2(1.0, 0.0)
        } else {
            self.a.normalized()
        };
        (u, vec2(-u.y, u.x))
    }
}

/// Proyeccion del punto `p` sobre la linea que pasa por `a` y `b` (`None` si las dos puntas coinciden).
fn project_on_line(a: egui::Pos2, b: egui::Pos2, p: egui::Pos2) -> Option<egui::Pos2> {
    let d = b - a;
    let len2 = d.length_sq();
    if len2 < 1.0 {
        return None;
    }
    let t = (p - a).dot(d) / len2;
    Some(a + d * t)
}

/// Proyeccion del punto `p` sobre el aro de centro `c` y radio `r` (`None` si el radio es nulo o `p` es el centro).
fn project_on_circle(c: egui::Pos2, r: f32, p: egui::Pos2) -> Option<egui::Pos2> {
    let v = p - c;
    let len = v.length();
    if r < 1.0 || len < 1e-3 {
        return None;
    }
    Some(c + v * (r / len))
}

/// Punto de la elipse de semiejes `a` y `b` (centrada en el origen y alineada con los ejes) mas cercano a `(x, y)`.
/// Se prueba un barrido de angulos en el primer cuadrante (el resto sale por simetria) y se afina el mejor con una
/// busqueda ternaria. Es mas lento que una formula, pero no se equivoca de lado en ninguna zona (el metodo iterativo que
/// habia antes podia caer en el lado equivocado cerca de los extremos).
fn closest_on_ellipse(a: f32, b: f32, x: f32, y: f32) -> (f32, f32) {
    let (px, py) = (x.abs(), y.abs());
    let dist2 = |t: f32| {
        let (s, c) = t.sin_cos();
        (a * c - px).powi(2) + (b * s - py).powi(2)
    };
    const N: usize = 48;
    let step = std::f32::consts::FRAC_PI_2 / N as f32;
    let (mut best, mut best_d) = (0usize, f32::MAX);
    for i in 0..=N {
        let d = dist2(i as f32 * step);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    let mut lo = (best as f32 - 1.0).max(0.0) * step;
    let mut hi = (best as f32 + 1.0).min(N as f32) * step;
    for _ in 0..24 {
        let m1 = lo + (hi - lo) / 3.0;
        let m2 = hi - (hi - lo) / 3.0;
        if dist2(m1) < dist2(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let (s, c) = ((lo + hi) * 0.5).sin_cos();
    (a * c * x.signum(), b * s * y.signum())
}

/// Proyeccion del punto `p` sobre la elipse (`None` si algun semieje es casi nulo).
fn project_on_ellipse(el: RulerEllipse, p: egui::Pos2) -> Option<egui::Pos2> {
    let rx = el.a.length();
    if rx < 1.0 || el.ry < 1.0 {
        return None;
    }
    let (u, v) = el.axes();
    let d = p - el.c;
    let (qx, qy) = closest_on_ellipse(rx, el.ry, d.dot(u), d.dot(v));
    Some(el.c + u * qx + v * qy)
}

/// Proyeccion del punto `p` sobre la regla elegida (`None` si no hay regla o aun no se coloco).
fn project_on_rule(options: &OptionsPanel, p: egui::Pos2) -> Option<egui::Pos2> {
    match options.rule {
        RuleKind::Off => None,
        RuleKind::Line => {
            let [a, b] = options.ruler_line?;
            project_on_line(a, b, p)
        }
        RuleKind::Circle => {
            let (c, e) = options.ruler_circle?;
            project_on_circle(c, c.distance(e), p)
        }
        RuleKind::Ellipse => project_on_ellipse(options.ruler_ellipse?, p),
    }
}

/// Una agarradera de la regla: un circulo que se arrastra. Devuelve cuanto se arrastro en este cuadro (en puntos).
/// `filled` = circulo lleno (el del medio, que mueve toda la regla); hueco = una punta.
fn ruler_handle(ctx: &egui::Context, name: &str, pos: egui::Pos2, filled: bool) -> egui::Vec2 {
    ruler_handle_drag(ctx, name, pos, filled).0
}

/// Lo mismo que `ruler_handle`, pero ademas devuelve donde debe quedar el centro de la agarradera mientras se arrastra
/// (la posicion del puntero mas lo que habia entre el puntero y el centro al agarrarla), o `None` si no se arrastra. Asi
/// la agarradera sigue al puntero aunque un limite (como `RULE_MIN_R`) la haya frenado, en vez de acumular desplazamientos.
#[allow(deprecated)]
fn ruler_handle_drag(
    ctx: &egui::Context,
    name: &str,
    pos: egui::Pos2,
    filled: bool,
) -> (egui::Vec2, Option<egui::Pos2>) {
    let id = egui::Id::new(name);
    let out = egui::Area::new(id)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(pos)
        .sense(Sense::drag())
        .show(ctx, |ui| {
            let (rect, _) =
                ui.allocate_exact_size(vec2(RULE_HANDLE, RULE_HANDLE), Sense::hover());
            let p = ui.painter();
            let c = rect.center();
            // Contorno oscuro por fuera para que se vea sobre cualquier color.
            p.circle_stroke(c, RULE_HANDLE_R + 1.2, Stroke::new(3.0, Color32::from_black_alpha(110)));
            if filled {
                p.circle_filled(c, RULE_HANDLE_R - 3.0, ACCENT);
            } else {
                p.circle_stroke(c, RULE_HANDLE_R, Stroke::new(2.4, ACCENT));
            }
        });
    let r = out.response;
    let off_id = id.with("grab_offset");
    if r.drag_started() {
        if let Some(p) = r.interact_pointer_pos() {
            ctx.data_mut(|d| d.insert_temp(off_id, pos - p));
        }
    }
    let target = if r.dragged() {
        r.interact_pointer_pos().map(|p| {
            p + ctx
                .data(|d| d.get_temp::<egui::Vec2>(off_id))
                .unwrap_or(egui::Vec2::ZERO)
        })
    } else {
        None
    };
    (r.drag_delta(), target)
}

/// Pintor de la guia de la regla: va detras de los paneles.
#[allow(deprecated)]
fn ruler_painter(ctx: &egui::Context) -> egui::Painter {
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("ruler_line"),
    ))
}

/// Contorno oscuro (mas ancho) de la guia, para que se vea sobre cualquier color; encima va el trazo azul.
fn guide_dark() -> Stroke {
    Stroke::new(4.0, Color32::from_black_alpha(90))
}
fn guide_blue() -> Stroke {
    Stroke::new(2.0, ACCENT)
}

/// Dibuja la regla elegida (su guia y sus agarraderas) y atiende su arrastre.
pub(super) fn ruler_overlay(ctx: &egui::Context, options: &mut OptionsPanel) {
    match options.rule {
        RuleKind::Off => {}
        RuleKind::Line => overlay_line(ctx, options),
        RuleKind::Circle => overlay_circle(ctx, options),
        RuleKind::Ellipse => overlay_ellipse(ctx, options),
    }
}

/// Regla recta: una linea que cruza toda la pantalla. Las puntas giran y estiran la linea; el punto del medio la mueve
/// entera. La primera vez sale horizontal en el centro de la pantalla.
fn overlay_line(ctx: &egui::Context, options: &mut OptionsPanel) {
    let [mut a, mut b] = *options.ruler_line.get_or_insert_with(|| {
        let c = ctx.content_rect().center();
        [c + vec2(-150.0, 0.0), c + vec2(150.0, 0.0)]
    });
    let mid = a + (b - a) * 0.5;
    let dir = if (b - a).length() < 1.0 {
        vec2(1.0, 0.0)
    } else {
        (b - a).normalized()
    };
    let (p0, p1) = (mid - dir * 10_000.0, mid + dir * 10_000.0);
    let painter = ruler_painter(ctx);
    painter.line_segment([p0, p1], guide_dark());
    painter.line_segment([p0, p1], guide_blue());

    let da = ruler_handle(ctx, "ruler_a", a, false);
    let db = ruler_handle(ctx, "ruler_b", b, false);
    let dm = ruler_handle(ctx, "ruler_m", mid, true);
    a += da + dm;
    b += db + dm;
    options.ruler_line = Some([a, b]);
}

/// Regla circular: un aro. El punto del medio mueve el circulo; la agarradera del aro cambia su radio (y lo gira, que
/// en un circulo no se nota). La primera vez sale en el centro de la pantalla.
fn overlay_circle(ctx: &egui::Context, options: &mut OptionsPanel) {
    let (mut c, mut e) = *options.ruler_circle.get_or_insert_with(|| {
        let m = ctx.content_rect().center();
        (m, m + vec2(150.0, 0.0))
    });
    let r = c.distance(e);
    let painter = ruler_painter(ctx);
    painter.circle_stroke(c, r, guide_dark());
    painter.circle_stroke(c, r, guide_blue());

    let dc = ruler_handle(ctx, "ruler_c_m", c, true);
    let de = ruler_handle(ctx, "ruler_c_e", e, false);
    c += dc;
    e += dc;
    let moved = e + de;
    // El radio no baja del minimo (si no, el aro se vuelve un punto y ya no se puede agarrar).
    if c.distance(moved) >= RULE_MIN_R {
        e = moved;
    }
    options.ruler_circle = Some((c, e));
}

/// Regla elíptica: un ovalo. El punto del medio lo mueve; la agarradera del eje mayor cambia su largo y lo gira; la del
/// eje menor cambia solo el ancho. La primera vez sale horizontal en el centro de la pantalla.
fn overlay_ellipse(ctx: &egui::Context, options: &mut OptionsPanel) {
    let mut el = *options.ruler_ellipse.get_or_insert_with(|| RulerEllipse {
        c: ctx.content_rect().center(),
        a: vec2(170.0, 0.0),
        ry: 100.0,
    });
    let (u, v) = el.axes();
    let rx = el.a.length();
    let pts: Vec<egui::Pos2> = (0..96)
        .map(|i| {
            let t = i as f32 / 96.0 * std::f32::consts::TAU;
            el.c + u * (rx * t.cos()) + v * (el.ry * t.sin())
        })
        .collect();
    let painter = ruler_painter(ctx);
    painter.add(egui::Shape::closed_line(pts.clone(), guide_dark()));
    painter.add(egui::Shape::closed_line(pts, guide_blue()));

    let end_minor = el.c + v * el.ry;
    let dm = ruler_handle(ctx, "ruler_e_m", el.c, true);
    let (_, target_a) = ruler_handle_drag(ctx, "ruler_e_a", el.c + el.a, false);
    let (_, target_b) = ruler_handle_drag(ctx, "ruler_e_b", end_minor, false);
    el.c += dm;
    // Las puntas siguen al puntero (no acumulan desplazamientos). La del eje mayor da su largo y su giro; si se acerca
    // demasiado al centro se queda en el largo minimo y conserva la direccion.
    if let Some(p) = target_a {
        let new_a = p - el.c;
        if new_a.length() >= RULE_MIN_R {
            el.a = new_a;
        } else if new_a.length() > 1e-3 {
            el.a = new_a.normalized() * RULE_MIN_R;
        }
    }
    // El eje menor se queda perpendicular al mayor: de donde esta el puntero solo cuenta lo que cae sobre ese eje.
    // Sin valor absoluto: al pasar del otro lado del centro se queda en el minimo en vez de saltar de lado.
    if let Some(p) = target_b {
        let (_, v2) = el.axes();
        el.ry = (p - el.c).dot(v2).max(RULE_MIN_R);
    }
    options.ruler_ellipse = Some(el);
}

/// Recorre el tramo `prev` -> `cur` en pasos de `RULE_STEP_PX` y devuelve, para cada paso, el punto ya corregido por
/// `snap` y si es un salto respecto al punto anterior (se movio mas que `RULE_JUMP_PX`). Si `snap` no da punto (el raton
/// esta justo en el centro de la regla, donde el mas cercano no esta definido) ese paso se salta.
fn snapped_steps(
    snap: impl Fn((f32, f32)) -> Option<(f32, f32)>,
    prev: (f32, f32),
    cur: (f32, f32),
) -> Vec<((f32, f32), bool)> {
    let dist = (cur.0 - prev.0).hypot(cur.1 - prev.1);
    let n = (dist / RULE_STEP_PX).ceil().clamp(1.0, 200.0) as usize;
    let mut last = snap(prev);
    let mut out = Vec::with_capacity(n);
    for i in 1..=n {
        let t = i as f32 / n as f32;
        let Some(q) = snap((prev.0 + (cur.0 - prev.0) * t, prev.1 + (cur.1 - prev.1) * t)) else {
            continue;
        };
        let jump = last.is_some_and(|l| (q.0 - l.0).hypot(q.1 - l.1) > RULE_JUMP_PX);
        out.push((q, jump));
        last = Some(q);
    }
    out
}

impl UiLayer {
    /// Apoyar el boton en `p` (pixeles fisicos): si hay una regla y el punto cae a menos de `RULE_SNAP` de ella, el
    /// trazo queda pegado a la regla hasta soltar el boton. Devuelve el punto donde empieza el trazo (`p` proyectado, o
    /// el mismo `p` si no se pega).
    pub fn ruler_begin(&mut self, p: (f32, f32)) -> (f32, f32) {
        self.options.ruler_snapping = false;
        let ppp = self.ctx.pixels_per_point();
        let pt = egui::pos2(p.0 / ppp, p.1 / ppp);
        let Some(q) = project_on_rule(&self.options, pt) else {
            return p;
        };
        if pt.distance(q) > RULE_SNAP {
            return p;
        }
        self.options.ruler_snapping = true;
        (q.x * ppp, q.y * ppp)
    }

    /// Un punto del raton `p` (pixeles fisicos) ya corregido por la regla: si el trazo en curso esta pegado a ella, `p`
    /// proyectado sobre la regla; si no, el mismo `p`.
    pub fn ruler_snap(&self, p: (f32, f32)) -> (f32, f32) {
        if !self.options.ruler_snapping {
            return p;
        }
        let ppp = self.ctx.pixels_per_point();
        match project_on_rule(&self.options, egui::pos2(p.0 / ppp, p.1 / ppp)) {
            Some(q) => (q.x * ppp, q.y * ppp),
            None => p,
        }
    }

    /// Los puntos por donde pasa el trazo al mover el raton de `prev` a `cur` (pixeles fisicos, tal como los manda el
    /// raton: aqui se corrigen por la regla). Cada punto lleva un `bool`: `true` = es un salto, el trazo no debe
    /// recorrer el tramo desde el punto anterior hasta este (ver `RULE_JUMP_PX`). Sin regla o con la recta es solo el
    /// punto final: el tramo recto ya cae sobre la regla y nunca salta. Con una regla curva se reparte el tramo en pasos
    /// chicos y se corrige cada uno, para que el trazo siga la curva y no la cuerda.
    pub fn ruler_path(&self, prev: (f32, f32), cur: (f32, f32)) -> Vec<((f32, f32), bool)> {
        if !self.options.ruler_snapping || self.options.rule == RuleKind::Line {
            return vec![(self.ruler_snap(cur), false)];
        }
        let ppp = self.ctx.pixels_per_point();
        let snap = |p: (f32, f32)| {
            project_on_rule(&self.options, egui::pos2(p.0 / ppp, p.1 / ppp))
                .map(|q| (q.x * ppp, q.y * ppp))
        };
        snapped_steps(snap, prev, cur)
    }

    /// Planos de simetria encendidos en el panel de reglas: espejo del trazo en X, Y y Z.
    pub fn symmetry(&self) -> [bool; 3] {
        self.options.symmetry
    }

    /// Soltar el boton: el trazo deja de estar pegado a la regla.
    pub fn ruler_end(&mut self) {
        self.options.ruler_snapping = false;
    }
}

#[cfg(test)]
mod ellipse_tests {
    use super::*;

    /// Cuanto vale la ecuacion de la elipse en `(x, y)`: 1 = justo sobre ella.
    fn ecuacion(a: f32, b: f32, x: f32, y: f32) -> f32 {
        (x / a).powi(2) + (y / b).powi(2)
    }

    #[test]
    fn con_semiejes_iguales_es_la_proyeccion_radial() {
        for (x, y) in [(300.0, 40.0), (-20.0, 50.0), (10.0, -400.0), (-70.0, -70.0)] {
            let (qx, qy) = closest_on_ellipse(100.0, 100.0, x, y);
            let k = 100.0 / (x * x + y * y).sqrt();
            assert!(
                (qx - x * k).abs() < 0.05 && (qy - y * k).abs() < 0.05,
                "({x}, {y}) -> ({qx}, {qy})"
            );
        }
    }

    #[test]
    fn el_resultado_cae_sobre_la_elipse() {
        for (a, b) in [(170.0, 100.0), (100.0, 170.0), (300.0, 20.0)] {
            for (x, y) in [
                (500.0, 30.0),
                (-40.0, 10.0),
                (0.5, 200.0),
                (-250.0, -90.0),
                (5.0, -3.0),
            ] {
                let (qx, qy) = closest_on_ellipse(a, b, x, y);
                let e = ecuacion(a, b, qx, qy);
                assert!((e - 1.0).abs() < 1e-3, "a={a} b={b} ({x}, {y}) -> ({qx}, {qy}), ecuacion {e}");
            }
        }
    }

    #[test]
    fn proyectar_dos_veces_da_el_mismo_punto() {
        for (x, y) in [(500.0, 30.0), (-40.0, 10.0), (0.5, 200.0), (-250.0, -90.0)] {
            let (px, py) = closest_on_ellipse(170.0, 100.0, x, y);
            let (qx, qy) = closest_on_ellipse(170.0, 100.0, px, py);
            assert!(
                (px - qx).abs() < 0.01 && (py - qy).abs() < 0.01,
                "({x}, {y}): ({px}, {py}) y luego ({qx}, {qy})"
            );
        }
    }

    #[test]
    fn un_punto_fuera_sobre_el_eje_mayor_va_a_la_punta() {
        let (qx, qy) = closest_on_ellipse(170.0, 100.0, 400.0, 0.0);
        // Junto al minimo la distancia casi no cambia y en f32 la busqueda se detiene a unas centesimas de punto: sobra.
        assert!((qx - 170.0).abs() < 0.1 && qy.abs() < 0.1, "({qx}, {qy})");
    }

    #[test]
    fn elipse_girada_proyecta_sobre_sus_ejes() {
        // Eje mayor vertical (150 de largo): un punto muy arriba del centro cae en la punta de arriba.
        let el = RulerEllipse {
            c: egui::pos2(100.0, 100.0),
            a: egui::vec2(0.0, 150.0),
            ry: 60.0,
        };
        let q = project_on_ellipse(el, egui::pos2(100.0, -300.0)).unwrap();
        assert!((q.x - 100.0).abs() < 0.05 && (q.y - (-50.0)).abs() < 0.05, "{q:?}");
    }

    #[test]
    fn semiejes_casi_nulos_no_proyectan() {
        let el = RulerEllipse {
            c: egui::pos2(0.0, 0.0),
            a: egui::vec2(0.5, 0.0),
            ry: 100.0,
        };
        assert!(project_on_ellipse(el, egui::pos2(10.0, 10.0)).is_none());
    }
}

#[cfg(test)]
mod salto_tests {
    use super::*;

    const C: egui::Pos2 = egui::pos2(500.0, 500.0);
    const R: f32 = 200.0;

    fn al_aro(p: (f32, f32)) -> Option<(f32, f32)> {
        project_on_circle(C, R, egui::pos2(p.0, p.1)).map(|q| (q.x, q.y))
    }

    fn al_borde_de_la_elipse(p: (f32, f32)) -> Option<(f32, f32)> {
        let el = RulerEllipse {
            c: C,
            a: egui::vec2(250.0, 0.0),
            ry: 120.0,
        };
        project_on_ellipse(el, egui::pos2(p.0, p.1)).map(|q| (q.x, q.y))
    }

    #[test]
    fn cruzar_el_centro_de_un_circulo_da_un_solo_salto() {
        // De cerca del borde de arriba a cerca del de abajo, en linea recta por el centro.
        let pasos = snapped_steps(al_aro, (500.0, 320.0), (500.0, 680.0));
        assert_eq!(pasos.iter().filter(|(_, salto)| *salto).count(), 1, "{pasos:?}");
        let (ultimo, _) = *pasos.last().unwrap();
        assert!((ultimo.0 - 500.0).abs() < 0.5 && (ultimo.1 - 700.0).abs() < 0.5, "{ultimo:?}");
        // El salto es justo al pasar el centro: antes de el no hay ninguno.
        let primero = pasos.iter().position(|(_, salto)| *salto).unwrap();
        assert!(pasos[..primero].iter().all(|(p, _)| (p.1 - 300.0).abs() < 0.5));
    }

    #[test]
    fn cruzar_el_interior_de_una_elipse_tambien_salta() {
        // Atravesando el eje mayor por el interior, el borde mas cercano pasa de arriba a abajo.
        let pasos = snapped_steps(al_borde_de_la_elipse, (500.0, 440.0), (500.0, 560.0));
        assert!(pasos.iter().any(|(_, salto)| *salto), "{pasos:?}");
    }

    #[test]
    fn seguir_el_aro_sin_cruzar_el_centro_no_salta() {
        // Del borde derecho al de abajo con el raton: la recta pasa por dentro pero sin acercarse al centro.
        let pasos = snapped_steps(al_aro, (700.0, 500.0), (500.0, 700.0));
        assert!(!pasos.is_empty());
        assert!(pasos.iter().all(|(_, salto)| !*salto), "{pasos:?}");
        let (ultimo, _) = *pasos.last().unwrap();
        assert!((ultimo.0 - 500.0).abs() < 0.5 && (ultimo.1 - 700.0).abs() < 0.5, "{ultimo:?}");
    }

    #[test]
    fn un_paso_sin_punto_se_salta() {
        // Si la regla no da punto en ningun paso, no hay nada que devolver (y no se rompe).
        let pasos = snapped_steps(|_| None, (0.0, 0.0), (30.0, 0.0));
        assert!(pasos.is_empty());
    }
}
