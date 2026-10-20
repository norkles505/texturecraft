//! Panel de pinceles (como el de Ibis Paint): titulo "Pincel (N)", vista previa del pincel elegido sobre un tablero,
//! pestanas Basico / Personalizado / En linea, categorias a la izquierda, lista de pinceles con su trazo de muestra y,
//! abajo, los deslizadores de Grosor y Opacidad.
//!
//! Los pinceles son los de `brush.rs`. Los trazos de muestra se dibujan en CPU con el mismo `falloff` que usa el
//! pincel de verdad (seccion 7.5) y se guardan como texturas de egui: las de la lista una sola vez, la de la vista
//! previa de arriba cada vez que cambia el pincel, el tamano, la opacidad o el color.
//!
//! Todavia sin funcion (solo se ven): el menu de los tres puntos, la lupa, los botones "+" y ">" de cada fila y las
//! pestanas Personalizado / En linea (las dos salen vacias).

use std::collections::HashMap;

use egui::pos2;

use super::*;
use crate::brush::{Brush, BrushGroup, BrushType, MAX_RADIUS, MIN_RADIUS};

/// Medidas del panel (en puntos de egui). `PB_LEFT`/`PB_TOP` = donde se ancla (justo a la derecha de la barra de
/// herramientas, igual que el panel de color).
const PB_W: f32 = 330.0;
const PB_PAD: f32 = 6.0;
const PB_GAP: f32 = 6.0;
const PB_LEFT: f32 = 62.0;
const PB_TOP: f32 = 12.0;
const PB_TITLE_H: f32 = 32.0;
const PB_PREVIEW_H: f32 = 78.0;
const PB_TABS_H: f32 = 40.0;
/// Columna de categorias y alto maximo de cada una.
const PB_RAIL_W: f32 = 62.0;
const PB_RAIL_ITEM_H: f32 = 62.0;
/// Alto de la lista: el que quepa en la ventana, entre estos dos valores (si hay mas pinceles que filas, se desplaza).
const PB_LIST_MIN_H: f32 = 132.0;
const PB_LIST_MAX_H: f32 = 308.0;
const PB_ROW_H: f32 = 44.0;
/// Zona de los deslizadores (dos bloques iguales: titulo + fila) y alto de la fila del deslizador.
const PB_SLIDERS_H: f32 = 112.0;
const PB_SLIDER_ROW_H: f32 = 28.0;
/// Trazo de muestra de cada fila (en puntos). Las texturas se dibujan al doble para que se vean nitidas.
const THUMB_W: f32 = 176.0;
const THUMB_H: f32 = 40.0;
const TEX_SCALE: f32 = 2.0;
/// Fondo oscuro de la columna de categorias, de la zona de deslizadores y azul de la parte llena de la barra de Grosor.
const PB_DARK: Color32 = Color32::from_gray(48);
const PB_SLIDERS_FILL: Color32 = Color32::from_gray(70);
const PB_BLUE: Color32 = Color32::from_rgb(100, 160, 255);
/// UV de la textura completa.
const FULL_UV: Rect = Rect { min: pos2(0.0, 0.0), max: pos2(1.0, 1.0) };

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum PbTab {
    #[default]
    Basic,
    Custom,
    Online,
}

impl PbTab {
    const ALL: [PbTab; 3] = [PbTab::Basic, PbTab::Custom, PbTab::Online];

    fn name(self) -> &'static str {
        match self {
            PbTab::Basic => "Básico",
            PbTab::Custom => "Personalizado",
            PbTab::Online => "En línea",
        }
    }
}

/// Lo que define la imagen de la vista previa: si algo de esto cambia, se vuelve a dibujar.
type PreviewKey = (BrushType, i32, u8, [u8; 3]);

/// Estado del panel de pinceles (el que abre el boton del pincel de la barra de herramientas).
#[derive(Default)]
pub(super) struct BrushPanel {
    pub(super) open: bool,
    tab: PbTab,
    /// Categoria elegida en la columna de la izquierda (`None` = Todos).
    group: Option<BrushGroup>,
    /// Cuanto se desplazo la lista (en puntos).
    scroll: f32,
    /// Candado de la vista previa: con el encendido, al cambiar de pincel se conservan el tamano y la opacidad.
    lock: bool,
    /// Trazos de muestra de la lista (uno por pincel, al tamano inicial de cada uno).
    thumbs: HashMap<BrushType, egui::TextureHandle>,
    /// Vista previa grande de arriba.
    preview: Option<(PreviewKey, egui::TextureHandle)>,
}

