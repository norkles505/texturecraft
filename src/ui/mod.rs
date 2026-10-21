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
mod panel_mallas;
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
use crate::obj::MeshPart;

use barra_herramientas::*;
use barra_inferior::*;
use cursor::*;
use estilo::*;
use gizmo::*;
use panel_capas::*;
use panel_mallas::*;
use panel_menu::*;
use panel_opciones::*;
use ventana_referencia::*;
pub use cursor::BrushRing;

/// true si en este cuadro se apreto el boton principal del raton fuera de `panel` y fuera de la barra de herramientas
/// (sus botones abren y cierran los paneles por su cuenta: si tambien contaran como "fuera", el panel se cerraria y se
/// volveria a abrir en el mismo clic). Los paneles flotantes lo usan para cerrarse al hacer clic en el vacio.
fn pressed_outside(ctx: &egui::Context, panel: Rect) -> bool {
    let toolbar = ctx.memory(|m| m.area_rect(egui::Id::new("toolbar")));
    ctx.input(|i| i.pointer.primary_pressed())
        && ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !panel.contains(p) && !toolbar.is_some_and(|t| t.contains(p)))
}

/// Estado del panel de opciones (el que abre el boton de los interruptores de la barra inferior).
#[derive(Default)]
struct OptionsPanel {
    open: bool,
    /// Panel del menu (el que abre el boton de la flecha de la barra inferior).
    menu_open: bool,
    /// Panel de Ajustes (el que abre la fila "Ajustes" del menu): ventana centrada en la pantalla.
    settings_open: bool,
    /// Panel de capas (el que abre el boton de capas de la barra inferior), cuanto se desplazo su lista (en puntos)
    /// y si esta abierto el desplegable de modos de mezcla.
    layers_open: bool,
    layers_scroll: f32,
    /// Panel de mallas (el que abre el boton del cubo de la barra inferior) y cuanto se desplazo su lista (en puntos).
    meshes_open: bool,
    meshes_scroll: f32,
    /// Opacidad (0..1) del deslizador del panel de mallas. `None` = 1 (100 %). Todavia solo se muestra: no cambia el
    /// dibujo del modelo.
    meshes_opacity: Option<f32>,
    /// Mientras el selector de archivos del boton Importar esta abierto: canal por donde llega la ruta elegida
    /// (`None` = cancelado). Cuando llega, queda en `pending_import` hasta que `UiLayer::take_import` la entrega.
    mesh_import: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    pending_import: Option<std::path::PathBuf>,
    /// Menu de los tres puntos del panel de capas.
    more_open: bool,
    blend_open: bool,
    /// Cuanto se desplazo la lista del desplegable de modos de mezcla (en puntos).
    blend_scroll: f32,
    /// Interruptor "Ventana de referencia": mientras esta encendido se muestra la ventana flotante.
    reference_window: bool,
    /// Interruptor "Acumular pasada" APAGADO (asi el valor por defecto, `false`, es "encendido"): el trazo no pasa de la
    /// opacidad del pincel, como en photocraft. Ver `UiLayer::pass_ceiling`.
    pass_ceiling: bool,
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

/// Lo que devuelve `UiLayer::run` en cada cuadro.
pub struct UiOutput {
    /// Lo que hay que dibujar.
    pub full_output: egui::FullOutput,
    /// Cambio la herramienta, el pincel o el color (hay que actualizar el titulo de la ventana).
    pub changed: bool,
    /// Eje del gizmo al que se hizo clic (la camara debe girar a esa vista).
    pub view_axis: Option<ViewAxis>,
    /// El panel de capas cambio algo que obliga a recomponer la textura (ojo, opacidad, modo, recorte o fondo).
    pub layers_dirty: bool,
    /// El panel de mallas mostro u oculto alguna malla.
    pub parts_dirty: bool,
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

    /// El .obj que se eligio con el boton Importar del panel de mallas, si ya llego (se entrega una sola vez).
    pub fn take_import(&mut self) -> Option<std::path::PathBuf> {
        self.options.pending_import.take()
    }

    /// true si el interruptor "Acumular pasada" del panel de opciones esta APAGADO: el trazo sube hacia la opacidad
    /// del pincel y no la pasa (como photocraft). Encendido (por defecto) = la regla de siempre.
    pub fn pass_ceiling(&self) -> bool {
        self.options.pass_ceiling
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
        parts: &mut [MeshPart],
        selected_part: &mut usize,
        camera: &Camera,
        view_locked: &mut bool,
        ring: Option<BrushRing>,
    ) -> UiOutput {
        let raw_input = self.state.take_egui_input(window);
        let mut changed = false;
        let mut layers_dirty = false;
        let mut parts_dirty = false;
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
            let (layers_changed, parts_changed) = bottom_bar(ui, layers, parts, selected_part, options);
            layers_dirty |= layers_changed;
            parts_dirty |= parts_changed;
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
        UiOutput { full_output, changed, view_axis, layers_dirty, parts_dirty }
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
