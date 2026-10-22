//! Barra de herramientas vertical (borde izquierdo): botones, candado de la vista, boton del pincel y cuadrito de color.

use super::*;

/// Distancia de la barra al borde superior de la ventana (por la izquierda va pegada al borde).
pub(super) const BAR_TOP: f32 = 12.0;
/// Radio de las esquinas del lado derecho de la barra.
pub(super) const BAR_RADIUS: u8 = 10;
/// Ancho y alto de cada boton, lado del icono y tamano del texto (en puntos de egui). El ancho de la barra es el del
/// boton: si un nombre no cabe, se achica `LABEL_SIZE` o se sube `BTN_W`.
pub(super) const BTN_W: f32 = 54.0;
pub(super) const BTN_H: f32 = 46.0;
pub(super) const ICON_SIZE: f32 = 22.0;
pub(super) const LABEL_SIZE: f32 = 9.0;
/// Tamano del cuadrito de color al pie de la barra.
pub(super) const SWATCH_W: f32 = 30.0;
pub(super) const SWATCH_H: f32 = 30.0;
/// Radio del circulo del boton del panel de pinceles.
pub(super) const BRUSH_BTN_R: f32 = 14.0;

/// Un boton de la barra: herramienta, nombre visible, icono y atajo de teclado (para el tooltip).
pub(super) struct ToolButton {
    tool: Tool,
    label: &'static str,
    key: &'static str,
    icon: egui::ImageSource<'static>,
}

/// Botones de la barra, de arriba abajo. Para agregar uno: un `Tool` nuevo, su SVG y una linea aqui.
pub(super) fn tool_buttons() -> [ToolButton; 7] {
    [
        ToolButton {
            tool: Tool::Brush,
            label: "Pincel",
            key: "P",
            icon: egui::include_image!("../../assets/icons/brush.svg"),
        },
        ToolButton {
            tool: Tool::Eraser,
            label: "Borrador",
            key: "G",
            icon: egui::include_image!("../../assets/icons/eraser.svg"),
        },
        ToolButton {
            tool: Tool::Fill,
            label: "Relleno",
            key: "F",
            icon: egui::include_image!("../../assets/icons/bucket.svg"),
        },
        ToolButton {
            tool: Tool::Eyedropper,
            label: "Cuentagotas",
            key: "I",
            icon: egui::include_image!("../../assets/icons/eyedropper.svg"),
        },
        ToolButton {
            tool: Tool::Smudge,
            label: "Difuminar",
            key: "D",
            icon: egui::include_image!("../../assets/icons/smudge.svg"),
        },
        ToolButton {
            tool: Tool::Blur,
            label: "Borrosidad",
            key: "U",
            icon: egui::include_image!("../../assets/icons/blur.svg"),
        },
        ToolButton {
            tool: Tool::Hand,
            label: "Mano",
            key: "H",
            icon: egui::include_image!("../../assets/icons/hand.svg"),
        },
    ]
}

/// Dibuja un boton (icono arriba, nombre abajo) y devuelve true si se hizo clic.
pub(super) fn tool_button(ui: &mut egui::Ui, b: &ToolButton, active: bool) -> bool {
    icon_button(
        ui,
        b.icon.clone(),
        b.label,
        &format!("{} ({})", b.label, b.key),
        active,
    )
}

/// Boton generico de la barra (icono arriba, nombre abajo, tooltip). Lo usan las herramientas y el candado de la vista.
pub(super) fn icon_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    label: &str,
    tooltip: &str,
    active: bool,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(BTN_W, BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        if hovered && !active {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(8), Color32::from_white_alpha(14));
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

/// Boton del panel de pinceles: circulo negro con aro claro (azul con el panel abierto) y, encima, el tamano del
/// pincel con contorno. Devuelve true si se hizo clic (abre o cierra el panel).
pub(super) fn brush_button(ui: &mut egui::Ui, size: f32, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(BTN_W, BTN_H), Sense::click());
    if ui.is_rect_visible(rect) {
        let ring = if active {
            ACCENT
        } else if response.hovered() {
            Color32::WHITE
        } else {
            Color32::from_gray(205)
        };
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
            p.text(
                pos + off,
                Align2::CENTER_CENTER,
                &label,
                font.clone(),
                Color32::WHITE,
            );
        }
        p.text(pos, Align2::CENTER_CENTER, &label, font, Color32::BLACK);
    }
    response
        .on_hover_text("Pinceles (clic para abrir el panel)")
        .clicked()
}

/// Cuadrito de color al pie de la barra: muestra el color del pincel y, al hacer clic, abre o cierra el panel de color.
pub(super) fn color_swatch(ui: &mut egui::Ui, color: [u8; 3], panel: &mut ColorPanel) {
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
    ui.painter().rect_filled(
        swatch,
        CornerRadius::same(4),
        Color32::from_rgb(color[0], color[1], color[2]),
    );
    ui.painter().rect_stroke(
        swatch,
        CornerRadius::same(4),
        Stroke::new(2.0, ring),
        StrokeKind::Inside,
    );
    if response
        .on_hover_text("Color (clic para abrir el panel)")
        .clicked()
    {
        panel.open = !panel.open;
    }
}

/// La barra completa. Devuelve true si el usuario cambio de herramienta con el raton.
pub(super) fn toolbar(
    ui: &mut egui::Ui,
    tool: &mut Tool,
    color: [u8; 3],
    panel: &mut ColorPanel,
    view_locked: &mut bool,
    brush: &Brush,
    brush_panel: &mut BrushPanel,
) -> bool {
    let mut changed = false;
    egui::Area::new(egui::Id::new("toolbar"))
        .anchor(Align2::LEFT_TOP, vec2(0.0, BAR_TOP))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(bar_fill())
                .stroke(Stroke::new(1.0, Color32::from_white_alpha(16)))
                // Pegada al borde izquierdo: solo se redondean las esquinas de la derecha.
                .corner_radius(CornerRadius {
                    nw: 0,
                    ne: BAR_RADIUS,
                    sw: 0,
                    se: BAR_RADIUS,
                })
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
                    if icon_button(
                        ui,
                        egui::include_image!("../../assets/icons/lock.svg"),
                        "Bloquear",
                        lock_tip,
                        *view_locked,
                    ) {
                        *view_locked = !*view_locked;
                    }
                    if brush_button(ui, brush.radius, brush_panel.open) {
                        brush_panel.open = !brush_panel.open;
                    }
                    color_swatch(ui, color, panel);
                });
        });
    changed
}
