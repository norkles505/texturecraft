//! Ventana de referencia flotante: imagen importada con zoom, arrastre, redimensionado y cuentagotas.

use super::*;

/// Ventana de referencia: lado del cuadro blanco, radio y separacion de los botones redondos de arriba, y alto de su
/// centro y del asa respecto al borde superior de la ventana.
pub(super) const REF_SIZE: f32 = 240.0;
pub(super) const REF_BTN_R: f32 = 19.0;
pub(super) const REF_BTN_PAD: f32 = 4.0;
pub(super) const REF_BTN_CY: f32 = 52.0;
pub(super) const REF_HANDLE_CY: f32 = 15.0;
/// Lado del icono de la camara dentro de su circulo.
pub(super) const REF_ICON: f32 = 30.0;
/// Tamano minimo al estirar (que quepan el asa y los dos botones), mitad del grosor de la zona de agarre de los
/// bordes y mitad del lado de la zona de las esquinas.
pub(super) const REF_MIN_W: f32 = 140.0;
pub(super) const REF_MIN_H: f32 = 120.0;
pub(super) const REF_EDGE: f32 = 5.0;
pub(super) const REF_CORNER: f32 = 10.0;
/// Lado mas largo al que se reduce una imagen de referencia al cargarla, y margen entre la imagen y el borde del cuadro.
pub(super) const REF_IMG_MAX: u32 = 2048;
pub(super) const REF_IMG_PAD: f32 = 4.0;
/// Zoom de la imagen con la rueda: sensibilidad (por punto de desplazamiento), minimo y maximo (1 = ajustada al cuadro).
pub(super) const REF_ZOOM_SPEED: f32 = 0.002;
pub(super) const REF_ZOOM_MIN: f32 = 0.25;
pub(super) const REF_ZOOM_MAX: f32 = 16.0;

/// Pixeles (RGBA, sin premultiplicar) de la imagen de referencia ya reducida, para que el cuentagotas lea su color.
pub(super) struct RefPixels {
    size: [usize; 2],
    rgba: Vec<u8>,
}

/// Color de la imagen de referencia en el punto `pos` de pantalla, dado el rectangulo `img_rect` donde se dibuja la
/// imagen (respeta zoom y posicion). `None` si `pos` cae fuera de la imagen o el pixel es del todo transparente; los
/// semitransparentes se mezclan con el fondo blanco del cuadro.
pub(super) fn sample_reference(px: &RefPixels, img_rect: Rect, pos: egui::Pos2) -> Option<[u8; 3]> {
    if !img_rect.contains(pos) {
        return None;
    }
    let u = (pos.x - img_rect.left()) / img_rect.width();
    let v = (pos.y - img_rect.top()) / img_rect.height();
    let x = ((u * px.size[0] as f32) as usize).min(px.size[0] - 1);
    let y = ((v * px.size[1] as f32) as usize).min(px.size[1] - 1);
    let i = (y * px.size[0] + x) * 4;
    let a = px.rgba[i + 3] as f32 / 255.0;
    if a <= 0.0 {
        return None;
    }
    let mix = |c: u8| (c as f32 * a + 255.0 * (1.0 - a)).round() as u8;
    Some([mix(px.rgba[i]), mix(px.rgba[i + 1]), mix(px.rgba[i + 2])])
}

/// Lee una imagen del disco y la sube a egui como textura. Las muy grandes se reducen a `REF_IMG_MAX`.
pub(super) fn load_reference_image(ctx: &egui::Context, path: &std::path::Path) -> Result<(egui::TextureHandle, RefPixels), String> {
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
pub(super) fn reference_window(ctx: &egui::Context, options: &mut OptionsPanel, tool: Tool, color: &mut [u8; 3]) -> bool {
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
    // Color de la imagen bajo el cursor (para rellenar el aro de la chincheta), aunque no se haya hecho clic.
    let mut under: Option<[u8; 3]> = None;
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
                    if tool == Tool::Eyedropper {
                        if body.is_pointer_button_down_on() {
                            if let (Some(px), Some(pos)) = (&pixels, body.interact_pointer_pos()) {
                                if avail.contains(pos) {
                                    picked = sample_reference(px, img_rect, pos);
                                }
                            }
                        }
                        if let (Some(px), Some(m)) = (&pixels, hover) {
                            if body.hovered() && avail.contains(m) {
                                under = sample_reference(px, img_rect, m);
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

                // Cuentagotas sobre la imagen de referencia: la misma chincheta que sobre el modelo. Si el cursor esta
                // sobre un borde, el asa o un boton, esos ponen su propio cursor (y el cuadro ya no cuenta como "hovered").
                if tool == Tool::Eyedropper && tex.is_some() && body.hovered() {
                    if let Some(m) = ctx.input(|i| i.pointer.hover_pos()) {
                        eyedropper_pin(ctx, m, under);
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
                        egui::Image::new(egui::include_image!("../../assets/icons/camera.svg"))
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
