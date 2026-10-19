//! texturecraft: ventana con un modelo, camara orbital y pintura por capas (ESPECIFICACION.md).
//! Controles: boton izquierdo = pintar, boton central = girar (con Mayus = mover), boton derecho = libre por ahora,
//! rueda = zoom,
//! 1 = vista ortografica frontal, 2 = perspectiva. Gizmo de ejes (arriba a la derecha): clic en un eje = girar
//! (animado) a esa vista ortografica; clic otra vez en el mismo = la vista opuesta. Ctrl+Z = deshacer, Ctrl+Mayus+Z o
//! Ctrl+Y = rehacer (por ahora solo trazos).
//! Capas (pruebas, hasta tener panel): N = capa nueva, [ ] = bajar/subir de capa, B / Mayus+B = modo de mezcla
//! siguiente/anterior, + - = opacidad, V = ojo, C = recorte, L = bloqueo, A = bloqueo alfa, K = cambiar color.
//! Herramientas (boton izquierdo): P = pincel, G = borrador (goma), F = relleno (balde), I = cuentagotas (toma el color
//! de lo que ves), D = difuminar, U = borrosidad (estas dos por ahora solo el boton), H = mano (el boton izquierdo mueve la vista en vez de pintar).
//! Exportar PNG a la carpeta actual: E = textura con fondo, Mayus+E = transparente, X = solo la capa elegida.

mod blend;
mod bvh;
mod camera;
mod color_panel;
mod export;
mod fill;
mod layers;
mod obj;
mod paint;
mod ui;
mod undo;
mod uvmap;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use blend::BlendMode;
use bvh::TriBvh;
use camera::{Camera, ViewAxis};
use layers::LayerStack;
use obj::{ObjMesh, load_obj};
use paint::{DabOptions, Rect, StrokeState, TEX_SIZE, dab_box};
use ui::UiLayer;
use undo::History;
use uvmap::UvMaps;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Pincel de prueba del paso 3: suave, radio 18 texeles, rojo (los valores iniciales del panel de la app).
const BRUSH_RADIUS: f32 = 18.0;
/// Distancia entre toques, como fraccion del radio (Suave = 0.25, seccion 7.5).
const BRUSH_SPACING: f32 = 0.25;
/// Colores de prueba para distinguir capas (tecla K).
const PALETTE: [[u8; 3]; 6] = [[225, 70, 60], [60, 120, 230], [70, 190, 90], [240, 200, 50], [20, 20, 20], [255, 255, 255]];
const CLEAR: wgpu::Color = wgpu::Color { r: 0.16, g: 0.16, b: 0.18, a: 1.0 };

/// Herramienta activa con el boton izquierdo (seccion 8).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Brush,
    Eraser,
    Fill,
    Eyedropper,
    Smudge,
    Blur,
    /// Mano: con el boton izquierdo mueve la vista en vez de girarla (el derecho no pinta con ella).
    Hand,
}

