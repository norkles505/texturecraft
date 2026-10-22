//! Pantalla "Mi galería": el lugar donde estan los proyectos, pensada para ser lo primero que se vea al abrir la app.
//!
//! Se dibuja cada cuadro desde `UiLayer::run` mientras `open` sea true (encima de todo, tapando el resto de la
//! interfaz). Muestra una tarjeta con la miniatura y el nombre de cada proyecto guardado (`crate::proyecto::list`); la
//! lista se relee al abrir la app y despues de cada guardado (`reload`). El "+" abre el selector de archivos, y al
//! elegir un .obj cierra la galeria y entrega el modelo por `UiLayer::take_import` (como antes `cargo run -- modelo.obj`).
//! "Regresar a mi galería" (menu) guarda el proyecto y vuelve aqui. Al pulsar la tarjeta de un proyecto guardado se
//! cierra la galeria y se entrega su id por `UiLayer::take_open_project` (`main.rs` lo abre). El resto de los botones
//! solo se resaltan al pasar el raton.

#![allow(dead_code)]

use super::*;
use crate::proyecto;

/// Fondo crema de la pantalla, barra superior oscura y su alto.
const GAL_BG: Color32 = Color32::from_rgb(244, 244, 232);
const GAL_BAR: Color32 = Color32::from_rgb(43, 43, 43);
const GAL_BAR_H: f32 = 44.0;
/// Texto gris del cuerpo, iconos y rotulos de la barra, y tamano de letra.
const GAL_TEXT: Color32 = Color32::from_gray(105);
const GAL_ICON: Color32 = Color32::from_gray(160);
const GAL_LABEL: Color32 = Color32::from_gray(205);
const GAL_FONT: f32 = 15.0;
/// Tarjeta de proyecto: ancho (y alto) de la miniatura, alto de la fila del nombre y separacion entre tarjetas.
const CARD_SIDE: f32 = 180.0;
const CARD_LABEL_H: f32 = 28.0;
const CARD_GAP: f32 = 20.0;

/// Tarjeta de un proyecto guardado: su carpeta (id), su nombre y su miniatura ya subida como textura de egui.
pub(super) struct Card {
    id: String,
    name: String,
    thumb: Option<egui::TextureHandle>,
}

/// Lee los proyectos guardados en disco y sube cada miniatura como textura.
fn load_cards(ctx: &egui::Context) -> Vec<Card> {
    proyecto::list()
        .into_iter()
        .map(|e| {
            let thumb = e.thumb.map(|(w, h, rgba)| {
                ctx.load_texture(
                    format!("proyecto_{}", e.id),
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba),
                    egui::TextureOptions::LINEAR,
                )
            });
            Card {
                id: e.id,
                name: e.name,
                thumb,
            }
        })
        .collect()
}