/// Radio (texeles) -> posicion del deslizador (0..1). La curva cuadratica da mas precision con los tamanos chicos.
fn radius_to_frac(r: f32) -> f32 {
    ((r - MIN_RADIUS) / (MAX_RADIUS - MIN_RADIUS)).clamp(0.0, 1.0).sqrt()
}

fn frac_to_radius(f: f32) -> f32 {
    MIN_RADIUS + (MAX_RADIUS - MIN_RADIUS) * f * f
}

/// Radio con el que se dibuja el trazo de muestra, en puntos: proporcional al del pincel pero acotado para que quepa.
fn sample_radius(radius: f32, max: f32) -> f32 {
    (radius * 0.45).clamp(0.75, max)
}

/// Texto negro con contorno blanco (como el numerito del boton del pincel), legible sobre cualquier fondo.
fn outlined_text(p: &egui::Painter, pos: egui::Pos2, align: Align2, text: &str, size: f32) {
    let font = FontId::proportional(size);
    for off in [vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(0.0, -1.0), vec2(0.0, 1.0)] {
        p.text(pos + off, align, text, font.clone(), Color32::WHITE);
    }
    p.text(pos, align, text, font, Color32::BLACK);
}

/// Trazo de muestra: una onda suave dibujada con toques espaciados como los del pincel real (mismo `falloff`, mismo
/// espaciado y el maximo de opacidad por pasada, seccion 7.1). `r_pt` y `amp_pt` van en puntos; la imagen sale al
/// `TEX_SCALE`.
fn stroke_image(kind: BrushType, r_pt: f32, opacity: f32, color: [u8; 3], size: egui::Vec2, amp_pt: f32) -> egui::ColorImage {
    let w = (size.x * TEX_SCALE).round() as usize;
    let h = (size.y * TEX_SCALE).round() as usize;
    let r = r_pt * TEX_SCALE;
    let amp = amp_pt * TEX_SCALE;
    let mut alpha = vec![0.0f32; w * h];
    let spacing = (r * kind.spacing()).max(0.5);
    // Margen para que los extremos redondeados no se corten.
    let margin = (r + 6.0 * TEX_SCALE).min(w as f32 * 0.25);
    let (x0, x1) = (margin, w as f32 - margin);
    let pos = |t: f32| (x0 + (x1 - x0) * t, h as f32 * 0.5 - amp * (t * std::f32::consts::TAU * 0.95).sin());
    let mut dab = |cx: f32, cy: f32| {
        let xa = (cx - r).floor().max(0.0) as usize;
        let xb = ((cx + r).ceil() as usize).min(w - 1);
        let ya = (cy - r).floor().max(0.0) as usize;
        let yb = ((cy + r).ceil() as usize).min(h - 1);
        for y in ya..=yb {
            for x in xa..=xb {
                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                if d >= r {
                    continue;
                }
                let a = kind.falloff(1.0 - d / r) * opacity;
                let i = y * w + x;
                if a > alpha[i] {
                    alpha[i] = a;
                }
            }
        }
    };
    let mut last = pos(0.0);
    dab(last.0, last.1);
    const STEPS: usize = 3000;
    for i in 1..=STEPS {
        let q = pos(i as f32 / STEPS as f32);
        if (q.0 - last.0).hypot(q.1 - last.1) >= spacing {
            dab(q.0, q.1);
            last = q;
        }
    }
    let end = pos(1.0);
    dab(end.0, end.1);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for a in alpha {
        rgba.extend_from_slice(&[color[0], color[1], color[2], (a.clamp(0.0, 1.0) * 255.0).round() as u8]);
    }
    egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

impl BrushPanel {
    /// Dibuja el panel si esta abierto. Cambia `brush` (pincel y tamano) y `opacity` (0..1); `color` solo se usa para
    /// pintar la vista previa y la barra de opacidad. Devuelve true si algo cambio.
    pub(super) fn show(&mut self, ctx: &egui::Context, brush: &mut Brush, opacity: &mut f32, color: [u8; 3]) -> bool {
        if !self.open {
            return false;
        }
        let before = (brush.kind, brush.radius, *opacity);
        // La lista ocupa lo que sobre de la ventana (la de la referencia mide ~308 y muestra 7 filas).
        let fixed = PB_TOP + 2.0 * PB_PAD + PB_TITLE_H + PB_PREVIEW_H + PB_TABS_H + PB_SLIDERS_H + 4.0 * PB_GAP + 12.0;
        let list_h = (ctx.content_rect().height() - fixed).clamp(PB_LIST_MIN_H, PB_LIST_MAX_H);
        #[allow(deprecated)]
        let id = egui::Id::new("brush_panel");
        egui::Area::new(id).anchor(Align2::LEFT_TOP, vec2(PB_LEFT, PB_TOP)).show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(PB_PAD as i8))
                .show(ui, |ui| {
                    self.contents(ui, brush, opacity, color, list_h);
                });
        });
        (brush.kind, brush.radius, *opacity) != before
    }

    fn contents(&mut self, ui: &mut egui::Ui, brush: &mut Brush, opacity: &mut f32, color: [u8; 3], list_h: f32) {
        ui.spacing_mut().item_spacing = vec2(0.0, PB_GAP);
        let base = ui.scope_id().with("pinceles");
        let p = ui.painter().clone();
        let ctx = ui.ctx().clone();

        // ---- Titulo y menu de los tres puntos (todavia sin funcion).
        let (title, _) = ui.allocate_exact_size(vec2(PB_W, PB_TITLE_H), Sense::hover());
        p.text(
            pos2(title.left() + 6.0, title.center().y),
            Align2::LEFT_CENTER,
            format!("Pincel ({})", BrushType::ALL.len()),
            FontId::proportional(16.0),
            Color32::WHITE,
        );
        let dots_c = pos2(title.right() - 14.0, title.center().y);
        let dots = ui.interact(Rect::from_center_size(dots_c, vec2(26.0, 26.0)), base.with("dots"), Sense::click());
        let dots_color = if dots.hovered() { Color32::WHITE } else { Color32::from_gray(205) };
        for k in -1..=1 {
            p.circle_filled(dots_c + vec2(0.0, k as f32 * 6.0), 2.0, dots_color);
        }
        dots.on_hover_text("Más opciones (todavía sin función)");

        // ---- Vista previa: tablero, trazo del pincel elegido (con su tamano, opacidad y color), nombre y candado.
        let (prev, _) = ui.allocate_exact_size(vec2(PB_W, PB_PREVIEW_H), Sense::hover());
        paint_checker(&p, prev, 8.0, Color32::from_gray(250), Color32::from_gray(222));
        let key: PreviewKey = (brush.kind, (brush.radius * 10.0).round() as i32, (*opacity * 100.0).round() as u8, color);
        if self.preview.as_ref().is_none_or(|(k, _)| *k != key) {
            let img = stroke_image(
                brush.kind,
                sample_radius(brush.radius, 19.0),
                *opacity,
                color,
                vec2(PB_W, PB_PREVIEW_H),
                18.0,
            );
            match &mut self.preview {
                Some((k, tex)) => {
                    tex.set(img, egui::TextureOptions::LINEAR);
                    *k = key;
                }
                None => {
                    let tex = ctx.load_texture("pincel_vista_previa", img, egui::TextureOptions::LINEAR);
                    self.preview = Some((key, tex));
                }
            }
        }
        if let Some((_, tex)) = &self.preview {
            p.image(tex.id(), prev, FULL_UV, Color32::WHITE);
        }
        outlined_text(&p, pos2(prev.left() + 8.0, prev.bottom() - 10.0), Align2::LEFT_CENTER, brush.kind.name(), 14.0);
        // Candado (el de la barra de herramientas): encendido, al cambiar de pincel se conservan tamano y opacidad.
        let lock_c = pos2(prev.right() - 17.0, prev.top() + 17.0);
        let lock = ui.interact(Rect::from_center_size(lock_c, vec2(28.0, 28.0)), base.with("lock"), Sense::click());
        let lock_color = if self.lock {
            ACCENT
        } else if lock.hovered() {
            Color32::from_gray(30)
        } else {
            Color32::from_gray(105)
        };
        p.circle_filled(lock_c, 13.0, Color32::from_white_alpha(150));
        egui::Image::new(egui::include_image!("../../assets/icons/lock.svg"))
            .tint(lock_color)
            .paint_at(ui, Rect::from_center_size(lock_c, vec2(17.0, 17.0)));
        if lock.on_hover_text("Conservar tamaño y opacidad al cambiar de pincel").clicked() {
            self.lock = !self.lock;
        }

        // ---- Pestanas (pastilla con tres segmentos) y lupa.
        let (tabs, _) = ui.allocate_exact_size(vec2(PB_W, PB_TABS_H), Sense::hover());
        let search_r = 15.0;
        let search_c = pos2(tabs.right() - search_r - 2.0, tabs.center().y);
        let pill = Rect::from_min_max(
            pos2(tabs.left() + 2.0, tabs.center().y - 15.0),
            pos2(search_c.x - search_r - 8.0, tabs.center().y + 15.0),
        );
        let seg_w = pill.width() / 3.0;
        let pr = (pill.height() / 2.0) as u8;
        for (i, tab) in PbTab::ALL.into_iter().enumerate() {
            let seg = Rect::from_min_size(pos2(pill.left() + seg_w * i as f32, pill.top()), vec2(seg_w, pill.height()));
            let on = self.tab == tab;
            if on {
                // Solo el segmento de la punta se redondea por fuera.
                let corners = CornerRadius {
                    nw: if i == 0 { pr } else { 0 },
                    sw: if i == 0 { pr } else { 0 },
                    ne: if i == 2 { pr } else { 0 },
                    se: if i == 2 { pr } else { 0 },
                };
                p.rect_filled(seg, corners, Color32::WHITE);
            }
            let text_color = if on { Color32::from_gray(30) } else { Color32::WHITE };
            p.text(seg.center(), Align2::CENTER_CENTER, tab.name(), FontId::proportional(12.0), text_color);
            if ui.interact(seg, base.with(("tab", i)), Sense::click()).clicked() && self.tab != tab {
                self.tab = tab;
                self.scroll = 0.0;
            }
        }
        p.rect_stroke(pill, CornerRadius::same(pr), Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
        // Lupa (todavia sin funcion): circulo negro con la lupa dibujada.
        p.circle_filled(search_c, search_r, Color32::BLACK);
        let lens = search_c + vec2(-1.5, -1.5);
        p.circle_stroke(lens, 5.0, Stroke::new(1.8, Color32::WHITE));
        p.line_segment([lens + vec2(3.6, 3.6), lens + vec2(8.0, 8.0)], Stroke::new(2.0, Color32::WHITE));
        ui.interact(Rect::from_center_size(search_c, vec2(32.0, 32.0)), base.with("search"), Sense::click())
            .on_hover_text("Buscar (todavía sin función)");

        // ---- Caja: columna de categorias | lista de pinceles.
        let (body, _) = ui.allocate_exact_size(vec2(PB_W, list_h), Sense::hover());
        p.rect_filled(body, CornerRadius::same(8), PB_DARK);
        let rail = Rect::from_min_size(body.min, vec2(PB_RAIL_W, list_h));
        let list = Rect::from_min_max(pos2(rail.right(), body.top()), body.max);
        p.rect_filled(list, CornerRadius { nw: 0, sw: 0, ne: 8, se: 8 }, MENU_FILL);

        // Categorias: solo texto por ahora (en assets/icons todavia no hay iconos de categorias).
        let groups: Vec<(Option<BrushGroup>, &str)> =
            std::iter::once((None, "Todos")).chain(BrushGroup::ALL.iter().map(|g| (Some(*g), g.name()))).collect();
        let item_h = (list_h / groups.len() as f32).min(PB_RAIL_ITEM_H);
        for (i, (group, label)) in groups.iter().enumerate() {
            let item = Rect::from_min_size(pos2(rail.left(), rail.top() + i as f32 * item_h), vec2(PB_RAIL_W, item_h));
            let on = self.group == *group;
            let r = ui.interact(item, base.with(("group", i)), Sense::click());
            if on {
                p.rect_filled(item, CornerRadius::same(0), Color32::from_gray(66));
            }
            let c = if on {
                ACCENT
            } else if r.hovered() {
                Color32::WHITE
            } else {
                Color32::from_gray(205)
            };
            p.text(item.center(), Align2::CENTER_CENTER, *label, FontId::proportional(11.0), c);
            if r.clicked() && !on {
                self.group = *group;
                self.scroll = 0.0;
            }
        }

        // Lista: los pinceles de la categoria elegida (solo la pestana Basico tiene; las otras salen vacias).
        let shown: Vec<BrushType> = if self.tab == PbTab::Basic {
            BrushType::ALL.iter().copied().filter(|b| self.group.is_none_or(|g| b.group() == g)).collect()
        } else {
            Vec::new()
        };
        if shown.is_empty() {
            let msg = match self.tab {
                PbTab::Custom => "Aún no hay pinceles\npersonalizados",
                PbTab::Online => "Los pinceles en línea\nllegarán más adelante",
                PbTab::Basic => "No hay pinceles",
            };
            p.text(list.center(), Align2::CENTER_CENTER, msg, FontId::proportional(13.0), Color32::from_gray(110));
        }
        let max_scroll = (shown.len() as f32 * PB_ROW_H - list.height()).max(0.0);
        if ctx.input(|i| i.pointer.hover_pos()).is_some_and(|m| list.contains(m)) {
            self.scroll -= ctx.input(|i| i.smooth_scroll_delta.y);
        }
        self.scroll = self.scroll.clamp(0.0, max_scroll);
        let scroll = self.scroll;
        let pa = p.with_clip_rect(list);
        let mut picked: Option<BrushType> = None;
        for (i, kind) in shown.iter().copied().enumerate() {
            let row = Rect::from_min_size(
                pos2(list.left(), list.top() - scroll + i as f32 * PB_ROW_H),
                vec2(list.width(), PB_ROW_H),
            );
            if row.bottom() < list.top() || row.top() > list.bottom() {
                continue;
            }
            let rr = ui.interact(row.intersect(list), base.with(("row", kind as u8)), Sense::click());
            let selected = brush.kind == kind;
            if selected {
                pa.rect_filled(row, CornerRadius::same(0), LP_SELECTED);
            } else if rr.hovered() {
                pa.rect_filled(row, CornerRadius::same(0), Color32::from_black_alpha(10));
            }
            // Trazo de muestra (negro, al tamano inicial del pincel), dibujado una sola vez.
            let thumb = Rect::from_min_size(pos2(row.left() + 8.0, row.top() + 2.0), vec2(THUMB_W, THUMB_H));
            let tex = self.thumbs.entry(kind).or_insert_with(|| {
                let img = stroke_image(
                    kind,
                    sample_radius(kind.default_radius(), 11.0),
                    1.0,
                    [0, 0, 0],
                    vec2(THUMB_W, THUMB_H),
                    8.0,
                );
                ctx.load_texture(format!("pincel_{}", kind.name()), img, egui::TextureOptions::LINEAR)
            });
            pa.image(tex.id(), thumb, FULL_UV, Color32::WHITE);
            // Nombre abajo a la izquierda y tamano arriba a la derecha (el de la fila elegida es el actual).
            pa.text(
                pos2(row.left() + 10.0, row.bottom() - 9.0),
                Align2::LEFT_CENTER,
                kind.name(),
                FontId::proportional(11.0),
                MENU_TEXT,
            );
            let shown_size = if selected { brush.radius } else { kind.default_radius() };
            pa.text(
                pos2(thumb.right(), row.top() + 9.0),
                Align2::RIGHT_CENTER,
                format!("{shown_size:.1}"),
                FontId::proportional(11.0),
                MENU_TEXT,
            );
            // Botones "+" y ">" (todavia sin funcion). Se crean despues de la fila para ganarle el clic.
            for (name, c, tip) in [
                ("plus", pos2(row.right() - 46.0, row.center().y), "Añadir a mis pinceles (todavía sin función)"),
                ("go", pos2(row.right() - 18.0, row.center().y), "Ajustes del pincel (todavía sin función)"),
            ] {
                let r = ui.interact(
                    Rect::from_center_size(c, vec2(26.0, 26.0)).intersect(list),
                    base.with((name, kind as u8)),
                    Sense::click(),
                );
                let col = if r.hovered() { Color32::from_gray(20) } else { Color32::from_gray(70) };
                let line = Stroke::new(1.4, col);
                pa.circle_stroke(c, 11.0, Stroke::new(1.3, col));
                if name == "plus" {
                    pa.line_segment([c - vec2(4.5, 0.0), c + vec2(4.5, 0.0)], line);
                    pa.line_segment([c - vec2(0.0, 4.5), c + vec2(0.0, 4.5)], line);
                } else {
                    pa.line_segment([c + vec2(-2.0, -4.5), c + vec2(2.5, 0.0)], line);
                    pa.line_segment([c + vec2(2.5, 0.0), c + vec2(-2.0, 4.5)], line);
                }
                r.on_hover_text(tip);
            }
            pa.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, MENU_LINE));
            if rr.clicked() {
                picked = Some(kind);
            }
        }
        // Barrita de desplazamiento, solo si la lista no cabe.
        if max_scroll > 0.0 {
            let track_h = list.height() - 8.0;
            let bar_h = (track_h * list.height() / (shown.len() as f32 * PB_ROW_H)).max(16.0);
            let y = list.top() + 4.0 + (track_h - bar_h) * (scroll / max_scroll);
            let bar = Rect::from_min_size(pos2(list.right() - 4.0, y), vec2(3.0, bar_h));
            p.rect_filled(bar, CornerRadius::same(2), Color32::from_black_alpha(70));
        }
        if let Some(kind) = picked {
            if brush.kind != kind {
                brush.kind = kind;
                // Con el candado puesto se queda el tamano; si no, el pincel trae el suyo.
                if !self.lock {
                    brush.radius = kind.default_radius();
                }
            }
        }

        // ---- Deslizadores: Grosor (radio en texeles) y Opacidad. El titulo y el valor van arriba de cada fila.
        let (sl, _) = ui.allocate_exact_size(vec2(PB_W, PB_SLIDERS_H), Sense::hover());
        p.rect_filled(sl, CornerRadius::same(8), PB_SLIDERS_FILL);
        let block_h = PB_SLIDERS_H / 2.0;
        let titles = [("Grosor", format!("{:.1}px", brush.radius)), ("Opacidad", format!("{}%", (*opacity * 100.0).round() as i32))];
        for (k, (name, value)) in titles.iter().enumerate() {
            let top = sl.top() + block_h * k as f32;
            p.text(pos2(sl.left() + 12.0, top + 16.0), Align2::LEFT_CENTER, *name, FontId::proportional(14.0), Color32::WHITE);
            p.text(pos2(sl.right() - 12.0, top + 16.0), Align2::RIGHT_CENTER, value, FontId::proportional(13.0), Color32::WHITE);
        }
        let row = |k: usize| {
            Rect::from_min_size(
                pos2(sl.left() + 8.0, sl.top() + block_h * k as f32 + 22.0),
                vec2(PB_W - 16.0, PB_SLIDER_ROW_H),
            )
        };

        let mut f = radius_to_frac(brush.radius);
        let f0 = f;
        let e = slider_row_with(ui, row(0), base.with("size"), "", 0.0, &mut f, 0.02, move |p, r| {
            pill_gradient(p, r, &[Color32::from_gray(205), Color32::from_gray(205)]);
            // Parte llena (azul) hasta la perilla; si es mas corta que la barra de alta no se dibuja.
            let w = r.width() * f0;
            if w >= r.height() {
                pill_gradient(p, Rect::from_min_max(r.min, pos2(r.left() + w, r.max.y)), &[PB_BLUE, PB_BLUE]);
            }
        });
        if e.changed {
            // Un decimal, como se muestra.
            brush.radius = (frac_to_radius(f) * 10.0).round() / 10.0;
        }

        let opaque = Color32::from_rgb(color[0], color[1], color[2]);
        let mut o = *opacity;
        let e = slider_row_with(ui, row(1), base.with("alpha"), "", 0.0, &mut o, 0.01, move |p, r| {
            pill_checker(p, r, 3.5);
            pill_gradient(p, r, &[Color32::TRANSPARENT, opaque]);
        });
        if e.changed {
            *opacity = o;
        }
    }
}
