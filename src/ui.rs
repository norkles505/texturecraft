//! Interfaz (egui): barra de herramientas vertical pegada al borde izquierdo, dibujada encima del modelo.
//!
//! Los iconos son archivos SVG en `assets/icons/` (se cargan con `include_image!`, es decir, van dentro del
//! ejecutable). Para cambiar un icono basta con reemplazar su archivo. Conviene que sean de un solo color
//! (blanco) y fondo transparente: la barra los tiñe de gris, blanco o azul segun el estado del boton.

use std::time::Duration;

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Rect, Sense, Stroke, StrokeKind, vec2};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::Tool;
use crate::camera::{Camera, ViewAxis};
use crate::color_panel::ColorPanel;

/// Azul del boton activo (como el "Brush" de la referencia).
const ACCENT: Color32 = Color32::from_rgb(33, 150, 243);
/// Opacidad del fondo de las barras (0 = invisible, 255 = solido).
const BAR_ALPHA: u8 = 150;
/// Fondo de las barras: gris oscuro semitransparente.
fn bar_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(58, 58, 60, BAR_ALPHA)
}
/// Distancia de la barra al borde superior de la ventana (por la izquierda va pegada al borde).
const BAR_TOP: f32 = 12.0;
/// Radio de las esquinas del lado derecho de la barra.
const BAR_RADIUS: u8 = 10;
/// Ancho y alto de cada boton, lado del icono y tamano del texto (en puntos de egui). El ancho de la barra es el del
/// boton: si un nombre no cabe, se achica `LABEL_SIZE` o se sube `BTN_W`.
const BTN_W: f32 = 54.0;
const BTN_H: f32 = 46.0;
const ICON_SIZE: f32 = 22.0;
const LABEL_SIZE: f32 = 9.0;
/// Tamano del cuadrito de color al pie de la barra.
const SWATCH_W: f32 = 30.0;
const SWATCH_H: f32 = 30.0;
/// Alto de la barra inferior.
const BOTTOM_BAR_H: f32 = 40.0;
/// Boton de capas de la barra inferior: tamano del boton y del icono.
const LAYERS_BTN_W: f32 = 48.0;
const LAYERS_BTN_H: f32 = 36.0;
const LAYERS_ICON: f32 = 31.0;
/// Radio del circulo del boton del panel de pinceles.
const BRUSH_BTN_R: f32 = 14.0;

/// Un boton de la barra: herramienta, nombre visible, icono y atajo de teclado (para el tooltip).
struct ToolButton {
    tool: Tool,
    label: &'static str,
    key: &'static str,
    icon: egui::ImageSource<'static>,
}

/// Botones de la barra, de arriba abajo. Para agregar uno: un `Tool` nuevo, su SVG y una linea aqui.
fn tool_buttons() -> [ToolButton; 7] {
    [
        ToolButton {
            tool: Tool::Brush,
            label: "Pincel",
            key: "P",
            icon: egui::include_image!("../assets/icons/brush.svg"),
        },
        ToolButton {
            tool: Tool::Eraser,
            label: "Borrador",
            key: "G",
            icon: egui::include_image!("../assets/icons/eraser.svg"),
        },
        ToolButton {
            tool: Tool::Fill,
            label: "Relleno",
            key: "F",
            icon: egui::include_image!("../assets/icons/bucket.svg"),
        },
        ToolButton {
            tool: Tool::Eyedropper,
            label: "Cuentagotas",
            key: "I",
            icon: egui::include_image!("../assets/icons/eyedropper.svg"),
        },
        ToolButton {
            tool: Tool::Smudge,
            label: "Difuminar",
            key: "D",
            icon: egui::include_image!("../assets/icons/smudge.svg"),
        },
        ToolButton {
            tool: Tool::Blur,
            label: "Borrosidad",
            key: "U",
            icon: egui::include_image!("../assets/icons/blur.svg"),
        },
        ToolButton {
            tool: Tool::Hand,
            label: "Mano",
            key: "H",
            icon: egui::include_image!("../assets/icons/hand.svg"),
        },
    ]
}

/// Dibuja un boton (icono arriba, nombre abajo) y devuelve true si se hizo clic.
fn tool_button(ui: &mut egui::Ui, b: &ToolButton, active: bool) -> bool {
    icon_button(ui, b.icon.clone(), b.label, &format!("{} ({})", b.label, b.key), active)
}

