//! Panel de color flotante (como el de Ibis Paint): rueda de tono con rombo de saturacion/brillo, pestanas
//! Paleta / RVA / HSB, deslizadores con botones - y +, barra de opacidad, colores recientes y el color anterior.
//!
//! El color del pincel vive fuera (`[u8; 3]` + opacidad). Aqui se guarda ademas el color en HSB, porque al llegar a
//! brillo 0 o saturacion 0 el tono se perderia si solo existiera el RGB.

use std::f32::consts::TAU;

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Mesh, Painter, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};

/// Ancho del contenido del panel.
const W: f32 = 232.0;
/// Lado de la zona de la rueda.
const WHEEL: f32 = 190.0;
const ROW_H: f32 = 28.0;
/// Alto de la caja oscura (rueda + 3 deslizadores + opacidad).
const BODY_H: f32 = 4.0 + WHEEL + 8.0 + 3.0 * ROW_H + 8.0 + ROW_H + 6.0;
/// Donde empieza la fila de opacidad dentro de la caja.
const ALPHA_TOP: f32 = BODY_H - ROW_H - 4.0;
const RING_THICK: f32 = 22.0;
/// Radio de la perilla blanca de los deslizadores.
const KNOB_R: f32 = 8.0;
const BLUE: Color32 = Color32::from_rgb(100, 160, 255);
const PANEL_FILL: Color32 = Color32::from_gray(128);
const BODY_FILL: Color32 = Color32::from_gray(87);
const RECENT_MAX: usize = 8;
/// Distancia del panel al borde izquierdo (justo a la derecha de la barra de herramientas) y al borde superior.
const PANEL_LEFT: f32 = 62.0;
const PANEL_TOP: f32 = 12.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Palette,
    Rgb,
    Hsb,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WheelDrag {
    Ring,
    Diamond,
}

pub struct ColorPanel {
    pub open: bool,
    tab: Tab,
    /// Tono, saturacion y brillo (0..1) del color actual.
    hsv: [f32; 3],
    /// Color anterior (el cuadro de la derecha del encabezado).
    prev: [u8; 3],
    /// Ultimo color confirmado (al soltar un control).
    last: Option<[u8; 3]>,
    recent: Vec<[u8; 3]>,
    drag: Option<WheelDrag>,
    /// Texto del codigo hex editable (se rehace desde el color cuando no se esta escribiendo).
    hex: String,
}

impl Default for ColorPanel {
    fn default() -> Self {
        ColorPanel { open: false, tab: Tab::Hsb, hsv: [0.0, 0.0, 0.0], prev: [0, 0, 0], last: None, recent: Vec::new(), drag: None, hex: String::new() }
    }
}

// ---------------------------------------------------------------- color

fn to_u8(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match (i as i32) % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [to_u8(r), to_u8(g), to_u8(b)]
}

