//! Gizmo de navegacion de ejes (arriba a la derecha).

use super::*;

/// Gizmo de navegacion (arriba a la derecha): lado de su area y distancia a los bordes superior y derecho.
pub(super) const GIZMO_SIZE: f32 = 90.0;
pub(super) const GIZMO_MARGIN: f32 = 10.0;
/// Del centro del gizmo al centro del circulo de cada eje.
pub(super) const GIZMO_ARM: f32 = 29.0;
/// Radio del circulo de un eje positivo (lleva la letra) y de uno negativo (hueco).
pub(super) const GIZMO_POS_R: f32 = 8.0;
pub(super) const GIZMO_NEG_R: f32 = 5.5;

/// Colores de los ejes, como en Blender.
pub(super) fn axis_color(axis: ViewAxis) -> Color32 {
    match axis {
        ViewAxis::PosX | ViewAxis::NegX => Color32::from_rgb(255, 56, 86),
        ViewAxis::PosY | ViewAxis::NegY => Color32::from_rgb(140, 220, 40),
        ViewAxis::PosZ | ViewAxis::NegZ => Color32::from_rgb(44, 143, 255),
    }
}

pub(super) fn axis_name(axis: ViewAxis) -> &'static str {
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
pub(super) fn nav_gizmo(ctx: &egui::Context, camera: &Camera) -> Option<ViewAxis> {
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