/// Boton generico de la barra (icono arriba, nombre abajo, tooltip). Lo usan las herramientas y el candado de la vista.
fn icon_button(ui: &mut egui::Ui, icon: egui::ImageSource<'static>, label: &str, tooltip: &str, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(BTN_W, BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered && !active {
            ui.painter().rect_filled(rect, CornerRadius::same(8), Color32::from_white_alpha(14));
        }
        let color = if active {
            ACCENT
        } else if hovered {
            Color32::WHITE
        } else {
            Color32::from_gray(205)
        };
        let icon_rect = Rect::from_center_size(
            egui::pos2(rect.center().x, rect.top() + 6.0 + ICON_SIZE * 0.5),
            vec2(ICON_SIZE, ICON_SIZE),
        );
        egui::Image::new(icon).tint(color).paint_at(ui, icon_rect);
        ui.painter().text(
            egui::pos2(rect.center().x, rect.bottom() - 4.0),
            Align2::CENTER_BOTTOM,
            label,
            FontId::proportional(LABEL_SIZE),
            color,
        );
    }
    response.on_hover_text(tooltip).clicked()
}

/// Boton del panel de pinceles: circulo negro con aro claro y, encima, el tamano del pincel con contorno.
/// Por ahora solo se muestra (todavia no abre ningun panel).
fn brush_button(ui: &mut egui::Ui, size: f32) {
    let (rect, response) = ui.allocate_exact_size(vec2(BTN_W, BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let ring = if response.hovered() { Color32::WHITE } else { Color32::from_gray(205) };
        let center = rect.center();
        let p = ui.painter();
        p.circle_filled(center, BRUSH_BTN_R, Color32::BLACK);
        p.circle_stroke(center, BRUSH_BTN_R - 1.0, Stroke::new(2.0, ring));
        // Texto negro con contorno blanco, un poco abajo a la derecha como en la referencia.
        let pos = center + vec2(3.0, 5.0);
        let font = FontId::proportional(10.0);
        let label = format!("{size:.1}");
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
    response.on_hover_text("Pinceles");
}

/// Cuadrito de color al pie de la barra: muestra el color del pincel y, al hacer clic, abre o cierra el panel de color.
fn color_swatch(ui: &mut egui::Ui, color: [u8; 3], panel: &mut ColorPanel) {
    let (rect, _) = ui.allocate_exact_size(vec2(BTN_W, SWATCH_H + 12.0), Sense::hover());
    let swatch = Rect::from_center_size(rect.center(), vec2(SWATCH_W, SWATCH_H));
    let response = ui.interact(swatch, ui.scope_id().with("swatch"), Sense::click());
    let ring = if panel.open {
        ACCENT
    } else if response.hovered() {
        Color32::WHITE
    } else {
        Color32::from_gray(205)
    };
    ui.painter().rect_filled(swatch, CornerRadius::same(4), Color32::from_rgb(color[0], color[1], color[2]));
    ui.painter().rect_stroke(swatch, CornerRadius::same(4), Stroke::new(2.0, ring), StrokeKind::Inside);
    if response.on_hover_text("Color (clic para abrir el panel)").clicked() {
        panel.open = !panel.open;
    }
}

/// Boton de capas de la barra inferior: el icono de hojas apiladas con el numero de capas sobre la hoja de enfrente.
/// Por ahora no abre nada: el panel de capas viene despues.
fn layers_button(ui: &mut egui::Ui, count: usize) {
    let (rect, response) = ui.allocate_exact_size(vec2(LAYERS_BTN_W, LAYERS_BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if hovered { Color32::WHITE } else { Color32::from_gray(205) };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(LAYERS_ICON, LAYERS_ICON));
        egui::Image::new(egui::include_image!("../assets/icons/layers.svg")).tint(color).paint_at(ui, icon_rect);
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
    response.on_hover_text("Capas");
}

/// Estado del panel de opciones (el que abre el boton de los interruptores de la barra inferior).
#[derive(Default)]
struct OptionsPanel {
    open: bool,
    /// Interruptor "Ventana de referencia": mientras esta encendido se muestra la ventana flotante.
    reference_window: bool,
    /// Esquina superior izquierda de la ventana de referencia (`None` = aun no se coloco; se pone la primera vez).
    reference_pos: Option<egui::Pos2>,
    /// Tamano de la ventana de referencia (`None` = el inicial, `REF_SIZE` cuadrado). Se cambia estirando los bordes.
    reference_size: Option<egui::Vec2>,
    /// Imagen de referencia ya cargada (textura de egui), si se eligio alguna.
    reference_image: Option<egui::TextureHandle>,
    /// Pixeles de esa imagen, para el cuentagotas.
    reference_pixels: Option<std::sync::Arc<RefPixels>>,
    /// Zoom de la imagen dentro del cuadro (`None` = 1, ajustada al cuadro) y cuanto se corrio del centro (arrastrando).
    reference_zoom: Option<f32>,
    reference_offset: egui::Vec2,
    /// Mientras el selector de archivos esta abierto: canal por donde llega la ruta elegida (`None` = cancelado).
    reference_pick: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
}

/// Tamano del interruptor del panel de opciones.
const TOGGLE_W: f32 = 46.0;
const TOGGLE_H: f32 = 26.0;

/// Boton de opciones de la barra inferior (a la izquierda del cubo): icono de dos interruptores. Se pinta de azul
/// mientras su panel esta abierto. Devuelve la respuesta para que la barra abra o cierre el panel.
fn options_button(ui: &mut egui::Ui, open: bool) -> egui::Response {
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
        egui::Image::new(egui::include_image!("../assets/icons/toggles.svg")).tint(color).paint_at(ui, icon_rect);
    }
    response.on_hover_text("Opciones")
}

/// Interruptor: pastilla con perilla blanca. Apagado = gris con la perilla a la izquierda; encendido = azul con la
/// perilla a la derecha (la perilla se desliza con una pequena animacion).
fn toggle_switch(ui: &mut egui::Ui, on: &mut bool) {
    let (rect, response) = ui.allocate_exact_size(vec2(TOGGLE_W, TOGGLE_H), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let p = ui.painter();
    let track = Color32::from_gray(150).lerp_to_gamma(ACCENT, t);
    p.rect_filled(rect, CornerRadius::same((TOGGLE_H / 2.0) as u8), track);
    let r = TOGGLE_H / 2.0 - 3.0;
    let x = egui::lerp((rect.left() + TOGGLE_H / 2.0)..=(rect.right() - TOGGLE_H / 2.0), t);
    p.circle_filled(egui::pos2(x, rect.center().y), r, Color32::WHITE);
}

/// Ventana de referencia: lado del cuadro blanco, radio y separacion de los botones redondos de arriba, y alto de su
/// centro y del asa respecto al borde superior de la ventana.
const REF_SIZE: f32 = 240.0;
const REF_BTN_R: f32 = 19.0;
const REF_BTN_PAD: f32 = 4.0;
const REF_BTN_CY: f32 = 52.0;
const REF_HANDLE_CY: f32 = 15.0;
/// Lado del icono de la camara dentro de su circulo.
const REF_ICON: f32 = 30.0;
/// Tamano minimo al estirar (que quepan el asa y los dos botones), mitad del grosor de la zona de agarre de los
/// bordes y mitad del lado de la zona de las esquinas.
const REF_MIN_W: f32 = 140.0;
const REF_MIN_H: f32 = 120.0;
const REF_EDGE: f32 = 5.0;
const REF_CORNER: f32 = 10.0;
/// Lado mas largo al que se reduce una imagen de referencia al cargarla, y margen entre la imagen y el borde del cuadro.
const REF_IMG_MAX: u32 = 2048;
const REF_IMG_PAD: f32 = 4.0;
/// Zoom de la imagen con la rueda: sensibilidad (por punto de desplazamiento), minimo y maximo (1 = ajustada al cuadro).
const REF_ZOOM_SPEED: f32 = 0.002;
const REF_ZOOM_MIN: f32 = 0.25;
const REF_ZOOM_MAX: f32 = 16.0;

/// Pixeles (RGBA, sin premultiplicar) de la imagen de referencia ya reducida, para que el cuentagotas lea su color.
struct RefPixels {
    size: [usize; 2],
    rgba: Vec<u8>,
}

/// Lee una imagen del disco y la sube a egui como textura. Las muy grandes se reducen a `REF_IMG_MAX`.
fn load_reference_image(ctx: &egui::Context, path: &std::path::Path) -> Result<(egui::TextureHandle, RefPixels), String> {
    let mut img = image::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if img.width().max(img.height()) > REF_IMG_MAX {
        img = img.resize(REF_IMG_MAX, REF_IMG_MAX, image::imageops::FilterType::Triangle);
    }
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let tex = ctx.load_texture("reference_image", color, egui::TextureOptions::LINEAR);
    Ok((tex, RefPixels { size, rgba: rgba.into_raw() }))
}

/// Ventana de referencia (la que enciende el interruptor del panel de opciones): cuadro blanco flotante con un asa
/// arriba para moverla, un boton de camara a la izquierda y un boton X a la derecha que la cierra.
/// Al pulsar la camara (`assets/icons/camera.svg`) se elige una imagen del disco y se muestra dentro del cuadro.
fn reference_window(ctx: &egui::Context, options: &mut OptionsPanel, tool: Tool, color: &mut [u8; 3]) -> bool {
    // Si el selector de archivos ya devolvio algo, se carga la imagen (aunque la ventana este oculta).
    let picked = options.reference_pick.as_ref().map(|rx| rx.try_recv());
    match picked {
        Some(Ok(path)) => {
            options.reference_pick = None;
            if let Some(path) = path {
                match load_reference_image(ctx, &path) {
                    Ok((tex, pixels)) => {
                        options.reference_image = Some(tex);
                        options.reference_pixels = Some(std::sync::Arc::new(pixels));
                        // Imagen nueva: vuelve al ajuste al cuadro.
                        options.reference_zoom = None;
                        options.reference_offset = egui::Vec2::ZERO;
                    }
                    Err(e) => eprintln!("No se pudo abrir la imagen de referencia: {e}"),
                }
            }
        }
        Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => options.reference_pick = None,
        _ => {}
    }
    if !options.reference_window {
        return false;
    }
    let screen = ctx.content_rect();
    // Primera vez: arriba a la derecha, debajo del gizmo de ejes.
    let mut pos = *options.reference_pos.get_or_insert_with(|| egui::pos2(screen.right() - REF_SIZE - 12.0, 110.0));
    let mut close = false;
    let mut pick_click = false;
    let tex = options.reference_image.clone();
    let mut zoom = options.reference_zoom.unwrap_or(1.0);
    let mut offset = options.reference_offset;
    let pixels = options.reference_pixels.clone();
    // Color que tomo el cuentagotas de la imagen en este cuadro (si tomo alguno).
    let mut picked: Option<[u8; 3]> = None;
    // Tamano actual (el inicial es cuadrado). Se cambia estirando los bordes o las esquinas.
    let mut size = options.reference_size.unwrap_or(vec2(REF_SIZE, REF_SIZE));
    let max_size = vec2((screen.width() - 20.0).max(REF_MIN_W), (screen.height() - 20.0).max(REF_MIN_H));
    #[allow(deprecated)]
    let id = egui::Id::new("reference_window");
    egui::Area::new(id).fixed_pos(pos).show(ctx, |ui| {
        Frame::new()
            .fill(Color32::WHITE)
            .corner_radius(CornerRadius::same(14))
            .shadow(egui::Shadow { offset: [0, 2], blur: 24, spread: 2, color: Color32::from_black_alpha(90) })
            .show(ui, |ui| {
                // El cuadro entero recibe el clic, asi pintar o girar la camara no empieza por debajo de la ventana.
                let cur = size;
                let (rect, body) = ui.allocate_exact_size(cur, Sense::click_and_drag());
                let p = ui.painter().clone();
                let ui_id = ui.scope_id();

                // Imagen de referencia: al inicio se ajusta al cuadro sin deformarse. Rueda = zoom hacia el cursor,
                // arrastrar = mover la imagen, doble clic = volver al ajuste. Queda debajo del asa y los botones.
                if let Some(t) = &tex {
                    let avail = rect.shrink(REF_IMG_PAD);
                    let img_size = t.size_vec2();
                    let fit = (avail.width() / img_size.x).min(avail.height() / img_size.y);
                    if body.double_clicked() && tool != Tool::Eyedropper {
                        zoom = 1.0;
                        offset = egui::Vec2::ZERO;
                    }
                    if body.dragged() && tool != Tool::Eyedropper {
                        offset += body.drag_delta();
                    }
                    let hover = ctx.input(|i| i.pointer.hover_pos());
                    let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                    if let Some(m) = hover.filter(|m| rect.contains(*m)) {
                        if scroll != 0.0 {
                            let new_zoom = (zoom * (scroll * REF_ZOOM_SPEED).exp()).clamp(REF_ZOOM_MIN, REF_ZOOM_MAX);
                            // El punto de la imagen bajo el cursor se queda donde esta.
                            let k = new_zoom / zoom;
                            let center = avail.center() + offset;
                            offset = (m - (m - center) * k) - avail.center();
                            zoom = new_zoom;
                        }
                    }
                    // Que siempre quede un pedazo de la imagen a la vista.
                    let shown = img_size * fit * zoom;
                    let max_off = ((shown + avail.size()) * 0.5 - vec2(20.0, 20.0)).max(egui::Vec2::ZERO);
                    offset = offset.clamp(-max_off, max_off);
                    let img_rect = Rect::from_center_size(avail.center() + offset, shown);
                    // Cuentagotas: mientras se mantiene apretado sobre la imagen toma el color del pixel bajo el cursor
                    // (los pixeles transparentes se mezclan con el fondo blanco del cuadro).
                    if tool == Tool::Eyedropper && body.is_pointer_button_down_on() {
                        if let (Some(px), Some(pos)) = (&pixels, body.interact_pointer_pos()) {
                            if avail.contains(pos) && img_rect.contains(pos) {
                                let u = (pos.x - img_rect.left()) / img_rect.width();
                                let v = (pos.y - img_rect.top()) / img_rect.height();
                                let x = ((u * px.size[0] as f32) as usize).min(px.size[0] - 1);
                                let y = ((v * px.size[1] as f32) as usize).min(px.size[1] - 1);
                                let i = (y * px.size[0] + x) * 4;
                                let a = px.rgba[i + 3] as f32 / 255.0;
                                if a > 0.0 {
                                    let mix = |c: u8| (c as f32 * a + 255.0 * (1.0 - a)).round() as u8;
                                    picked = Some([mix(px.rgba[i]), mix(px.rgba[i + 1]), mix(px.rgba[i + 2])]);
                                }
                            }
                        }
                    }
                    let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    p.with_clip_rect(avail).image(t.id(), img_rect, uv, Color32::WHITE);
                }

                // Estirar la ventana: 4 bordes y 4 esquinas. Se crean antes que el asa y los botones para que estos
                // tengan prioridad donde se solapan. dx/dy: -1 = lado izquierdo/superior, 1 = derecho/inferior.
                let e = REF_EDGE;
                let c = REF_CORNER;
                let zones: [(i8, i8, Rect, egui::CursorIcon); 8] = [
                    (-1, 0, Rect::from_min_max(egui::pos2(rect.left() - e, rect.top() + c), egui::pos2(rect.left() + e, rect.bottom() - c)), egui::CursorIcon::ResizeHorizontal),
                    (1, 0, Rect::from_min_max(egui::pos2(rect.right() - e, rect.top() + c), egui::pos2(rect.right() + e, rect.bottom() - c)), egui::CursorIcon::ResizeHorizontal),
                    (0, -1, Rect::from_min_max(egui::pos2(rect.left() + c, rect.top() - e), egui::pos2(rect.right() - c, rect.top() + e)), egui::CursorIcon::ResizeVertical),
                    (0, 1, Rect::from_min_max(egui::pos2(rect.left() + c, rect.bottom() - e), egui::pos2(rect.right() - c, rect.bottom() + e)), egui::CursorIcon::ResizeVertical),
                    (-1, -1, Rect::from_center_size(rect.left_top(), vec2(c * 2.0, c * 2.0)), egui::CursorIcon::ResizeNwSe),
                    (1, 1, Rect::from_center_size(rect.right_bottom(), vec2(c * 2.0, c * 2.0)), egui::CursorIcon::ResizeNwSe),
                    (1, -1, Rect::from_center_size(rect.right_top(), vec2(c * 2.0, c * 2.0)), egui::CursorIcon::ResizeNeSw),
                    (-1, 1, Rect::from_center_size(rect.left_bottom(), vec2(c * 2.0, c * 2.0)), egui::CursorIcon::ResizeNeSw),
                ];
                for (i, (dx, dy, zone, cursor)) in zones.into_iter().enumerate() {
                    let r = ui.interact(zone, ui_id.with(("refresize", i)), Sense::drag());
                    if r.hovered() || r.dragged() {
                        ctx.set_cursor_icon(cursor);
                    }
                    if r.dragged() {
                        let d = r.drag_delta();
                        let before = size;
                        size.x = (before.x + dx as f32 * d.x).clamp(REF_MIN_W, max_size.x);
                        size.y = (before.y + dy as f32 * d.y).clamp(REF_MIN_H, max_size.y);
                        // Al estirar desde la izquierda o arriba, la esquina opuesta debe quedarse quieta.
                        if dx < 0 {
                            pos.x += before.x - size.x;
                        }
                        if dy < 0 {
                            pos.y += before.y - size.y;
                        }
                    }
                }

                // Asa para mover la ventana: pastilla del mismo gris oscuro semitransparente que las barras y los botones.
                let handle_c = rect.center_top() + vec2(0.0, REF_HANDLE_CY);
                let handle = Rect::from_center_size(handle_c, vec2(72.0, 8.0));
                p.rect_filled(handle, CornerRadius::same(4), bar_fill());
                p.rect_stroke(handle, CornerRadius::same(4), Stroke::new(1.0, Color32::from_white_alpha(40)), StrokeKind::Inside);
                // Zona de agarre mas grande que la pastilla, para que sea facil de tomar.
                let grab = ui.interact(Rect::from_center_size(handle_c, vec2(96.0, 24.0)), ui_id.with("grab"), Sense::drag());
                if grab.dragged() {
                    pos += grab.drag_delta();
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                } else if grab.hovered() {
                    ctx.set_cursor_icon(egui::CursorIcon::Grab);
                }

                // Botones redondos: camara (izquierda) y X (derecha).
                for which in 0..2 {
                    let cx = if which == 0 {
                        rect.left() + REF_BTN_PAD + REF_BTN_R
                    } else {
                        rect.right() - REF_BTN_PAD - REF_BTN_R
                    };
                    let c = egui::pos2(cx, rect.top() + REF_BTN_CY);
                    let r = ui.interact(
                        Rect::from_center_size(c, vec2(REF_BTN_R * 2.0, REF_BTN_R * 2.0)),
                        ui_id.with(("refbtn", which)),
                        Sense::click(),
                    );
                    // Mismo gris oscuro semitransparente que las barras (mas opaco al pasar el cursor).
                    let fill = if r.hovered() {
                        Color32::from_rgba_unmultiplied(58, 58, 60, BAR_ALPHA.saturating_add(50))
                    } else {
                        bar_fill()
                    };
                    p.circle_filled(c, REF_BTN_R, fill);
                    if which == 1 {
                        let k = 8.0;
                        let stroke = Stroke::new(1.5, Color32::WHITE);
                        p.line_segment([c + vec2(-k, -k), c + vec2(k, k)], stroke);
                        p.line_segment([c + vec2(-k, k), c + vec2(k, -k)], stroke);
                        if r.clicked() {
                            close = true;
                        }
                        r.on_hover_text("Cerrar");
                    } else {
                        let icon = Rect::from_center_size(c, vec2(REF_ICON, REF_ICON));
                        egui::Image::new(egui::include_image!("../assets/icons/camera.svg"))
                            .tint(Color32::WHITE)
                            .paint_at(ui, icon);
                        if r.clicked() {
                            pick_click = true;
                        }
                        r.on_hover_text("Importar imagen de referencia");
                    }
                }
            });
    });
    // Clic en la camara: el selector de archivos se abre en otro hilo para no congelar la interfaz; la ruta elegida
    // llega por el canal y se carga al inicio de `reference_window` en el cuadro siguiente.
    if pick_click && options.reference_pick.is_none() {
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint_ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Elegir imagen de referencia")
                .add_filter("Imagen", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file();
            let _ = tx.send(path);
            repaint_ctx.request_repaint();
        });
        options.reference_pick = Some(rx);
    }
    // Que no se pueda sacar del todo de la pantalla.
    pos.x = pos.x.min(screen.right() - 40.0).max(screen.left() - size.x + 40.0);
    pos.y = pos.y.min(screen.bottom() - 40.0).max(screen.top());
    options.reference_pos = Some(pos);
    options.reference_size = Some(size);
    options.reference_zoom = Some(zoom);
    options.reference_offset = offset;
    if close {
        options.reference_window = false;
    }
    // Si el cuentagotas tomo un color de la imagen, pasa a ser el color del pincel.
    match picked {
        Some(c) if *color != c => {
            *color = c;
            true
        }
        _ => false,
    }
}

/// Panel flotante del boton de opciones: aparece justo encima del boton y se cierra al hacer clic fuera de el.
fn options_panel(ctx: &egui::Context, options: &mut OptionsPanel, button: Rect) {
    if !options.open {
        return;
    }
    #[allow(deprecated)]
    let id = egui::Id::new("options_panel");
    let out = egui::Area::new(id)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(button.center().x, button.top() - 10.0))
        .sense(Sense::click())
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 28.0;
                        ui.label(egui::RichText::new("Ventana de referencia").size(15.0).color(Color32::WHITE));
                        toggle_switch(ui, &mut options.reference_window);
                    });
                });
        });
    // Un clic fuera del panel (y fuera del boton, que ya lo abre/cierra) lo cierra.
    let panel = out.response.rect;
    let pressed_outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx.input(|i| i.pointer.interact_pos()).is_some_and(|p| !panel.contains(p) && !button.contains(p));
    if pressed_outside {
        options.open = false;
    }
}

