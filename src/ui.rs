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
use crate::color_panel::ColorPanel;

/// Azul del boton activo (como el "Brush" de la referencia).
const ACCENT: Color32 = Color32::from_rgb(33, 150, 243);
/// Fondo de la barra.
const BAR_FILL: Color32 = Color32::from_rgb(58, 58, 60);
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
fn tool_buttons() -> [ToolButton; 4] {
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
    ]
}

/// Dibuja un boton (icono arriba, nombre abajo) y devuelve true si se hizo clic.
fn tool_button(ui: &mut egui::Ui, b: &ToolButton, active: bool) -> bool {
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
        egui::Image::new(b.icon.clone()).tint(color).paint_at(ui, icon_rect);
        ui.painter().text(
            egui::pos2(rect.center().x, rect.bottom() - 4.0),
            Align2::CENTER_BOTTOM,
            b.label,
            FontId::proportional(LABEL_SIZE),
            color,
        );
    }
    response.on_hover_text(format!("{} ({})", b.label, b.key)).clicked()
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

/// La barra completa. Devuelve true si el usuario cambio de herramienta con el raton.
fn toolbar(ui: &mut egui::Ui, tool: &mut Tool, color: [u8; 3], panel: &mut ColorPanel) -> bool {
    let mut changed = false;
    egui::Area::new(egui::Id::new("toolbar"))
        .anchor(Align2::LEFT_TOP, vec2(0.0, BAR_TOP))
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(BAR_FILL)
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
                    brush_button(ui, crate::BRUSH_RADIUS);
                    color_swatch(ui, color, panel);
                });
        });
    changed
}

/// Todo lo de egui junto: contexto, entrada de winit y dibujado en wgpu.
pub struct UiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    /// Panel de color flotante (se abre con el cuadrito de color de la barra).
    panel: ColorPanel,
}

impl UiLayer {
    /// `target_format` es el formato de la superficie donde se dibuja encima del modelo.
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
        UiLayer { ctx, state, renderer, panel: ColorPanel::default() }
    }

    /// Pasa un evento de winit a egui. `consumed` = egui lo uso (cursor sobre la barra, etc.): la camara y el
    /// pincel no deben reaccionar. `repaint` = hay que redibujar.
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Arma la interfaz de este cuadro. Devuelve lo que hay que dibujar y si cambio la herramienta.
    pub fn run(
        &mut self,
        window: &Window,
        tool: &mut Tool,
        color: &mut [u8; 3],
        opacity: &mut f32,
    ) -> (egui::FullOutput, bool) {
        let raw_input = self.state.take_egui_input(window);
        let mut changed = false;
        let panel = &mut self.panel;
        let mut full_output = self.ctx.run_ui(raw_input, |ui| {
            changed |= toolbar(ui, tool, *color, panel);
            changed |= panel.show(ui.ctx(), color, opacity);
        });
        // Cursor, portapapeles, enlaces...
        self.state.handle_platform_output(window, std::mem::take(&mut full_output.platform_output));
        (full_output, changed)
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
