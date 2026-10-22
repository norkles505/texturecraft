//! Panel de mallas (el que abre el boton del cubo de la barra inferior), con el mismo estilo que el de capas: titulo
//! "Malla", barra negra con Importar y Eliminar, lista de las mallas del modelo (partes `o`/`g` del OBJ) pegada abajo,
//! fila "Fondo" y barra de opacidad.
//!
//! Cada fila lleva la miniatura, el nombre, un ojo para mostrarla u ocultarla, el porcentaje, la cantidad de
//! triangulos y las tres rayas del extremo. Una malla oculta no se dibuja ni se puede pintar (ESPECIFICACION.md,
//! seccion 3.1). Tocar una fila la elige como pieza activa (seccion 3.3): el panel de capas muestra las capas de esa
//! pieza y la fila "Fondo" es el fondo de su textura. La elegida se ve con fondo azul claro.
//!
//! Todavia sin funcion: Eliminar, el porcentaje de cada fila, las tres rayas (reordenar) y el efecto de la barra de
//! opacidad (el deslizador ya se mueve y muestra su valor, pero no cambia el dibujo del modelo).

use super::*;

/// Medidas del panel (en puntos de egui): ancho (mas angosto que el de capas, que lleva tiras de botones a los lados),
/// margen del marco, alto del titulo, de la barra negra y de la lista (con su fila "Fondo" abajo).
const MP_W: f32 = 252.0;
const MP_PAD: f32 = 6.0;
const MP_TITLE_H: f32 = 34.0;
const MP_BAR_H: f32 = 38.0;
const MP_LIST_H: f32 = 292.0;