/// Boton del cubo de la barra inferior (a la izquierda del de capas): muestra cuantas mallas (partes) tiene el modelo.
/// Por ahora no abre nada: el panel viene despues.
fn cube_button(ui: &mut egui::Ui, count: usize) {
    let (rect, response) = ui.allocate_exact_size(vec2(LAYERS_BTN_W, LAYERS_BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered {
            ui.painter().rect_filled(rect, CornerRadius::same(6), Color32::from_white_alpha(14));
        }
        let color = if hovered { Color32::WHITE } else { Color32::from_gray(205) };
        let icon_rect = Rect::from_center_size(rect.center(), vec2(LAYERS_ICON, LAYERS_ICON));
        egui::Image::new(egui::include_image!("../assets/icons/cube.svg")).tint(color).paint_at(ui, icon_rect);
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
}

/// Barra inferior: ocupa todo el ancho de la ventana, pegada abajo, con el mismo estilo que la barra de herramientas.
/// Su contenido va dentro del `with_layout` de abajo (a la derecha: capas y, a su izquierda, el cubo).
fn bottom_bar(ui: &mut egui::Ui, layer_count: usize, mesh_count: usize, options: &mut OptionsPanel) {
    let width = ui.ctx().content_rect().width();
    // Donde quedo el boton de opciones: el panel se abre justo encima de el.
    let mut options_rect = None;
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
                        layers_button(ui, layer_count);
                        cube_button(ui, mesh_count);
                        let r = options_button(ui, options.open);
                        if r.clicked() {
                            options.open = !options.open;
                        }
                        options_rect = Some(r.rect);
                    });
                });
        });
    if let Some(button) = options_rect {
        options_panel(ui.ctx(), options, button);
    }
}