/// RGB -> HSB. En un gris no hay tono: se conserva `prev_h`.
fn rgb_to_hsv(c: [u8; 3], prev_h: f32) -> [f32; 3] {
    let (r, g, b) = (c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max };
    let h = if d == 0.0 {
        prev_h
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    [h, s, max]
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Lee un codigo hex de 6 o 3 digitos, con o sin `#`.
fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let t = s.trim().trim_start_matches('#');
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match t.len() {
        6 => Some([
            u8::from_str_radix(&t[0..2], 16).ok()?,
            u8::from_str_radix(&t[2..4], 16).ok()?,
            u8::from_str_radix(&t[4..6], 16).ok()?,
        ]),
        3 => {
            let d = |i: usize| u8::from_str_radix(&t[i..i + 1], 16).ok().map(|v| v * 17);
            Some([d(0)?, d(1)?, d(2)?])
        }
        _ => None,
    }
}

fn palette_colors() -> Vec<[u8; 3]> {
    let mut v = vec![[0, 0, 0], [255, 255, 255]];
    for k in 0..18 {
        v.push(hsv_to_rgb(k as f32 / 18.0, 1.0, 1.0));
    }
    v.extend([[255, 245, 225], [253, 239, 196], [254, 231, 180], [255, 205, 165]]);
    v
}

// ---------------------------------------------------------------- dibujo

fn checker(p: &Painter, rect: Rect, cell: f32) {
    p.rect_filled(rect, CornerRadius::ZERO, Color32::from_gray(235));
    let nx = (rect.width() / cell).ceil() as i32;
    let ny = (rect.height() / cell).ceil() as i32;
    for j in 0..ny {
        for i in 0..nx {
            if (i + j) % 2 == 1 {
                let r = Rect::from_min_size(pos2(rect.left() + i as f32 * cell, rect.top() + j as f32 * cell), vec2(cell, cell));
                p.rect_filled(r.intersect(rect), CornerRadius::ZERO, Color32::from_gray(195));
            }
        }
    }
}

/// Color de un degradado con varias paradas en `t` (0..1).
fn lerp_stops(stops: &[Color32], t: f32) -> Color32 {
    let n = stops.len();
    if n == 1 {
        return stops[0];
    }
    let x = t.clamp(0.0, 1.0) * (n - 1) as f32;
    let i = (x.floor() as usize).min(n - 2);
    let f = x - i as f32;
    let (a, b) = (stops[i].to_array(), stops[i + 1].to_array());
    let l = |k: usize| (a[k] as f32 + (b[k] as f32 - a[k] as f32) * f).round() as u8;
    Color32::from_rgba_premultiplied(l(0), l(1), l(2), l(3))
}

/// Mitad del alto de la pastilla en la posicion `x` (baja a 0 en las puntas redondas).
fn pill_half_height(rect: Rect, x: f32) -> f32 {
    let r = rect.height() / 2.0;
    let dx = x - x.clamp(rect.left() + r, rect.right() - r);
    (r * r - dx * dx).max(0.0).sqrt()
}

/// Barra en forma de pastilla (puntas redondas) con degradado horizontal y borde suavizado: el contorno se difumina
/// un pixel hacia afuera con vertices transparentes, igual que hace egui con sus formas.
fn pill_gradient(p: &Painter, rect: Rect, stops: &[Color32]) {
    const CAP: u32 = 10;
    const MID: u32 = 32;
    const FEATHER: f32 = 1.0;
    let r = rect.height() / 2.0;
    let cy = rect.center().y;
    let (cl, cr) = (rect.left() + r, rect.right() - r);
    // Columnas: la punta izquierda por angulos, el centro uniforme y la punta derecha por angulos.
    let mut xs: Vec<f32> = Vec::new();
    for i in 0..=CAP {
        let th = std::f32::consts::FRAC_PI_2 * i as f32 / CAP as f32;
        xs.push(cl - r * th.cos());
    }
    for i in 1..MID {
        xs.push(cl + (cr - cl) * i as f32 / MID as f32);
    }
    for i in 0..=CAP {
        let th = std::f32::consts::FRAC_PI_2 * (1.0 - i as f32 / CAP as f32);
        xs.push(cr + r * th.cos());
    }
    let mut mesh = Mesh::default();
    for &x in &xs {
        let h = pill_half_height(rect, x);
        let col = lerp_stops(stops, (x - rect.left()) / rect.width());
        let center = pos2(x.clamp(cl, cr), cy);
        let top = pos2(x, cy - h);
        let bot = pos2(x, cy + h);
        let normal = |pt: egui::Pos2, fallback: egui::Vec2| {
            let v = pt - center;
            if v.length() > 1e-4 { v.normalized() } else { fallback }
        };
        mesh.colored_vertex(top + normal(top, vec2(0.0, -1.0)) * FEATHER, Color32::TRANSPARENT);
        mesh.colored_vertex(top, col);
        mesh.colored_vertex(bot, col);
        mesh.colored_vertex(bot + normal(bot, vec2(0.0, 1.0)) * FEATHER, Color32::TRANSPARENT);
    }
    for k in 0..(xs.len() as u32 - 1) {
        let (a, b) = (k * 4, (k + 1) * 4);
        // Tres franjas por tramo: difuminado de arriba, relleno y difuminado de abajo.
        for s in 0..3 {
            mesh.add_triangle(a + s, a + s + 1, b + s);
            mesh.add_triangle(a + s + 1, b + s + 1, b + s);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// Fondo de cuadros para la barra de opacidad, tambien en forma de pastilla.
fn pill_checker(p: &Painter, rect: Rect, cell: f32) {
    pill_gradient(p, rect, &[Color32::from_gray(235), Color32::from_gray(235)]);
    let cy = rect.center().y;
    let nx = (rect.width() / cell).ceil() as i32;
    let ny = (rect.height() / cell).ceil() as i32;
    for j in 0..ny {
        for i in 0..nx {
            if (i + j) % 2 == 1 {
                let x0 = rect.left() + i as f32 * cell;
                let x1 = (x0 + cell).min(rect.right());
                let h = pill_half_height(rect, x0).min(pill_half_height(rect, x1));
                let inner = Rect::from_min_max(pos2(x0, cy - h), pos2(x1, cy + h));
                let r = Rect::from_min_max(pos2(x0, rect.top() + j as f32 * cell), pos2(x1, rect.top() + (j + 1) as f32 * cell))
                    .intersect(inner);
                if r.width() > 0.0 && r.height() > 0.0 {
                    p.rect_filled(r, CornerRadius::ZERO, Color32::from_gray(195));
                }
            }
        }
    }
}

fn on_circle(center: egui::Pos2, angle: f32, r: f32) -> egui::Pos2 {
    // Angulo en sentido contrario a las agujas del reloj, con el rojo a la derecha.
    center + vec2(angle.cos(), -angle.sin()) * r
}

fn paint_ring(p: &Painter, center: egui::Pos2, r_out: f32, r_in: f32) {
    const N: u32 = 120;
    const FEATHER: f32 = 1.0;
    let mut mesh = Mesh::default();
    // Por cada angulo: un vertice transparente fuera, el borde exterior, el interior y otro transparente dentro.
    // El degradado de 1 px hacia lo transparente suaviza el borde (sin esto se ve con escalones).
    for i in 0..=N {
        let h = i as f32 / N as f32;
        let c = c32(hsv_to_rgb(h, 1.0, 1.0));
        let a = h * TAU;
        mesh.colored_vertex(on_circle(center, a, r_out + FEATHER), Color32::TRANSPARENT);
        mesh.colored_vertex(on_circle(center, a, r_out), c);
        mesh.colored_vertex(on_circle(center, a, r_in), c);
        mesh.colored_vertex(on_circle(center, a, r_in - FEATHER), Color32::TRANSPARENT);
    }
    for i in 0..N {
        let (a, b) = (i * 4, (i + 1) * 4);
        for s in 0..3 {
            mesh.add_triangle(a + s, a + s + 1, b + s);
            mesh.add_triangle(a + s + 1, b + s + 1, b + s);
        }
    }
    p.add(Shape::mesh(mesh));
}

/// Posicion de (s, v) dentro del rombo: arriba = blanco, derecha = tono puro, abajo e izquierda = negro.
fn diamond_pos(center: egui::Pos2, d: f32, s: f32, v: f32) -> egui::Pos2 {
    center + vec2(s + v - 1.0, s - v) * d
}

fn paint_diamond(p: &Painter, center: egui::Pos2, d: f32, hue: f32) {
    const N: u32 = 12;
    let mut mesh = Mesh::default();
    for i in 0..=N {
        for j in 0..=N {
            let (s, v) = (i as f32 / N as f32, j as f32 / N as f32);
            mesh.colored_vertex(diamond_pos(center, d, s, v), c32(hsv_to_rgb(hue, s, v)));
        }
    }
    let idx = |i: u32, j: u32| i * (N + 1) + j;
    for i in 0..N {
        for j in 0..N {
            mesh.add_triangle(idx(i, j), idx(i + 1, j), idx(i, j + 1));
            mesh.add_triangle(idx(i + 1, j), idx(i + 1, j + 1), idx(i, j + 1));
        }
    }
    // Borde suavizado: por cada lado, una franja de 1 px hacia afuera que se vuelve transparente.
    let k = std::f32::consts::FRAC_1_SQRT_2;
    let edges: [(fn(f32) -> (f32, f32), egui::Vec2); 4] = [
        (|t| (0.0, t), vec2(-k, -k)),
        (|t| (1.0, t), vec2(k, k)),
        (|t| (t, 0.0), vec2(-k, k)),
        (|t| (t, 1.0), vec2(k, -k)),
    ];
    for (f, n) in edges {
        let start = mesh.vertices.len() as u32;
        for i in 0..=N {
            let (s, v) = f(i as f32 / N as f32);
            let pos = diamond_pos(center, d, s, v);
            mesh.colored_vertex(pos, c32(hsv_to_rgb(hue, s, v)));
            mesh.colored_vertex(pos + n, Color32::TRANSPARENT);
        }
        for i in 0..N {
            let a = start + i * 2;
            mesh.add_triangle(a, a + 1, a + 2);
            mesh.add_triangle(a + 1, a + 3, a + 2);
        }
    }
    p.add(Shape::mesh(mesh));
}

fn handle(p: &Painter, pos: egui::Pos2, fill: Color32) {
    p.circle_filled(pos, 7.0, fill);
    p.circle_stroke(pos, 7.0, Stroke::new(2.5, Color32::from_gray(220)));
}

// ---------------------------------------------------------------- deslizador

struct Edit {
    changed: bool,
    done: bool,
}

/// Una fila: etiqueta, boton -, barra con degradado, boton +. `frac` va de 0 a 1.
fn slider_row(
    ui: &Ui,
    rect: Rect,
    id: egui::Id,
    label: &str,
    frac: &mut f32,
    step: f32,
    paint_track: impl FnOnce(&Painter, Rect),
) -> Edit {
    let p = ui.painter().clone();
    // El centro se redondea a pixel entero para que el borde de arriba y el de abajo de la barra queden iguales.
    let ppp = ui.ctx().pixels_per_point();
    let cy = (rect.center().y * ppp).round() / ppp;
    let label_w = 38.0;
    let btn_r = 10.0;
    let minus_c = pos2(rect.left() + label_w + btn_r, cy);
    let plus_c = pos2(rect.right() - btn_r, cy);
    let track = {
        // Arriba y abajo se redondean por separado a pixel entero, asi las tres barras quedan identicas aunque
        // la escala de pantalla no sea 1.0.
        let snap = |v: f32| (v * ppp).round() / ppp;
        let half = snap(3.0);
        Rect::from_min_max(pos2(minus_c.x + btn_r + 12.0, cy - half), pos2(plus_c.x - btn_r - 12.0, cy + half))
    };
    let mut edit = Edit { changed: false, done: false };

    p.text(pos2(rect.left() + label_w - 6.0, cy), Align2::RIGHT_CENTER, label, FontId::proportional(11.0), Color32::WHITE);

    paint_track(&p, track);

    // Botones - y +.
    for (c, sign, name) in [(minus_c, -1.0_f32, "minus"), (plus_c, 1.0_f32, "plus")] {
        p.circle_filled(c, btn_r, Color32::BLACK);
        p.line_segment([c - vec2(4.0, 0.0), c + vec2(4.0, 0.0)], Stroke::new(1.5, Color32::WHITE));
        if sign > 0.0 {
            p.line_segment([c - vec2(0.0, 4.0), c + vec2(0.0, 4.0)], Stroke::new(1.5, Color32::WHITE));
        }
        let r = ui.interact(Rect::from_center_size(c, vec2(24.0, 24.0)), id.with(name), Sense::click());
        if r.clicked() {
            *frac = (*frac + sign * step).clamp(0.0, 1.0);
            edit.changed = true;
            edit.done = true;
        }
    }

    // Barra: arrastrar o hacer clic.
    let rt = ui.interact(track.expand2(vec2(10.0, 8.0)), id.with("track"), Sense::click_and_drag());
    if rt.is_pointer_button_down_on() {
        if let Some(pos) = rt.interact_pointer_pos() {
            *frac = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            edit.changed = true;
        }
    }
    if rt.drag_stopped() || rt.clicked() {
        edit.done = true;
    }

    // Perilla.
    let knob = pos2(track.left() + *frac * track.width(), cy);
    p.circle_filled(knob, KNOB_R, Color32::WHITE);
    edit
}

// ---------------------------------------------------------------- panel

impl ColorPanel {
    /// Dibuja el panel si esta abierto. `color` y `opacity` (0..1) son los del pincel. Devuelve true si algo cambio.
    pub fn show(&mut self, ctx: &egui::Context, color: &mut [u8; 3], opacity: &mut f32) -> bool {
        if !self.open {
            return false;
        }
        // Si el color cambio desde fuera (cuentagotas, tecla K), el HSB se pone al dia.
        if hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2]) != *color {
            self.hsv = rgb_to_hsv(*color, self.hsv[0]);
        }
        if self.last.is_none() {
            self.last = Some(*color);
        }
        let (start_color, start_opacity) = (*color, *opacity);
        let mut done = false;
        #[allow(deprecated)]
        let id = egui::Id::new("color_panel");
        egui::Area::new(id).anchor(Align2::LEFT_TOP, vec2(PANEL_LEFT, PANEL_TOP)).show(ctx, |ui| {
            Frame::new()
                .fill(PANEL_FILL)
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    self.contents(ui, color, opacity, &mut done);
                });
        });
        if done {
            self.commit(*color);
        }
        *color != start_color || *opacity != start_opacity
    }

    /// Un control se solto: el color pasa a "anterior" y a la lista de recientes.
    fn commit(&mut self, color: [u8; 3]) {
        let last = self.last.unwrap_or(color);
        if color != last {
            self.prev = last;
            self.last = Some(color);
            self.recent.retain(|c| *c != color);
            self.recent.insert(0, color);
            self.recent.truncate(RECENT_MAX);
        }
    }

    fn set_color(&mut self, c: [u8; 3], color: &mut [u8; 3], done: &mut bool) {
        *color = c;
        self.hsv = rgb_to_hsv(c, self.hsv[0]);
        *done = true;
    }

    fn contents(&mut self, ui: &mut Ui, color: &mut [u8; 3], opacity: &mut f32, done: &mut bool) {
        ui.spacing_mut().item_spacing = vec2(0.0, 6.0);
        let base = ui.scope_id().with("panel");

        // Encabezado: "Color" y los dos cuadros (actual | anterior).
        let (head, _) = ui.allocate_exact_size(vec2(W, 26.0), Sense::hover());
        let p = ui.painter().clone();
        p.text(pos2(head.left() + 4.0, head.center().y), Align2::LEFT_CENTER, "Color", FontId::proportional(16.0), Color32::WHITE);
        let sw = Rect::from_min_size(pos2(head.right() - 84.0, head.top() + 1.0), vec2(84.0, 24.0));
        let left = Rect::from_min_max(sw.min, pos2(sw.center().x, sw.max.y));
        let right = Rect::from_min_max(pos2(sw.center().x, sw.min.y), sw.max);
        p.rect_filled(left, CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, c32(*color));
        p.rect_filled(right, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, c32(self.prev));
        p.line_segment([pos2(sw.center().x, sw.top()), pos2(sw.center().x, sw.bottom())], Stroke::new(1.0, Color32::from_white_alpha(40)));
        let r = ui.interact(right, base.with("prev"), Sense::click()).on_hover_text("Color anterior (clic para volver a el)");
        if r.clicked() {
            let c = self.prev;
            self.set_color(c, color, done);
        }

        // Caja oscura.
        let (body, _) = ui.allocate_exact_size(vec2(W, BODY_H), Sense::hover());
        p.rect_filled(body, CornerRadius::same(8), BODY_FILL);
        match self.tab {
            Tab::Palette => self.palette_tab(ui, &p, body, base, color, done),
            Tab::Rgb | Tab::Hsb => {
                self.wheel(ui, &p, body, base, color, done);
                if self.tab == Tab::Hsb {
                    self.hsb_sliders(ui, body, base, color, done);
                } else {
                    self.rgb_sliders(ui, body, base, color, done);
                }
            }
        }

        // Opacidad.
        let sep_y = body.top() + ALPHA_TOP - 4.0;
        p.line_segment([pos2(body.left(), sep_y), pos2(body.right(), sep_y)], Stroke::new(1.0, Color32::from_gray(110)));
        let alpha_rect = Rect::from_min_size(pos2(body.left() + 8.0, body.top() + ALPHA_TOP), vec2(W - 16.0, ROW_H));
        let opaque = c32(*color);
        let label = format!("{}%", (*opacity * 100.0).round() as i32);
        slider_row(ui, alpha_rect, base.with("alpha"), &label, opacity, 0.01, |p, r| {
            pill_checker(p, r, 3.5);
            pill_gradient(p, r, &[Color32::TRANSPARENT, opaque]);
        });

        // Pestanas.
        let (bar, _) = ui.allocate_exact_size(vec2(W, 38.0), Sense::hover());
        p.rect_filled(bar, CornerRadius::same(10), Color32::BLACK);
        let tabs = [(Tab::Palette, "Paleta"), (Tab::Rgb, "RVA"), (Tab::Hsb, "HSB")];
        for (i, (tab, name)) in tabs.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(bar.left() + W / 3.0 * i as f32, bar.top()), vec2(W / 3.0, bar.height()));
            let col = if self.tab == tab { BLUE } else { Color32::WHITE };
            let c = pos2(r.center().x, r.top() + 12.0);
            match tab {
                Tab::Palette => {
                    for gy in 0..3 {
                        for gx in 0..3 {
                            let q = Rect::from_center_size(c + vec2((gx - 1) as f32 * 6.0, (gy - 1) as f32 * 6.0), vec2(4.0, 4.0));
                            p.rect_filled(q, CornerRadius::ZERO, col);
                        }
                    }
                }
                Tab::Rgb => {
                    for off in [vec2(0.0, -4.0), vec2(-4.0, 3.0), vec2(4.0, 3.0)] {
                        p.circle_stroke(c + off, 6.5, Stroke::new(1.5, col));
                    }
                }
                Tab::Hsb => {
                    p.circle_stroke(c, 6.5, Stroke::new(3.5, col));
                }
            }
            p.text(pos2(r.center().x, r.bottom() - 3.0), Align2::CENTER_BOTTOM, name, FontId::proportional(10.0), col);
            if ui.interact(r, base.with(("tab", i)), Sense::click()).clicked() {
                self.tab = tab;
            }
        }
    }

    /// Rueda de tono + rombo de saturacion/brillo.
    fn wheel(&mut self, ui: &mut Ui, p: &Painter, body: Rect, base: egui::Id, color: &mut [u8; 3], done: &mut bool) {
        let wheel = Rect::from_min_size(pos2(body.center().x - WHEEL / 2.0, body.top() + 4.0), vec2(WHEEL, WHEEL));
        let center = wheel.center();
        let r_out = WHEEL / 2.0 - 3.0;
        let r_in = r_out - RING_THICK;
        let d = r_in - 4.0;
        let [h, s, v] = self.hsv;

        paint_ring(p, center, r_out, r_in);
        paint_diamond(p, center, d, h);
        handle(p, on_circle(center, h * TAU, (r_out + r_in) / 2.0), c32(hsv_to_rgb(h, 1.0, 1.0)));
        handle(p, diamond_pos(center, d, s, v), c32(*color));

        // Arrastrar: se decide al apoyar si se mueve el anillo (tono) o el rombo (saturacion y brillo).
        let resp = ui.interact(wheel, base.with("wheel"), Sense::click_and_drag());
        if resp.is_pointer_button_down_on() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let rel = pos - center;
                if self.drag.is_none() {
                    self.drag = Some(if rel.length() >= r_in - 2.0 { WheelDrag::Ring } else { WheelDrag::Diamond });
                }
                match self.drag {
                    Some(WheelDrag::Ring) => {
                        self.hsv[0] = (-rel.y).atan2(rel.x).rem_euclid(TAU) / TAU;
                    }
                    _ => {
                        let (x, y) = (rel.x / d, rel.y / d);
                        self.hsv[1] = ((x + y + 1.0) / 2.0).clamp(0.0, 1.0);
                        self.hsv[2] = ((x - y + 1.0) / 2.0).clamp(0.0, 1.0);
                    }
                }
                *color = hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2]);
            }
        } else if self.drag.take().is_some() {
            *done = true;
        }

        // Codigo hex editable. Va despues de la rueda para quedar encima y recibir el clic primero.
        let hex_id = base.with("hex");
        if !ui.ctx().memory(|m| m.has_focus(hex_id)) {
            self.hex = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);
        }
        let hex_rect = Rect::from_min_size(pos2(body.left() + 8.0, wheel.bottom() - 17.0), vec2(62.0, 15.0));
        let edit = egui::TextEdit::singleline(&mut self.hex)
            .id(hex_id)
            .font(FontId::proportional(11.0))
            .text_color(Color32::WHITE)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .char_limit(7)
            .desired_width(62.0);
        // Se dibuja en un Ui hijo para no mover el cursor del panel (con `ui.put` las pestanas subian y tapaban los sliders).
        let mut hex_ui = ui.new_child(egui::UiBuilder::new().max_rect(hex_rect));
        let resp = hex_ui.add(edit);
        // Mientras se escribe se aplica en cuanto hay 6 digitos; al terminar (Enter o clic fuera) tambien vale el de 3.
        let typed = if resp.changed() && self.hex.trim_start_matches('#').len() == 6 { parse_hex(&self.hex) } else { None };
        let finished = if resp.lost_focus() { parse_hex(&self.hex) } else { None };
        if let Some(c) = typed.or(finished) {
            if c != *color {
                self.set_color(c, color, done);
            }
        }
    }

    fn hsb_sliders(&mut self, ui: &mut Ui, body: Rect, base: egui::Id, color: &mut [u8; 3], done: &mut bool) {
        let top = body.top() + WHEEL + 12.0;
        let row = |i: usize| Rect::from_min_size(pos2(body.left() + 8.0, top + i as f32 * ROW_H), vec2(W - 16.0, ROW_H));
        let [h, s, v] = self.hsv;

        let mut hf = h;
        let e = slider_row(ui, row(0), base.with(("hsb", 0)), &format!("{}°", (h * 360.0).round() as i32), &mut hf, 1.0 / 360.0, |p, r| {
            let stops: Vec<Color32> = (0..=6).map(|k| c32(hsv_to_rgb(k as f32 / 6.0, 1.0, 1.0))).collect();
            pill_gradient(p, r, &stops);
        });
        if e.changed {
            self.hsv[0] = hf;
        }
        *done |= e.done;
        let mut changed = e.changed;

        let mut sf = s;
        let e = slider_row(ui, row(1), base.with(("hsb", 1)), &format!("{}%", (s * 100.0).round() as i32), &mut sf, 0.01, |p, r| {
            pill_gradient(p, r, &[c32(hsv_to_rgb(h, 0.0, v)), c32(hsv_to_rgb(h, 1.0, v))]);
        });
        if e.changed {
            self.hsv[1] = sf;
        }
        *done |= e.done;
        changed |= e.changed;

        let mut vf = v;
        let e = slider_row(ui, row(2), base.with(("hsb", 2)), &format!("{}%", (v * 100.0).round() as i32), &mut vf, 0.01, |p, r| {
            pill_gradient(p, r, &[c32(hsv_to_rgb(h, s, 0.0)), c32(hsv_to_rgb(h, s, 1.0))]);
        });
        if e.changed {
            self.hsv[2] = vf;
        }
        *done |= e.done;
        changed |= e.changed;

        if changed {
            *color = hsv_to_rgb(self.hsv[0], self.hsv[1], self.hsv[2]);
        }
    }

    fn rgb_sliders(&mut self, ui: &mut Ui, body: Rect, base: egui::Id, color: &mut [u8; 3], done: &mut bool) {
        let top = body.top() + WHEEL + 12.0;
        for k in 0..3 {
            let rect = Rect::from_min_size(pos2(body.left() + 8.0, top + k as f32 * ROW_H), vec2(W - 16.0, ROW_H));
            let c0 = *color;
            let mut f = c0[k] as f32 / 255.0;
            let e = slider_row(ui, rect, base.with(("rgb", k)), &c0[k].to_string(), &mut f, 1.0 / 255.0, move |p, r| {
                let (mut a, mut b) = (c0, c0);
                a[k] = 0;
                b[k] = 255;
                pill_gradient(p, r, &[c32(a), c32(b)]);
            });
            if e.changed {
                color[k] = (f * 255.0).round() as u8;
                self.hsv = rgb_to_hsv(*color, self.hsv[0]);
            }
            *done |= e.done;
        }
    }

    /// Cuadricula de colores y, abajo, los usados hace poco.
    fn palette_tab(&mut self, ui: &mut Ui, p: &Painter, body: Rect, base: egui::Id, color: &mut [u8; 3], done: &mut bool) {
        let cell = W / 8.0;
        let colors = palette_colors();
        let rows = colors.len().div_ceil(8);
        for (i, c) in colors.iter().enumerate() {
            let r = Rect::from_min_size(pos2(body.left() + (i % 8) as f32 * cell, body.top() + (i / 8) as f32 * cell), vec2(cell, cell));
            p.rect_filled(r, CornerRadius::ZERO, c32(*c));
            p.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_white_alpha(200)), StrokeKind::Inside);
            if ui.interact(r, base.with(("pal", i)), Sense::click()).clicked() {
                self.set_color(*c, color, done);
            }
        }
        let recent_top = body.top() + ALPHA_TOP - 4.0 - cell;
        let empty = Rect::from_min_max(pos2(body.left(), body.top() + rows as f32 * cell), pos2(body.right(), recent_top));
        if empty.height() > 0.0 {
            checker(p, empty, 12.0);
        }
        for i in 0..RECENT_MAX {
            let r = Rect::from_min_size(pos2(body.left() + i as f32 * cell, recent_top), vec2(cell, cell));
            match self.recent.get(i).copied() {
                Some(c) => {
                    p.rect_filled(r, CornerRadius::ZERO, c32(c));
                    p.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, Color32::from_white_alpha(120)), StrokeKind::Inside);
                    if ui.interact(r, base.with(("recent", i)), Sense::click()).clicked() {
                        self.set_color(c, color, done);
                    }
                }
                None => {
                    p.rect_filled(r, CornerRadius::ZERO, Color32::from_gray(40));
                }
            }
        }
    }
}
