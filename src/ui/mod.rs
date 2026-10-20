//! Interfaz (egui): barra de herramientas vertical pegada al borde izquierdo, dibujada encima del modelo.
//!
//! Los iconos son archivos SVG en `assets/icons/` (se cargan con `include_image!`, es decir, van dentro del
//! ejecutable). Para cambiar un icono basta con reemplazar su archivo. Conviene que sean de un solo color
//! (blanco) y fondo transparente: la barra los tiñe de gris, blanco o azul segun el estado del boton.
//!
//! El codigo esta repartido en un archivo por componente (ver los `mod` de abajo).

mod barra_herramientas;
mod barra_inferior;
mod cursor;
mod estilo;
mod gizmo;
mod panel_capas;
mod panel_color;
mod panel_menu;
mod panel_opciones;
mod panel_pinceles;
mod ventana_referencia;

use std::time::Duration;

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Rect, Sense, Stroke, StrokeKind, vec2};
use winit::event::WindowEvent;
use winit::window::Window;

use crate::Tool;
use crate::blend::BlendMode;
use crate::brush::Brush;
use crate::camera::{Camera, ViewAxis};
use panel_color::{ColorPanel, pill_checker, pill_gradient, slider_row, slider_row_with};
use panel_pinceles::BrushPanel;
use crate::layers::{Background, LayerStack};

use barra_herramientas::*;
use barra_inferior::*;
use cursor::*;
use estilo::*;
use gizmo::*;
use panel_capas::*;
use panel_menu::*;
use panel_opciones::*;
use ventana_referencia::*;
pub use cursor::BrushRing;

/// Estado del panel de opciones (el que abre el boton de los interruptores de la barra inferior).
#[derive(Default)]
struct OptionsPanel {
    open: bool,
    /// Panel del menu (el que abre el boton de la flecha de la barra inferior).
    menu_open: bool,
    /// Panel de capas (el que abre el boton de capas de la barra inferior), cuanto se desplazo su lista (en puntos)
    /// y si esta abierto el desplegable de modos de mezcla.
    layers_open: bool,
    layers_scroll: f32,
    /// Menu de los tres puntos del panel de capas.
    more_open: bool,
    blend_open: bool,
    /// Cuanto se desplazo la lista del desplegable de modos de mezcla (en puntos).
    blend_scroll: f32,
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

/// Todo lo de egui junto: contexto, entrada de winit y dibujado en wgpu.
pub struct UiLayer {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    /// Panel de color flotante (se abre con el cuadrito de color de la barra).
    panel: ColorPanel,
    /// Panel del boton de opciones de la barra inferior.
    options: OptionsPanel,
    /// Panel de pinceles flotante (se abre con el boton del pincel de la barra; sale en el mismo lugar que el de color).
    brush_panel: BrushPanel,
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
        UiLayer {
            ctx,
            state,
            renderer,
            panel: ColorPanel::default(),
            options: OptionsPanel::default(),
            brush_panel: BrushPanel::default(),
        }
    }

    /// Pasa un evento de winit a egui. `consumed` = egui lo uso (cursor sobre la barra, etc.): la camara y el
    /// pincel no deben reaccionar. `repaint` = hay que redibujar.
    pub fn on_window_event(&mut self, window: &Window, event: &WindowEvent) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Arma la interfaz de este cuadro. Devuelve lo que hay que dibujar, si cambio la herramienta, el eje del
    /// gizmo al que se hizo clic (la camara debe girar a esa vista) y si el panel de capas cambio algo que obliga a
    /// recomponer la textura (ojo, opacidad, modo de mezcla, recorte o fondo).
    pub fn run(
        &mut self,
        window: &Window,
        tool: &mut Tool,
        color: &mut [u8; 3],
        opacity: &mut f32,
        brush: &mut Brush,
        layers: &mut LayerStack,
        mesh_count: usize,
        camera: &Camera,
        view_locked: &mut bool,
        ring: Option<BrushRing>,
    ) -> (egui::FullOutput, bool, Option<ViewAxis>, bool) {
        let raw_input = self.state.take_egui_input(window);
        let mut changed = false;
        let mut layers_dirty = false;
        let mut view_axis = None;
        let panel = &mut self.panel;
        let options = &mut self.options;
        let brush_panel = &mut self.brush_panel;
        let mut full_output = self.ctx.run_ui(raw_input, |ui| {
            let (color_was_open, brush_was_open) = (panel.open, brush_panel.open);
            changed |= toolbar(ui, tool, *color, panel, view_locked, brush, brush_panel);
            // El panel de color y el de pinceles salen en el mismo lugar: abrir uno cierra el otro.
            if brush_panel.open && !brush_was_open {
                panel.open = false;
            } else if panel.open && !color_was_open {
                brush_panel.open = false;
            }
            layers_dirty |= bottom_bar(ui, layers, mesh_count, options);
            changed |= reference_window(ui.ctx(), options, *tool, color);
            view_axis = nav_gizmo(ui.ctx(), camera);
            changed |= panel.show(ui.ctx(), color, opacity);
            changed |= brush_panel.show(ui.ctx(), brush, opacity, *color);
            // Al final, cuando ya se sabe que cosas de la interfaz hay bajo el cursor.
            if let Some(r) = ring {
                brush_ring(ui.ctx(), r);
            }
        });
        // Cursor, portapapeles, enlaces...
        self.state.handle_platform_output(window, std::mem::take(&mut full_output.platform_output));
        (full_output, changed, view_axis, layers_dirty)
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