/// Dibuja una tarjeta: miniatura con borde (azul al pasar el raton) y el nombre debajo, centrado. Devuelve su respuesta
/// (`clicked()` = abrir ese proyecto).
fn card_ui(ui: &mut egui::Ui, card: &Card) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(CARD_SIDE, CARD_SIDE + CARD_LABEL_H),
        Sense::click(),
    );
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    let thumb = Rect::from_min_size(rect.min, vec2(CARD_SIDE, CARD_SIDE));
    p.rect_filled(thumb, CornerRadius::same(4), Color32::from_gray(225));
    if let Some(tex) = &card.thumb {
        p.image(
            tex.id(),
            thumb,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let border = if response.hovered() {
        ACCENT
    } else {
        Color32::from_gray(200)
    };
    p.rect_stroke(
        thumb,
        CornerRadius::same(4),
        Stroke::new(1.5, border),
        StrokeKind::Inside,
    );
    // Nombre en una sola linea: si es largo se corta con puntos suspensivos.
    let name = if card.name.chars().count() > 22 {
        let short: String = card.name.chars().take(21).collect();
        format!("{short}…")
    } else {
        card.name.clone()
    };
    p.text(
        egui::pos2(rect.center().x, thumb.bottom() + CARD_LABEL_H / 2.0),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(GAL_FONT),
        GAL_TEXT,
    );
    response
}

/// Estado de la galeria.
pub(super) struct GalleryState {
    /// Proyectos guardados (uno por tarjeta); el titulo muestra "Mi galería (N)" con su cantidad.
    pub(super) cards: Vec<Card>,
    /// Hay que volver a leer los proyectos de disco (empieza en true; `UiLayer::reload_gallery` lo vuelve a poner
    /// despues de guardar). Se atiende la proxima vez que la galeria se dibuja.
    pub(super) reload: bool,
    /// Interruptor "Sincronización en la nube" (apagado por defecto). Todavia solo se muestra: no sincroniza nada.
    pub(super) cloud_sync: bool,
    /// La galeria se esta mostrando (empieza en true: es lo primero que se ve). Al crear un proyecto con "+" pasa a false.
    pub(super) open: bool,
    /// Mientras el selector de archivos del "+" esta abierto: canal por donde llega la ruta elegida (`None` = cancelado).
    pick: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    /// El .obj elegido con el "+", a la espera de que `UiLayer::take_import` lo entregue (una sola vez).
    pub(super) pending_import: Option<std::path::PathBuf>,
    /// Proyecto guardado (su id) al que se le hizo clic en la galeria, a la espera de que `UiLayer::take_open_project`
    /// lo entregue (una sola vez).
    pub(super) pending_open: Option<String>,
}

impl Default for GalleryState {
    fn default() -> Self {
        GalleryState {
            cards: Vec::new(),
            reload: true,
            cloud_sync: false,
            open: true,
            pick: None,
            pending_import: None,
            pending_open: None,
        }
    }
}

/// Abre el selector de archivos en otro hilo (para no congelar la interfaz); la ruta llega por el canal.
fn start_new_project(ctx: &egui::Context, state: &mut GalleryState) {
    if state.pick.is_some() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let repaint_ctx = ctx.clone();
    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .set_title("Nuevo proyecto: elige un modelo")
            .add_filter("Modelo OBJ", &["obj"])
            .pick_file();
        let _ = tx.send(path);
        repaint_ctx.request_repaint();
    });
    state.pick = Some(rx);
}