impl Tool {
    fn name(self) -> &'static str {
        match self {
            Tool::Brush => "pincel",
            Tool::Eraser => "borrador",
            Tool::Fill => "relleno",
            Tool::Eyedropper => "cuentagotas",
            Tool::Smudge => "difuminar",
            Tool::Blur => "borrosidad",
            Tool::Hand => "mano",
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    texture: wgpu::Texture,
    layers: LayerStack,
    stroke: StrokeState,
    history: History,
    brush_color: [u8; 3],
    /// Opacidad del pincel (0..1), la barra de opacidad del panel de color.
    brush_opacity: f32,
    tool: Tool,
    /// Candado de la barra: con la vista bloqueada la camara no gira, no se mueve y no hace zoom.
    view_locked: bool,
    palette_index: usize,
    bvh: TriBvh,
    /// UVs del modelo (u,v por vertice) y los mapas de islas/cobertura que salen de ellas (se arman la primera vez
    /// que se rellena y se tiran al cambiar de modelo).
    uvs: Vec<f32>,
    uv_maps: Option<UvMaps>,
    /// Cuantas mallas (partes `o`/`g` del OBJ) tiene el modelo; 1 si no trae partes. Es el numero del boton del cubo.
    mesh_count: usize,
    /// Interfaz egui (barra de herramientas...), dibujada encima del modelo.
    ui: UiLayer,
}

struct App {
    gpu: Option<Gpu>,
    camera: Camera,
    mouse: (f32, f32),
    left_down: bool,
    middle_down: bool,
    /// Boton derecho apretado (reservado: por ahora no hace nada).
    #[allow(dead_code)]
    right_down: bool,
    /// Ultima posicion de pantalla del trazo en curso.
    last_paint: Option<(f32, f32)>,
    modifiers: ModifiersState,
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("profundidad"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Lado mas largo al que se ajusta cada modelo (la camara por defecto lo encuadra bien).
const FIT_SIZE: f32 = 4.5;

/// Esfera de prueba: mide 1 de diametro, se agranda a `FIT_SIZE` como un OBJ cargado.
fn test_sphere() -> ObjMesh {
    let mut mesh = ObjMesh::uv_sphere(48, 24);
    for p in mesh.positions.iter_mut() {
        *p *= FIT_SIZE;
    }
    mesh
}

/// Lee un OBJ (Y arriba, como los de Blender al exportar) y lo centra y ajusta a `FIT_SIZE`.
fn load_mesh(path: &Path) -> Result<ObjMesh, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let mesh = load_obj(&text, FIT_SIZE, true).map_err(|e| format!("{}: {e}", path.display()))?;
    if !mesh.has_uvs {
        eprintln!("Aviso: {} no trae UVs, no se podra pintar bien.", path.display());
    }
    Ok(mesh)
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Gpu {
    fn new(event_loop: &ActiveEventLoop, window: Arc<Window>, mesh: &ObjMesh) -> Gpu {
        // winit recomienda pasar su handle de pantalla al crear la instancia (Wayland/X11/GL lo necesitan).
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        )));
        let surface = instance.create_surface(window.clone()).expect("no se pudo crear la superficie");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no hay GPU compatible");
        log::info!("GPU: {:?}", adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("no se pudo abrir el dispositivo");

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("la superficie no es compatible con el adaptador");
        // Formato sRGB para que el shader escriba en lineal y se vea igual que en la app.
        let caps = surface.get_capabilities(&adapter);
        if let Some(f) = caps.formats.iter().copied().find(|f| f.is_srgb()) {
            config.format = f;
        }
        // La interfaz (egui) se dibuja con una vista NO sRGB de esta misma superficie: egui mezcla los bordes
        // suavizados en espacio gamma, y sobre un destino sRGB quedan duros (se ven "pixeleados"). El modelo sigue
        // usando la vista sRGB. Para poder crear esa segunda vista hay que declararla aqui.
        if config.format.is_srgb() {
            config.view_formats = vec![config.format.remove_srgb_suffix()];
        }
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        // Textura de prueba.
        let tex_size = TEX_SIZE;
        let layers = LayerStack::new(tex_size);
        let stroke = StrokeState::new(tex_size);
        let texture = device.create_texture_with_data(
            &queue,
            &wgpu::TextureDescriptor {
                label: Some("tablero"),
                size: wgpu::Extent3d { width: tex_size, height: tex_size, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Sin conversion sRGB: la luz se multiplica sobre los valores tal cual, como en la app Android (el
                // shader pasa el resultado a lineal al final porque la superficie si es sRGB).
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &layers.composite,
        );
        let tex_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&tex_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("modelo"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 32,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            // Sin culling: el sentido de giro de los OBJ varia y asi se ve siempre.
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&mesh.interleaved()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let depth_view = create_depth_view(&device, config.width, config.height);

        let ui = UiLayer::new(&window, &device, config.format.remove_srgb_suffix());

        Gpu {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            vertex_count: mesh.vertex_count() as u32,
            uniform_buffer,
            bind_group,
            depth_view,
            texture,
            layers,
            stroke,
            history: History::default(),
            brush_color: PALETTE[0],
            brush_opacity: 1.0,
            tool: Tool::Brush,
            view_locked: false,
            palette_index: 0,
            bvh: TriBvh::new(mesh),
            uvs: mesh.uvs.clone(),
            uv_maps: None,
            mesh_count: mesh.parts.len().max(1),
            ui,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, width, height);
    }

    /// Cambia el modelo que se dibuja.
    fn set_mesh(&mut self, mesh: &ObjMesh) {
        self.vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&mesh.interleaved()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.vertex_count = mesh.vertex_count() as u32;
        self.bvh = TriBvh::new(mesh);
        self.uvs = mesh.uvs.clone();
        self.uv_maps = None;
        self.mesh_count = mesh.parts.len().max(1);
    }

    /// Rayo de pantalla -> BVH -> UV del punto tocado (`None` si el cursor no esta sobre el modelo).
    fn pick_uv(&self, camera: &Camera, px: f32, py: f32) -> Option<(f32, f32)> {
        let (origin, dir) = camera.ray(px, py, self.config.width as f32, self.config.height as f32);
        self.bvh.pick(origin, dir).map(|h| (h.u, h.v))
    }

    /// Sube a la GPU el rectangulo de la textura compuesta.
    fn upload_rect(&mut self, rect: Rect) {
        let (x0, y0, x1, y1) = rect;
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let bytes = self.layers.rect_bytes(rect);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: x0, y: y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    /// Tras cambiar ajustes de capas (ojo, opacidad, modo...): recompone todo y lo sube.
    fn refresh_all(&mut self) {
        self.layers.composite_all();
        let m = self.layers.size - 1;
        self.upload_rect((0, 0, m, m));
        self.window.set_title(&self.status());
        self.window.request_redraw();
    }

    /// Texto de estado (en la barra de titulo, mientras no haya panel): herramienta, color y capa elegida.
    fn status(&self) -> String {
        let l = self.layers.selected_layer();
        let c = self.brush_color;
        format!(
            "texturecraft - {} #{:02X}{:02X}{:02X} | capa {}/{} \"{}\" | {} | {}%{}{}{}{}",
            self.tool.name(),
            c[0],
            c[1],
            c[2],
            self.layers.selected + 1,
            self.layers.layers.len(),
            l.name,
            l.blend.name(),
            l.opacity,
            if l.visible { "" } else { " | oculta" },
            if l.locked { " | bloqueada" } else { "" },
            if l.alpha_lock { " | bloqueo alfa" } else { "" },
            if l.clip_to_below { " | recorte" } else { "" },
        )
    }

    /// Cuentagotas (seccion 8.2): toma el color de la textura COMPUESTA (lo que se ve, con fondo y todas las capas)
    /// en el punto de pantalla y lo pone como color del pincel.
    fn pick_color(&mut self, camera: &Camera, px: f32, py: f32) {
        let Some((u, v)) = self.pick_uv(camera, px, py) else { return };
        let max = (self.layers.size - 1) as f32;
        let x = ((u - u.floor()) * max).round() as u32;
        let y = ((v - v.floor()) * max).round() as u32;
        let i = ((y * self.layers.size + x) * 4) as usize;
        let c = &self.layers.composite;
        self.brush_color = [c[i], c[i + 1], c[i + 2]];
        self.window.set_title(&self.status());
        // El cuadrito de color de la barra tambien debe mostrar el color nuevo.
        self.window.request_redraw();
    }

    /// Un toque en `(u, v)` sobre la capa elegida; recompone el rectangulo y lo suma a `dirty`.
    fn dab_uv(&mut self, (u, v): (f32, f32), dirty: &mut Option<Rect>) {
        let sel = self.layers.selected;
        let layer = &mut self.layers.layers[sel];
        if !layer.paintable() {
            return;
        }
        let opts = DabOptions { alpha_lock: layer.alpha_lock, erase: self.tool == Tool::Eraser };
        // Antes de tocar nada: copia de las baldosas que va a cambiar el toque (deshacer).
        let size = self.layers.size;
        self.history.save_tiles(&layer.pixels, size, dab_box(size, u, v, BRUSH_RADIUS));
        if let Some(r) = self.stroke.dab(&mut layer.pixels, u, v, BRUSH_RADIUS, self.brush_color, self.brush_opacity, opts) {
            self.layers.composite_rect(r);
            *dirty = Some(match *dirty {
                Some(d) => (d.0.min(r.0), d.1.min(r.1), d.2.max(r.2), d.3.max(r.3)),
                None => r,
            });
        }
    }

    /// Relleno (seccion 8.6): un solo toque por apoyo, entra al historial como un paso. Devuelve true si cambio algo.
    fn fill_at(&mut self, camera: &Camera, px: f32, py: f32) -> bool {
        let Some((u, v)) = self.pick_uv(camera, px, py) else { return false };
        let sel = self.layers.selected;
        if !self.layers.layers[sel].paintable() {
            return false;
        }
        let size = self.layers.size;
        let max = (size - 1) as f32;
        let x = ((u - u.floor()) * max).round() as u32;
        let y = ((v - v.floor()) * max).round() as u32;
        // Islas y cobertura UV: se arman la primera vez.
        let maps = self.uv_maps.get_or_insert_with(|| UvMaps::build(&self.uvs, size));
        let layer = &mut self.layers.layers[sel];
        let Some(plan) = fill::plan(&layer.pixels, size, x, y, self.brush_color, layer.alpha_lock, maps) else {
            return false;
        };
        // Antes de tocar nada: copia de las baldosas que va a cambiar (deshacer).
        self.history.begin(layer.id);
        for r in fill::touched_tiles(&plan, size) {
            self.history.save_tiles(&layer.pixels, size, r);
        }
        fill::apply(&mut layer.pixels, &plan, size, self.brush_color, layer.alpha_lock);
        self.history.end();
        self.layers.composite_rect(plan.rect);
        self.upload_rect(plan.rect);
        true
    }

    /// Apoyar el boton: empieza un trazo nuevo y pinta el primer toque. Devuelve true si cambio algo.
    fn stroke_begin(&mut self, camera: &Camera, px: f32, py: f32) -> bool {
        // El cuentagotas no pinta ni entra al historial: solo toma el color (tambien al arrastrar).
        if self.tool == Tool::Eyedropper {
            self.pick_color(camera, px, py);
            return false;
        }
        // Difuminar y borrosidad: todavia solo estan los botones, no hacen nada al pintar (para no pintar color por error).
        if self.tool == Tool::Smudge || self.tool == Tool::Blur || self.tool == Tool::Hand {
            return false;
        }
        if self.tool == Tool::Fill {
            return self.fill_at(camera, px, py);
        }
        self.stroke.begin_stroke();
        self.history.begin(self.layers.selected_layer().id);
        let mut dirty = None;
        if let Some(uv) = self.pick_uv(camera, px, py) {
            self.dab_uv(uv, &mut dirty);
        }
        dirty.map(|r| self.upload_rect(r)).is_some()
    }

    /// Soltar el boton: el trazo entra al historial.
    fn stroke_end(&mut self) {
        self.history.end();
    }

    /// Exporta un PNG a la carpeta actual (seccion 13). `kind`: 0 = textura con fondo, 1 = textura transparente,
    /// 2 = solo la capa elegida (sus pixeles tal cual, sin opacidad ni visibilidad). Devuelve el mensaje a mostrar.
    fn export_png(&self, kind: u8) -> String {
        let size = self.layers.size;
        let (base, pixels) = match kind {
            0 => ("texturecraft_textura", self.layers.export_composite(true)),
            1 => ("texturecraft_textura_transparente", self.layers.export_composite(false)),
            _ => ("texturecraft_capa", self.layers.selected_layer().pixels.clone()),
        };
        let path = export::free_path(base);
        match export::write_png(&path, size, &pixels, true) {
            Ok(()) => {
                let full = std::fs::canonicalize(&path).unwrap_or(path.clone());
                println!("Exportado: {}", full.display());
                format!("texturecraft - exportado: {}", path.display())
            }
            Err(e) => {
                eprintln!("{e}");
                format!("texturecraft - no se pudo exportar ({e})")
            }
        }
    }

    /// Deshacer (`redo = false`) o rehacer el ultimo trazo. Devuelve true si cambio algo.
    fn undo_redo(&mut self, redo: bool) -> bool {
        let rect = if redo { self.history.redo(&mut self.layers) } else { self.history.undo(&mut self.layers) };
        match rect {
            Some(r) => {
                self.layers.composite_rect(r);
                self.upload_rect(r);
                self.window.request_redraw();
                true
            }
            None => false,
        }
    }

    /// Mover con el boton apretado: rellena el tramo `from` -> `to` con toques espaciados (seccion 7.4).
    fn stroke_move(&mut self, camera: &Camera, from: (f32, f32), to: (f32, f32)) -> bool {
        if self.tool == Tool::Eyedropper {
            self.pick_color(camera, to.0, to.1);
            return false;
        }
        // El relleno es de un solo toque por apoyo: arrastrar no vuelve a rellenar. Difuminar y borrosidad aun no pintan.
        if self.tool == Tool::Fill || self.tool == Tool::Smudge || self.tool == Tool::Blur || self.tool == Tool::Hand {
            return false;
        }
        let spacing = (BRUSH_RADIUS * BRUSH_SPACING).max(0.5);
        let ua = self.pick_uv(camera, from.0, from.1);
        let ub = self.pick_uv(camera, to.0, to.1);
        let mut dirty = None;
        self.stroke_segment(camera, from, ua, to, ub, spacing, 0, &mut dirty);
        dirty.map(|r| self.upload_rect(r)).is_some()
    }

    /// Parte el tramo por la mitad hasta que los extremos quedan a `spacing` texeles o menos; solo se pinta el
    /// extremo final de cada trozo (el inicial ya se pinto). Si un extremo cae fuera del modelo se mide en pantalla.
    #[allow(clippy::too_many_arguments)]
    fn stroke_segment(
        &mut self,
        camera: &Camera,
        a: (f32, f32),
        ua: Option<(f32, f32)>,
        b: (f32, f32),
        ub: Option<(f32, f32)>,
        spacing: f32,
        depth: u32,
        dirty: &mut Option<Rect>,
    ) {
        let screen = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let close = match (ua, ub) {
            (Some(p), Some(q)) => {
                let max = (self.layers.size - 1) as f32;
                (((p.0 - q.0) * max).powi(2) + ((p.1 - q.1) * max).powi(2)).sqrt() <= spacing
            }
            _ => screen <= 6.0,
        };
        if close || screen < 0.05 || depth >= 12 {
            if let Some(q) = ub {
                self.dab_uv(q, dirty);
            }
            return;
        }
        let m = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
        let um = self.pick_uv(camera, m.0, m.1);
        self.stroke_segment(camera, a, ua, m, um, spacing, depth + 1, dirty);
        self.stroke_segment(camera, m, um, b, ub, spacing, depth + 1, dirty);
    }

    /// Dibuja un cuadro (modelo + interfaz). Devuelve cuanto falta para el proximo cuadro que pide egui
    /// (`ZERO` = ya mismo, `Duration::MAX` = ninguno) y el eje del gizmo al que se hizo clic, si hubo (la camara
    /// vive en `App`, asi que es quien la gira).
    fn render(&mut self, camera: &Camera) -> (Duration, Option<ViewAxis>) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                // Se pinta este cuadro y se reconfigura para el siguiente.
                self.surface.configure(&self.device, &self.config);
                f
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return (Duration::MAX, None);
            }
            // Ventana minimizada, tiempo agotado o error de validacion: se salta el cuadro.
            _ => return (Duration::MAX, None),
        };

        let aspect = self.config.width as f32 / self.config.height as f32;
        let (mvp, normal) = camera.matrices(aspect);
        let uniforms = Uniforms { mvp: mvp.to_cols_array_2d(), normal_matrix: normal.to_cols_array_2d() };
        self.queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Misma textura vista sin sRGB, solo para la interfaz (ver el comentario en `Gpu::new`).
        let ui_view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.remove_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cuadro") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("modelo"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(CLEAR), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
        }
        // Interfaz (egui) encima del modelo.
        let layer_count = self.layers.layers.len();
        let mesh_count = self.mesh_count;
        let (full_output, ui_changed, view_axis) = self.ui.run(
            &self.window,
            &mut self.tool,
            &mut self.brush_color,
            &mut self.brush_opacity,
            layer_count,
            mesh_count,
            camera,
            &mut self.view_locked,
        );
        if ui_changed {
            self.window.set_title(&self.status());
        }
        let size = [self.config.width, self.config.height];
        let (extra, repaint_delay) =
            self.ui.paint(&self.device, &self.queue, &mut encoder, &ui_view, size, full_output);
        self.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        self.window.pre_present_notify();
        self.queue.present(frame);
        (repaint_delay, view_axis)
    }
}

impl ApplicationHandler for App {
    /// Si egui pidio un cuadro "en X milisegundos" (tooltips, animaciones) y se cumplio el plazo, se redibuja.
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            if let Some(g) = &self.gpu {
                g.window.request_redraw();
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("texturecraft"))
                .expect("no se pudo crear la ventana"),
        );
        // `cargo run -- modelo.obj` abre ese modelo; sin argumento (o si falla) se ve la esfera de prueba.
        let mut title = String::from("texturecraft");
        let mesh = match std::env::args().nth(1) {
            Some(arg) => match load_mesh(Path::new(&arg)) {
                Ok(m) => {
                    title = format!("texturecraft - {}", file_name(Path::new(&arg)));
                    m
                }
                Err(e) => {
                    eprintln!("{e}");
                    test_sphere()
                }
            },
            None => test_sphere(),
        };
        window.set_title(&title);
        self.gpu = Some(Gpu::new(event_loop, window, &mesh));
        if let Some(g) = &self.gpu {
            g.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        // egui ve cada evento primero. Si lo "consume" (cursor sobre la barra, etc.), la camara y el pincel no
        // reaccionan. Los gestos que empezaron fuera de la interfaz siguen su curso aunque pasen por encima.
        let response = gpu.ui.on_window_event(&gpu.window, &event);
        if response.repaint {
            gpu.window.request_redraw();
        }
        let ui_has_it = response.consumed;
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gpu.resize(size.width, size.height);
                gpu.window.request_redraw();
            }
            // Arrastrar un .obj a la ventana lo abre.
            WindowEvent::DroppedFile(path) => match load_mesh(&path) {
                Ok(mesh) => {
                    gpu.set_mesh(&mesh);
                    gpu.window.set_title(&format!("texturecraft - {}", file_name(&path)));
                    gpu.window.request_redraw();
                }
                Err(e) => {
                    eprintln!("{e}");
                    gpu.window.set_title("texturecraft - no se pudo abrir el archivo (mira la terminal)");
                }
            },
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::MouseInput { state, button, .. } => {
                let down = state == ElementState::Pressed;
                // Un clic sobre la barra no gira la camara ni pinta. Soltar el boton siempre se atiende.
                if down && ui_has_it {
                    return;
                }
                match button {
                    MouseButton::Left => {
                        let was_down = self.left_down;
                        self.left_down = down;
                        if down {
                            self.last_paint = Some(self.mouse);
                            if gpu.stroke_begin(&self.camera, self.mouse.0, self.mouse.1) {
                                gpu.window.request_redraw();
                            }
                        } else if was_down {
                            self.last_paint = None;
                            gpu.stroke_end();
                        }
                    }
                    MouseButton::Middle => self.middle_down = down,
                    // Boton derecho: libre por ahora (sin funcion).
                    MouseButton::Right => self.right_down = down,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32, position.y as f32);
                let (dx, dy) = (x - self.mouse.0, y - self.mouse.1);
                self.mouse = (x, y);
                if self.left_down && gpu.tool == Tool::Hand {
                    // Mano: el boton izquierdo mueve la vista en vez de pintar (si la vista esta bloqueada, nada).
                    if !gpu.view_locked {
                        self.camera.pan_by(dx, dy);
                        gpu.window.request_redraw();
                    }
                } else if self.left_down {
                    let from = self.last_paint.unwrap_or((x, y));
                    self.last_paint = Some((x, y));
                    if gpu.stroke_move(&self.camera, from, (x, y)) {
                        gpu.window.request_redraw();
                    }
                } else if self.middle_down && !gpu.view_locked {
                    // Boton central = girar; con Mayus = mover.
                    if self.modifiers.shift_key() {
                        self.camera.pan_by(dx, dy);
                    } else {
                        self.camera.rotate_by(dx, dy);
                    }
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if ui_has_it || gpu.view_locked {
                    return;
                }
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.camera.zoom(steps);
                gpu.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !ui_has_it => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Character(ref c) if c == "1" && !gpu.view_locked => {
                        // Vista frontal ortografica (como el boton de vistas de la app).
                        self.camera.stop_animation();
                        self.camera.angle_x = 0.0;
                        self.camera.angle_y = 0.0;
                        self.camera.orthographic = true;
                        gpu.window.request_redraw();
                    }
                    Key::Character(ref c) if c == "2" && !gpu.view_locked => {
                        self.camera.orthographic = false;
                        gpu.window.request_redraw();
                    }
                    // Teclas de prueba de las capas (hasta tener el panel).
                    Key::Character(ref c) => {
                        let shift = self.modifiers.shift_key();
                        // Ctrl+Z deshace; Ctrl+Mayus+Z o Ctrl+Y rehacen.
                        if self.modifiers.control_key() {
                            match (c.to_lowercase().as_str(), shift) {
                                ("z", false) => {
                                    gpu.undo_redo(false);
                                }
                                ("z", true) | ("y", _) => {
                                    gpu.undo_redo(true);
                                }
                                _ => {}
                            }
                            return;
                        }
                        let sel = gpu.layers.selected;
                        let (mut recompose, mut retitle) = (false, false);
                        match c.to_lowercase().as_str() {
                            "n" => retitle = gpu.layers.add_layer(),
                            "]" => retitle = gpu.layers.select_step(1),
                            "[" => retitle = gpu.layers.select_step(-1),
                            "b" => {
                                let l = &mut gpu.layers.layers[sel];
                                let n = BlendMode::ALL.len();
                                let i = BlendMode::ALL.iter().position(|&m| m == l.blend).unwrap_or(0);
                                l.blend = BlendMode::ALL[if shift { (i + n - 1) % n } else { (i + 1) % n }];
                                recompose = true;
                            }
                            "v" => {
                                let l = &mut gpu.layers.layers[sel];
                                l.visible = !l.visible;
                                recompose = true;
                            }
                            "c" => {
                                let l = &mut gpu.layers.layers[sel];
                                l.clip_to_below = !l.clip_to_below;
                                recompose = true;
                            }
                            "l" => {
                                let l = &mut gpu.layers.layers[sel];
                                l.locked = !l.locked;
                                retitle = true;
                            }
                            "a" => {
                                let l = &mut gpu.layers.layers[sel];
                                l.alpha_lock = !l.alpha_lock;
                                retitle = true;
                            }
                            "-" => {
                                let l = &mut gpu.layers.layers[sel];
                                l.opacity = l.opacity.saturating_sub(10);
                                recompose = true;
                            }
                            "+" | "=" => {
                                let l = &mut gpu.layers.layers[sel];
                                l.opacity = (l.opacity + 10).min(100);
                                recompose = true;
                            }
                            "k" => {
                                gpu.palette_index = (gpu.palette_index + 1) % PALETTE.len();
                                gpu.brush_color = PALETTE[gpu.palette_index];
                                retitle = true;
                            }
                            "p" => {
                                gpu.tool = Tool::Brush;
                                retitle = true;
                            }
                            "g" => {
                                gpu.tool = Tool::Eraser;
                                retitle = true;
                            }
                            "f" => {
                                gpu.tool = Tool::Fill;
                                retitle = true;
                            }
                            "i" => {
                                gpu.tool = Tool::Eyedropper;
                                retitle = true;
                            }
                            "d" => {
                                gpu.tool = Tool::Smudge;
                                retitle = true;
                            }
                            "u" => {
                                gpu.tool = Tool::Blur;
                                retitle = true;
                            }
                            "h" => {
                                gpu.tool = Tool::Hand;
                                retitle = true;
                            }
                            "e" => {
                                let msg = gpu.export_png(if shift { 1 } else { 0 });
                                gpu.window.set_title(&msg);
                            }
                            "x" => {
                                let msg = gpu.export_png(2);
                                gpu.window.set_title(&msg);
                            }
                            _ => {}
                        }
                        if recompose {
                            gpu.refresh_all();
                        } else if retitle {
                            gpu.window.set_title(&gpu.status());
                            // La barra de herramientas tambien debe reflejar el cambio (P, G, I).
                            gpu.window.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                // Redibujo continuo solo mientras egui anima algo; en reposo la app duerme.
                // Si hay un giro animado de la vista (clic en el gizmo), se avanza antes de dibujar y se sigue
                // redibujando hasta que termine.
                let animating = self.camera.update_animation();
                let (delay, view_axis) = gpu.render(&self.camera);
                if let Some(axis) = view_axis {
                    if !gpu.view_locked {
                        self.camera.snap_to(axis);
                    }
                }
                if animating || view_axis.is_some() || delay.is_zero() {
                    gpu.window.request_redraw();
                } else if delay < Duration::from_secs(3600) {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + delay));
                } else {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("no se pudo crear el bucle de eventos");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        gpu: None,
        camera: Camera::default(),
        mouse: (0.0, 0.0),
        left_down: false,
        middle_down: false,
        right_down: false,
        last_paint: None,
        modifiers: ModifiersState::default(),
    };
    event_loop.run_app(&mut app).expect("error en el bucle de eventos");
}