/// La barra completa. Devuelve true si el usuario cambio de herramienta con el raton.
fn toolbar(ui: &mut egui::Ui, tool: &mut Tool, color: [u8; 3], panel: &mut ColorPanel, view_locked: &mut bool) -> bool {
    let mut changed = false;
    egui::Area::new(egui::Id::new("toolbar"))
        .anchor(Align2::LEFT_TOP, vec2(0.0, BAR_TOP))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                // Pegada al borde izquierdo: solo se redondean las esquinas de la derecha.
                .corner_radius(CornerRadius { nw: 0, ne: BAR_RADIUS, sw: 0, se: BAR_RADIUS })
                .inner_margin(Margin::symmetric(0, 6))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    for b in tool_buttons() {
                        if tool_button(ui, &b, *tool == b.tool) && *tool != b.tool {
                            *tool = b.tool;
                            changed = true;
                        }
                    }
                    // Candado: mientras esta encendido (azul) la vista no se puede girar, mover ni acercar.
                    let lock_tip = "Bloquear vista (no gira, no se mueve y no hace zoom)";
                    if icon_button(ui, egui::include_image!("../assets/icons/lock.svg"), "Bloquear", lock_tip, *view_locked) {
                        *view_locked = !*view_locked;
                    }
                    brush_button(ui, crate::BRUSH_RADIUS);
                    color_swatch(ui, color, panel);
                });
        });
    changed
}