/// Recorta `text` para que mida como mucho `max_w` puntos con la letra `font`, agregando "…" al final si hubo que
/// cortar. Mide el ancho real del texto (no cuenta caracteres), asi aprovecha todo el espacio libre.
fn fit_text(p: &egui::Painter, text: &str, font: FontId, max_w: f32) -> String {
    let width = |s: &str| {
        p.layout_no_wrap(s.to_string(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    if width(text) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for n in (1..chars.len()).rev() {
        let cut: String = chars[..n].iter().collect::<String>() + "…";
        if width(&cut) <= max_w {
            return cut;
        }
    }
    "…".to_string()
}

/// Si el selector de archivos del boton Importar ya devolvio algo, deja la ruta en `pending_import` (la recoge
/// `UiLayer::take_import`). Se llama en cada cuadro, aunque el panel este cerrado: el selector puede seguir abierto.
pub(super) fn poll_mesh_import(ctx: &egui::Context, options: &mut OptionsPanel) {
    let Some(rx) = options.mesh_import.as_ref() else {
        return;
    };
    match rx.try_recv() {
        Ok(path) => {
            options.mesh_import = None;
            if path.is_some() {
                options.pending_import = path;
                // Que el cuadro siguiente ya cargue el modelo.
                ctx.request_repaint();
            }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => options.mesh_import = None,
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }
}

/// Abre el selector de archivos en otro hilo (para no congelar la interfaz); la ruta llega por el canal.
fn start_mesh_import(ctx: &egui::Context, options: &mut OptionsPanel) {
    if options.mesh_import.is_some() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let repaint_ctx = ctx.clone();
    std::thread::spawn(move || {
        let path = rfd::FileDialog::new()
            .set_title("Importar modelo")
            .add_filter("Modelo OBJ", &["obj"])
            .pick_file();
        let _ = tx.send(path);
        repaint_ctx.request_repaint();
    });
    options.mesh_import = Some(rx);
}

/// Panel flotante del boton del cubo: aparece justo encima del boton y se cierra al hacer clic fuera de el. Cambia la
/// visibilidad de `parts`, la malla elegida `selected` (posicion en `parts`) y el fondo de `stack` (las capas de la
/// pieza activa). Devuelve `(cambio la visibilidad de alguna malla, cambio el fondo)`.
pub(super) fn meshes_panel(
    ctx: &egui::Context,
    options: &mut OptionsPanel,
    parts: &mut [MeshPart],
    selected: &mut usize,
    stack: &mut LayerStack,
    button: Rect,
) -> (bool, bool) {
    if !options.meshes_open {
        return (false, false);
    }
    let mut changed = false;
    let mut bg_changed = false;
    let mut import_click = false;
    let n = parts.len();
    #[allow(deprecated)]
    let id = egui::Id::new("meshes_panel");
    let out = egui::Area::new(id)
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(button.center().x, button.top() - 10.0))
        .sense(Sense::click())
        .show(ctx, |ui| {
            Frame::new()
                .fill(bar_fill())
                .corner_radius(CornerRadius::same(14))
                .inner_margin(Margin::same(MP_PAD as i8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    let ui_id = ui.scope_id();
                    let p = ui.painter().clone();

                    // Titulo.
                    let (title, _) = ui.allocate_exact_size(vec2(MP_W, MP_TITLE_H), Sense::hover());
                    p.text(
                        egui::pos2(title.left() + 6.0, title.center().y),
                        Align2::LEFT_CENTER,
                        "Malla",
                        FontId::proportional(15.0),
                        Color32::WHITE,
                    );

                    // Barra negra: Importar (funciona) a la izquierda y Eliminar (todavia sin funcion) a la derecha.
                    let (bar, _) = ui.allocate_exact_size(vec2(MP_W, MP_BAR_H), Sense::hover());
                    p.rect_filled(bar, CornerRadius::same(8), Color32::BLACK);
                    let imp = Rect::from_min_max(bar.min, egui::pos2(bar.center().x, bar.bottom()));
                    let del = Rect::from_min_max(egui::pos2(bar.center().x, bar.top()), bar.max);
                    let waiting = options.mesh_import.is_some();
                    let r = ui.interact(imp, ui_id.with("mimport"), Sense::click());
                    let imp_color = if waiting {
                        Color32::from_gray(150)
                    } else if r.hovered() {
                        ACCENT
                    } else {
                        Color32::WHITE
                    };
                    let imp_label = if waiting {
                        "Elige un archivo…"
                    } else {
                        "Importar"
                    };
                    p.text(
                        imp.center(),
                        Align2::CENTER_CENTER,
                        imp_label,
                        FontId::proportional(16.0),
                        imp_color,
                    );
                    if r.on_hover_text("Importar un modelo (.obj): reemplaza el modelo actual")
                        .clicked()
                    {
                        import_click = true;
                    }
                    let r = ui.interact(del, ui_id.with("mdelete"), Sense::hover());
                    p.text(
                        del.center(),
                        Align2::CENTER_CENTER,
                        "Eliminar",
                        FontId::proportional(16.0),
                        Color32::WHITE,
                    );
                    r.on_hover_text("Eliminar malla (todavia sin funcion)");

                    // Lista: fondo gris oscuro, con la fila "Fondo" abajo. Si las mallas caben, se pegan abajo (justo
                    // sobre "Fondo") y arriba queda el gris vacio; si no, se desplazan con la rueda.
                    ui.add_space(3.0);
                    let (list, _) = ui.allocate_exact_size(vec2(MP_W, MP_LIST_H), Sense::hover());
                    p.rect_filled(list, CornerRadius::same(8), LP_LIST_EMPTY);
                    let bg_row = Rect::from_min_max(
                        egui::pos2(list.left(), list.bottom() - LP_BG_H),
                        list.max,
                    );
                    let rows_h = n as f32 * LP_ROW_H;
                    let avail = bg_row.top() - list.top();
                    let fits = rows_h <= avail;
                    let area = if fits {
                        Rect::from_min_max(
                            egui::pos2(list.left(), bg_row.top() - rows_h),
                            egui::pos2(list.right(), bg_row.top()),
                        )
                    } else {
                        Rect::from_min_max(list.min, egui::pos2(list.right(), bg_row.top()))
                    };
                    let top_round = if fits { 0 } else { 8 };
                    p.rect_filled(
                        area,
                        CornerRadius {
                            nw: top_round,
                            ne: top_round,
                            sw: 0,
                            se: 0,
                        },
                        MENU_FILL,
                    );

                    let max_scroll = if fits { 0.0 } else { rows_h - area.height() };
                    if ctx
                        .input(|i| i.pointer.hover_pos())
                        .is_some_and(|m| area.contains(m))
                    {
                        options.meshes_scroll -= ctx.input(|i| i.smooth_scroll_delta.y);
                    }
                    options.meshes_scroll = options.meshes_scroll.clamp(0.0, max_scroll);
                    let scroll = options.meshes_scroll;
                    let pa = p.with_clip_rect(area);
                    // Que lo que se dibuje con `ui` (los iconos y los clics) tambien se recorte a la lista.
                    let old_clip = ui.clip_rect();
                    ui.set_clip_rect(area.intersect(old_clip));
                    for (i, part) in parts.iter_mut().enumerate() {
                        let row = Rect::from_min_size(
                            egui::pos2(area.left(), area.top() - scroll + i as f32 * LP_ROW_H),
                            vec2(area.width(), LP_ROW_H),
                        );
                        if row.bottom() < area.top() || row.top() > area.bottom() {
                            continue;
                        }
                        let rr = ui.interact(
                            row.intersect(area),
                            ui_id.with(("mrow", i)),
                            Sense::click(),
                        );
                        if rr.clicked() {
                            *selected = i;
                        }
                        let th = Rect::from_min_size(
                            egui::pos2(row.left() + 6.0, row.center().y - LP_THUMB_H * 0.5),
                            vec2(LP_THUMB_W, LP_THUMB_H),
                        );
                        // Ojo: se crea despues de la fila para tener prioridad donde se solapan.
                        let eye_c = egui::pos2(th.right() + 26.0, row.center().y);
                        let eye = ui.interact(
                            Rect::from_center_size(eye_c, vec2(26.0, 26.0)).intersect(area),
                            ui_id.with(("meye", i)),
                            Sense::click(),
                        );
                        let was_visible = part.visible;
                        if eye
                            .on_hover_text(if was_visible {
                                "Ocultar malla"
                            } else {
                                "Mostrar malla"
                            })
                            .clicked()
                        {
                            part.visible = !part.visible;
                            changed = true;
                        }
                        let is_selected = i == *selected;
                        if is_selected {
                            pa.rect_filled(row, CornerRadius::same(0), LP_SELECTED);
                        } else if rr.hovered() {
                            pa.rect_filled(
                                row,
                                CornerRadius::same(0),
                                Color32::from_black_alpha(10),
                            );
                        }
                        paint_checker(
                            &pa,
                            th,
                            6.0,
                            Color32::from_gray(250),
                            Color32::from_gray(222),
                        );
                        let border = if is_selected {
                            Stroke::new(2.0, ACCENT)
                        } else {
                            Stroke::new(1.0, Color32::from_gray(170))
                        };
                        pa.rect_stroke(th, CornerRadius::same(3), border, StrokeKind::Inside);
                        // Una malla oculta se ve apagada. El nombre esta en la fila de arriba, donde a su derecha no hay
                        // nada (el porcentaje y las rayas van mas abajo), asi que usa todo el ancho libre; solo se
                        // corta con puntos suspensivos si de verdad no cabe.
                        let text_color = if part.visible {
                            MENU_TEXT
                        } else {
                            Color32::from_gray(150)
                        };
                        let name_x = th.right() + 8.0;
                        let name = fit_text(
                            &pa,
                            &part.name,
                            FontId::proportional(16.0),
                            row.right() - 8.0 - name_x,
                        );
                        pa.text(
                            egui::pos2(name_x, row.top() + 13.0),
                            Align2::LEFT_CENTER,
                            name,
                            FontId::proportional(16.0),
                            text_color,
                        );
                        let eye_color = if part.visible {
                            MENU_TEXT
                        } else {
                            Color32::from_gray(175)
                        };
                        egui::Image::new(egui::include_image!(
                            "../../assets/icons/layer_visible.svg"
                        ))
                        .tint(eye_color)
                        .paint_at(ui, Rect::from_center_size(eye_c, vec2(24.0, 24.0)));
                        // Porcentaje (todavia fijo en 100 %) a media altura y, abajo, los triangulos de la malla.
                        let col_x = row.right() - 62.0;
                        pa.text(
                            egui::pos2(col_x, row.center().y),
                            Align2::CENTER_CENTER,
                            "100%",
                            FontId::proportional(16.0),
                            text_color,
                        );
                        pa.text(
                            egui::pos2(col_x, row.bottom() - 13.0),
                            Align2::CENTER_CENTER,
                            format!("{} tri", part.vertex_count / 3),
                            FontId::proportional(14.0),
                            text_color,
                        );
                        // Tres rayas del extremo derecho (solo adorno por ahora: todavia no se reordena).
                        egui::Image::new(egui::include_image!("../../assets/icons/layer_drag.svg"))
                            .tint(Color32::from_gray(150))
                            .paint_at(
                                ui,
                                Rect::from_center_size(
                                    egui::pos2(row.right() - 20.0, row.center().y),
                                    vec2(16.0, 16.0),
                                ),
                            );
                        pa.hline(
                            row.x_range(),
                            row.bottom() - 0.5,
                            Stroke::new(1.0, MENU_LINE),
                        );
                    }
                    ui.set_clip_rect(old_clip);
                    // Barrita de desplazamiento, solo si la lista no cabe.
                    if max_scroll > 0.0 {
                        let track_h = area.height() - 8.0;
                        let bar_h = (track_h * area.height() / rows_h).max(16.0);
                        let y = area.top() + 4.0 + (track_h - bar_h) * (scroll / max_scroll);
                        let bar = Rect::from_min_size(
                            egui::pos2(area.right() - 4.0, y),
                            vec2(3.0, bar_h),
                        );
                        p.rect_filled(bar, CornerRadius::same(2), Color32::from_black_alpha(70));
                    }

                    // Fila "Fondo": blanco, ajedrez claro, ajedrez oscuro y sin fondo. Es el de la pieza activa.
                    p.rect_filled(
                        bg_row,
                        CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: 8,
                            se: 8,
                        },
                        MENU_FILL,
                    );
                    p.hline(bg_row.x_range(), bg_row.top(), Stroke::new(1.0, MENU_LINE));
                    p.text(
                        egui::pos2(bg_row.left() + 8.0, bg_row.center().y),
                        Align2::LEFT_CENTER,
                        "Fondo",
                        FontId::proportional(16.0),
                        MENU_TEXT,
                    );
                    let kinds = [
                        Background::White,
                        Background::Checker,
                        Background::CheckerDark,
                        Background::None,
                    ];
                    let s = 21.0;
                    for (i, kind) in kinds.iter().copied().enumerate() {
                        let x = bg_row.right() - 6.0 - s - (3 - i) as f32 * (s + 4.0);
                        let sw = Rect::from_min_size(
                            egui::pos2(x, bg_row.center().y - s * 0.5),
                            vec2(s, s),
                        );
                        let r = ui.interact(sw, ui_id.with(("mbg", i)), Sense::click());
                        if r.clicked() && stack.background != kind {
                            stack.background = kind;
                            bg_changed = true;
                        }
                        match kind {
                            Background::White => {
                                p.rect_filled(sw, CornerRadius::same(2), Color32::WHITE);
                            }
                            Background::Checker => {
                                paint_checker(&p, sw, 5.0, Color32::WHITE, Color32::from_gray(222));
                            }
                            Background::CheckerDark => {
                                paint_checker(
                                    &p,
                                    sw,
                                    5.0,
                                    Color32::from_gray(90),
                                    Color32::from_gray(55),
                                );
                            }
                            Background::None => {
                                p.rect_filled(sw, CornerRadius::same(2), Color32::WHITE);
                                p.line_segment(
                                    [sw.left_bottom(), sw.right_top()],
                                    Stroke::new(1.0, Color32::from_gray(60)),
                                );
                            }
                        }
                        let border = if stack.background == kind {
                            Stroke::new(2.0, ACCENT)
                        } else {
                            Stroke::new(1.0, Color32::from_gray(150))
                        };
                        p.rect_stroke(sw, CornerRadius::same(2), border, StrokeKind::Inside);
                    }

                    // Opacidad de la malla: el mismo deslizador del panel de capas (`slider_row`): etiqueta, boton -,
                    // barra en pastilla con tablero y degradado, boton + y perilla blanca. Todavia solo se mueve y se
                    // muestra: no cambia el dibujo del modelo.
                    ui.add_space(4.0);
                    let (sl, _) = ui.allocate_exact_size(vec2(MP_W, LP_SLIDER_H), Sense::hover());
                    let mut frac = options.meshes_opacity.unwrap_or(1.0);
                    let label = format!("{}%", (frac * 100.0).round() as i32);
                    let e = slider_row(
                        ui,
                        sl.shrink2(vec2(6.0, 0.0)),
                        ui_id.with("mopacity"),
                        &label,
                        &mut frac,
                        0.01,
                        |p, r| {
                            pill_checker(p, r, 3.5);
                            pill_gradient(p, r, &[Color32::TRANSPARENT, Color32::from_gray(25)]);
                        },
                    );
                    if e.changed {
                        options.meshes_opacity = Some(frac);
                    }
                });
        });
    if import_click {
        start_mesh_import(ctx, options);
    }
    // Un clic fuera del panel (y fuera del boton, que ya lo abre/cierra) lo cierra.
    let panel = out.response.rect;
    let pressed_outside = ctx.input(|i| i.pointer.primary_pressed())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !panel.contains(p) && !button.contains(p));
    if pressed_outside {
        options.meshes_open = false;
    }
    (changed, bg_changed)
}