/// Si el selector del "+" ya devolvio algo: deja la ruta en `pending_import` y cierra la galeria (el proyecto nuevo
/// es el modelo elegido, como cuando se abria con `cargo run -- modelo.obj`). Si se cancelo, no cambia nada.
fn poll_new_project(ctx: &egui::Context, state: &mut GalleryState) {
    let Some(rx) = state.pick.as_ref() else {
        return;
    };
    match rx.try_recv() {
        Ok(path) => {
            state.pick = None;
            if path.is_some() {
                state.pending_import = path;
                state.open = false;
                // Que el cuadro siguiente ya cargue el modelo.
                ctx.request_repaint();
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => state.pick = None,
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }
}

/// Iconos de la barra superior, dibujados con lineas (no usan archivos SVG).
#[derive(Clone, Copy)]
enum GalIcon {
    Plus,
    Info,
    Play,
    Share,
    Dots,
}

fn paint_icon(p: &egui::Painter, icon: GalIcon, c: egui::Pos2, color: Color32) {
    let s = Stroke::new(1.6, color);
    let at = |x: f32, y: f32| c + vec2(x, y);
    let pts = |v: &[(f32, f32)]| v.iter().map(|&(x, y)| at(x, y)).collect::<Vec<_>>();
    match icon {
        GalIcon::Plus => {
            p.line_segment([at(-8.0, 0.0), at(8.0, 0.0)], s);
            p.line_segment([at(0.0, -8.0), at(0.0, 8.0)], s);
        }
        GalIcon::Info => {
            p.circle_stroke(c, 9.5, Stroke::new(1.3, color));
            p.circle_filled(at(0.0, -4.0), 1.2, color);
            p.line_segment([at(0.0, -0.5), at(0.0, 5.0)], Stroke::new(1.5, color));
        }
        GalIcon::Play => {
            p.add(egui::Shape::convex_polygon(
                pts(&[(-5.0, -8.0), (-5.0, 8.0), (8.0, 0.0)]),
                color,
                Stroke::NONE,
            ));
        }
        GalIcon::Share => {
            // Caja abierta arriba a la derecha y una flecha que sale hacia esa esquina.
            p.add(egui::Shape::line(
                pts(&[(-2.0, -6.0), (-8.0, -6.0), (-8.0, 8.0), (8.0, 8.0), (8.0, 2.0)]),
                s,
            ));
            p.add(egui::Shape::line(
                pts(&[(-3.0, 3.0), (-1.0, -2.0), (3.0, -5.0), (8.0, -5.0)]),
                s,
            ));
            p.add(egui::Shape::line(
                pts(&[(4.0, -9.0), (8.0, -5.0), (4.0, -1.0)]),
                s,
            ));
        }
        GalIcon::Dots => {
            for dx in [-7.0, 0.0, 7.0] {
                p.circle_filled(at(dx, 0.0), 1.9, color);
            }
        }
    }
}

/// Boton de icono de la barra superior (40 puntos de ancho, todo el alto de la barra). Se pone blanco al pasar el raton.
fn icon_button(
    ui: &egui::Ui,
    center: egui::Pos2,
    icon: GalIcon,
    base: Color32,
    name: &str,
) -> egui::Response {
    let hit = Rect::from_center_size(center, vec2(40.0, GAL_BAR_H));
    let response = ui.interact(hit, ui.id().with(name), Sense::click());
    let color = if response.hovered() {
        Color32::WHITE
    } else {
        base
    };
    paint_icon(ui.painter(), icon, center, color);
    response
}

/// Boton de texto de la barra superior: ocupa `width` puntos terminando en `right`, con el texto pegado a la derecha.
fn text_button(
    ui: &egui::Ui,
    bar: Rect,
    right: f32,
    width: f32,
    label: &str,
    name: &str,
) -> egui::Response {
    let hit = Rect::from_min_max(
        egui::pos2(right - width, bar.top()),
        egui::pos2(right, bar.bottom()),
    );
    let response = ui.interact(hit, ui.id().with(name), Sense::click());
    let color = if response.hovered() {
        Color32::WHITE
    } else {
        GAL_LABEL
    };
    ui.painter().text(
        egui::pos2(right - 8.0, bar.center().y),
        Align2::RIGHT_CENTER,
        label,
        FontId::proportional(GAL_FONT),
        color,
    );
    response
}

/// Dibuja la pantalla de la galeria ocupando toda la ventana.
pub(super) fn gallery_screen(ctx: &egui::Context, state: &mut GalleryState) {
    if !state.open {
        return;
    }
    poll_new_project(ctx, state);
    // Si el selector acaba de devolver un modelo, la galeria ya se cerro: no se dibuja este cuadro.
    if !state.open {
        return;
    }
    // Hay que releer los proyectos de disco (al abrir la app y despues de cada guardado).
    if state.reload {
        state.cards = load_cards(ctx);
        state.reload = false;
    }
    let screen = ctx.content_rect();
    #[allow(deprecated)]
    let id = egui::Id::new("gallery_screen");
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let p = ui.painter().clone();
            // Fondo de toda la pantalla; ademas se come los clics para que no lleguen a lo que hay debajo.
            let _ = ui.allocate_rect(screen, Sense::click());
            p.rect_filled(screen, CornerRadius::ZERO, GAL_BG);

            // ---- Barra superior ----
            let bar = Rect::from_min_size(screen.min, vec2(screen.width(), GAL_BAR_H));
            p.rect_filled(bar, CornerRadius::ZERO, GAL_BAR);
            let cy = bar.center().y;

            // Atras: flecha y texto en azul, a la izquierda.
            let back = ui.interact(
                Rect::from_min_size(bar.min, vec2(80.0, GAL_BAR_H)),
                ui.id().with("gal_back"),
                Sense::click(),
            );
            let blue = if back.hovered() {
                Color32::from_rgb(110, 190, 255)
            } else {
                ACCENT
            };
            let c = egui::pos2(bar.left() + 18.0, cy);
            p.add(egui::Shape::line(
                vec![
                    c + vec2(3.5, -8.0),
                    c + vec2(-3.5, 0.0),
                    c + vec2(3.5, 8.0),
                ],
                Stroke::new(1.8, blue),
            ));
            p.text(
                egui::pos2(bar.left() + 30.0, cy),
                Align2::LEFT_CENTER,
                "Atrás",
                FontId::proportional(GAL_FONT),
                blue,
            );

            // Titulo centrado, en negrita (se dibuja dos veces con medio punto de diferencia, como el de Ajustes).
            let title = format!("Mi galería ({})", state.cards.len());
            for dx in [0.0, 0.6] {
                p.text(
                    egui::pos2(bar.center().x + dx, cy),
                    Align2::CENTER_CENTER,
                    &title,
                    FontId::proportional(17.0),
                    Color32::WHITE,
                );
            }

            // Derecha, de derecha a izquierda: Editar, Seleccionar, tres puntos, compartir, reproducir, info y "+".
            let mut x = bar.right() - 4.0;
            let _edit = text_button(ui, bar, x, 60.0, "Editar", "gal_edit");
            x -= 60.0;
            let _select = text_button(ui, bar, x, 100.0, "Seleccionar", "gal_select");
            x -= 100.0;
            for (icon, color, name) in [
                (GalIcon::Dots, GAL_ICON, "gal_more"),
                (GalIcon::Share, GAL_ICON, "gal_share"),
                (GalIcon::Play, GAL_ICON, "gal_play"),
                (GalIcon::Info, GAL_ICON, "gal_info"),
                (GalIcon::Plus, ACCENT, "gal_new"),
            ] {
                let r = icon_button(ui, egui::pos2(x - 20.0, cy), icon, color, name);
                // "+": nuevo proyecto = elegir un modelo .obj (lo mismo que antes se hacia con `cargo run`).
                if name == "gal_new" {
                    let waiting = state.pick.is_some();
                    let r = r.on_hover_text(if waiting {
                        "Elige un archivo…"
                    } else {
                        "Nuevo proyecto: elegir un modelo (.obj)"
                    });
                    if r.clicked() {
                        start_new_project(ctx, state);
                    }
                }
                x -= 40.0;
            }

            // ---- Sincronizacion en la nube: texto, interruptor e icono de info, centrados bajo la barra ----
            let cx = bar.center().x;
            let row_y = bar.bottom() + 20.0;
            p.text(
                egui::pos2(cx + 24.0, row_y),
                Align2::RIGHT_CENTER,
                "Sincronización en la nube",
                FontId::proportional(GAL_FONT),
                GAL_TEXT,
            );
            let toggle = Rect::from_center_size(
                egui::pos2(cx + 53.0, row_y),
                vec2(TOGGLE_W, TOGGLE_H),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(toggle), |ui| {
                toggle_switch(ui, &mut state.cloud_sync);
            });
            paint_icon(
                &p,
                GalIcon::Info,
                egui::pos2(cx + 100.0, row_y),
                Color32::from_gray(120),
            );

            // ---- Cuerpo: sin proyectos, un mensaje en el centro; con proyectos, la cuadricula de tarjetas ----
            if state.cards.is_empty() {
                let mid = egui::pos2(cx, (bar.bottom() + screen.bottom()) / 2.0);
                p.text(
                    mid - vec2(0.0, 10.0),
                    Align2::CENTER_CENTER,
                    "No hay ningún proyecto.",
                    FontId::proportional(GAL_FONT),
                    GAL_TEXT,
                );
                p.text(
                    mid + vec2(0.0, 10.0),
                    Align2::CENTER_CENTER,
                    "Pulsa el botón '+' para crear un proyecto nuevo.",
                    FontId::proportional(GAL_FONT),
                    GAL_TEXT,
                );
            } else {
                // Tarjetas en filas que se ajustan al ancho de la ventana, con barra de desplazamiento si no caben.
                let body = Rect::from_min_max(
                    egui::pos2(screen.left() + CARD_GAP, row_y + 28.0),
                    egui::pos2(screen.right() - CARD_GAP, screen.bottom()),
                );
                let mut clicked: Option<String> = None;
                ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
                    ui.spacing_mut().item_spacing = vec2(CARD_GAP, CARD_GAP);
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for card in &state.cards {
                                    if card_ui(ui, card).clicked() {
                                        clicked = Some(card.id.clone());
                                    }
                                }
                            });
                        });
                });
                // Clic en una miniatura: se cierra la galeria y `main.rs` abre ese proyecto.
                if let Some(id) = clicked {
                    state.pending_open = Some(id);
                    state.open = false;
                    ctx.request_repaint();
                }
            }
        });
}