/// Gizmo de navegacion (arriba a la derecha): lado de su area y distancia a los bordes superior y derecho.
const GIZMO_SIZE: f32 = 90.0;
const GIZMO_MARGIN: f32 = 10.0;
/// Del centro del gizmo al centro del circulo de cada eje.
const GIZMO_ARM: f32 = 29.0;
/// Radio del circulo de un eje positivo (lleva la letra) y de uno negativo (hueco).
const GIZMO_POS_R: f32 = 8.0;
const GIZMO_NEG_R: f32 = 5.5;

/// Colores de los ejes, como en Blender.
fn axis_color(axis: ViewAxis) -> Color32 {
    match axis {
        ViewAxis::PosX | ViewAxis::NegX => Color32::from_rgb(255, 56, 86),
        ViewAxis::PosY | ViewAxis::NegY => Color32::from_rgb(140, 220, 40),
        ViewAxis::PosZ | ViewAxis::NegZ => Color32::from_rgb(44, 143, 255),
    }
}

fn axis_name(axis: ViewAxis) -> &'static str {
    match axis {
        ViewAxis::PosX => "X",
        ViewAxis::NegX => "-X",
        ViewAxis::PosY => "Y",
        ViewAxis::NegY => "-Y",
        ViewAxis::PosZ => "Z",
        ViewAxis::NegZ => "-Z",
    }
}

/// Gizmo de ejes: muestra hacia donde apuntan X, Y y Z en la vista actual (circulo con letra = lado positivo,
/// circulo hueco = lado negativo). Devuelve el eje al que se hizo clic para que la camara gire a esa vista.
fn nav_gizmo(ctx: &egui::Context, camera: &Camera) -> Option<ViewAxis> {
    let mut picked = None;
    egui::Area::new(egui::Id::new("navgizmo"))
        .anchor(Align2::RIGHT_TOP, vec2(-GIZMO_MARGIN, GIZMO_MARGIN))
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(vec2(GIZMO_SIZE, GIZMO_SIZE), Sense::click());
            let center = rect.center();
            let pointer = response.hover_pos();
            let radius = |a: ViewAxis| if a.is_positive() { GIZMO_POS_R } else { GIZMO_NEG_R };

            // (eje, posicion en pantalla, profundidad). En espacio de vista +Z mira hacia quien ve, asi que mayor Z =
            // mas cerca; se ordena de lejos a cerca para que lo de delante tape lo de atras.
            let mut items: Vec<(ViewAxis, egui::Pos2, f32)> = ViewAxis::ALL
                .iter()
                .map(|&a| {
                    let v = camera.axis_in_view(a.dir());
                    (a, center + vec2(v.x, -v.y) * GIZMO_ARM, v.z)
                })
                .collect();
            items.sort_by(|a, b| a.2.total_cmp(&b.2));

            // Eje bajo el cursor: el de mas adelante si se solapan.
            let hovered = pointer.and_then(|p| {
                items.iter().rev().find(|(a, pos, _)| pos.distance(p) <= radius(*a) + 2.0).map(|(a, _, _)| *a)
            });

            let p = ui.painter();
            // Fondo suave solo mientras el cursor esta encima del gizmo.
            if pointer.is_some_and(|q| q.distance(center) <= GIZMO_SIZE * 0.5) {
                p.circle_filled(center, GIZMO_SIZE * 0.5 - 2.0, Color32::from_white_alpha(22));
            }
            for &(axis, pos, depth) in &items {
                // Los ejes que apuntan hacia atras se ven mas apagados.
                let fade = 0.55 + 0.45 * (depth * 0.5 + 0.5);
                let color = axis_color(axis).gamma_multiply(fade);
                let hot = hovered == Some(axis);
                if axis.is_positive() {
                    p.line_segment([center, pos], Stroke::new(2.5, color));
                    p.circle_filled(pos, GIZMO_POS_R, color);
                    if hot {
                        p.circle_stroke(pos, GIZMO_POS_R + 1.0, Stroke::new(1.5, Color32::WHITE));
                    }
                    p.text(pos, Align2::CENTER_CENTER, axis_name(axis), FontId::proportional(9.5), Color32::from_rgb(30, 30, 30));
                } else {
                    // Negativo: circulo hueco; al pasar el cursor crece un poco y muestra su nombre.
                    let r = if hot { GIZMO_NEG_R + 3.5 } else { GIZMO_NEG_R };
                    let fill = axis_color(axis).gamma_multiply(if hot { 0.55 } else { 0.28 } * fade);
                    p.circle_filled(pos, r, fill);
                    p.circle_stroke(pos, r, Stroke::new(1.5, if hot { Color32::WHITE } else { color }));
                    if hot {
                        p.text(pos, Align2::CENTER_CENTER, axis_name(axis), FontId::proportional(9.0), Color32::WHITE);
                    }
                }
            }
            if hovered.is_some() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if response.clicked() {
                picked = hovered;
            }
        });
    picked
}

/// Todo lo de egui junto: contexto, entrada de winit y dibujado en wgpu.
pub struct UiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    /// Panel de color flotante (se abre con el cuadrito de color de la barra).
    panel: ColorPanel,
    /// Panel del boton de opciones de la barra inferior.
    options: OptionsPanel,
}

impl UiLayer {
    /// `target_format` es el formato de la vista donde se dibuja encima del modelo. Debe ser NO sRGB: egui mezcla en
    /// espacio gamma y en un destino sRGB los bordes suavizados salen duros.
    pub fn new(window: &Window, device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(device, target_format, egui_wgpu::RendererOptions::default());
        UiLayer { ctx, state, renderer, panel: ColorPanel::default(), options: OptionsPanel::default() }
    }

    /// Pasa un evento de winit a egui. `consumed` = egui lo uso (cursor sobre la barra, etc.): la camara y el
    /// pincel no deben reaccionar. `repaint` = hay que redibujar.
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Arma la interfaz de este cuadro. Devuelve lo que hay que dibujar, si cambio la herramienta y el eje del
    /// gizmo al que se hizo clic (la camara debe girar a esa vista).
    pub fn run(
        &mut self,
        window: &Window,
        tool: &mut Tool,
        color: &mut [u8; 3],
        opacity: &mut f32,
        layer_count: usize,
        mesh_count: usize,
        camera: &Camera,
        view_locked: &mut bool,
    ) -> (egui::FullOutput, bool, Option<ViewAxis>) {
        let raw_input = self.state.take_egui_input(window);
        let mut changed = false;
        let mut view_axis = None;
        let panel = &mut self.panel;
        let options = &mut self.options;
        let mut full_output = self.ctx.run_ui(raw_input, |ui| {
            changed |= toolbar(ui, tool, *color, panel, view_locked);
            bottom_bar(ui, layer_count, mesh_count, options);
            changed |= reference_window(ui.ctx(), options, *tool, color);
            view_axis = nav_gizmo(ui.ctx(), camera);
            changed |= panel.show(ui.ctx(), color, opacity);
        });
        // Cursor, portapapeles, enlaces...
        self.state.handle_platform_output(window, std::mem::take(&mut full_output.platform_output));
        (full_output, changed, view_axis)
    }

    /// Dibuja la interfaz sobre `view` (que ya tiene el modelo). Devuelve los buffers de comandos extra que haya
    /// que enviar antes del `encoder` y cuanto falta para el proximo cuadro que pide egui (`ZERO` = ya mismo,
    /// `Duration::MAX` = no hace falta).
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size_in_pixels: [u32; 2],
        mut full_output: egui::FullOutput,
    ) -> (Vec<wgpu::CommandBuffer>, Duration) {
        let pixels_per_point = full_output.pixels_per_point;
        let repaint_delay = full_output
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map(|v| v.repaint_delay)
            .unwrap_or(Duration::MAX);

        // En egui 0.37 cada textura puede traer varias actualizaciones seguidas.
        for (id, deltas) in &full_output.textures_delta.set {
            for delta in deltas {
                self.renderer.update_texture(device, queue, *id, delta);
            }
        }
        let paint_jobs = self.ctx.tessellate(full_output.shapes, pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor { size_in_pixels, pixels_per_point };
        let extra = self.renderer.update_buffers(device, queue, encoder, &paint_jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("interfaz"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    // Load: se conserva el modelo ya dibujado y la interfaz va encima.
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            self.renderer.render(&mut pass, &paint_jobs, &screen);
        }
        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
        // Ya aplicadas las texturas: se vacia para que egui no se queje al soltar `full_output`.
        full_output.textures_delta.clear();
        (extra, repaint_delay)
    }
}
