//! texturecraft: ventana con un modelo, camara orbital y pintura por capas (ESPECIFICACION.md).
//! Controles: boton izquierdo = pintar, boton central = girar (con Mayus = mover), boton derecho = libre por ahora,
//! rueda = zoom,
//! 1 = vista ortografica frontal, 2 = perspectiva. Gizmo de ejes (arriba a la derecha): clic en un eje = girar
//! (animado) a esa vista ortografica; clic otra vez en el mismo = la vista opuesta. Ctrl+Z = deshacer, Ctrl+Mayus+Z o
//! Ctrl+Y = rehacer (por ahora solo trazos).
//! Piezas (seccion 3.3): un modelo con 2 a 12 partes se separa en una pieza por parte, cada una con su textura, sus
//! capas y su historial. La pieza activa es la que muestran el panel de capas y la exportacion; se elige en el panel de
//! mallas o apoyando el pincel sobre ella.
//! Capas (pruebas, hasta tener panel): N = capa nueva, [ ] = bajar/subir de capa, B / Mayus+B = modo de mezcla
//! siguiente/anterior, + - = opacidad, V = ojo, C = recorte, L = bloqueo, A = bloqueo alfa, K = cambiar color.
//! Herramientas (boton izquierdo): P = pincel, G = borrador (goma), F = relleno (balde), I = cuentagotas (toma el color
//! de lo que ves), D = difuminar, U = borrosidad (estas dos por ahora solo el boton), H = mano (el boton izquierdo mueve la vista en vez de pintar).
//! Exportar PNG a la carpeta actual: E = textura con fondo, Mayus+E = transparente, X = solo la capa elegida.

mod blend;
mod brush;
mod bvh;
mod camera;
mod export;
mod fill;
mod layers;
mod obj;
mod paint;
mod pinceles;
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
use brush::{BUILD_UP_INTERVAL, Brush};
use bvh::TriBvh;
use camera::{Camera, ViewAxis};
use glam::Vec3;
use layers::LayerStack;
use obj::{MeshPart, ObjMesh, load_obj, part_of_triangle, piece_ranges, triangle_visibility, visible_ranges};
use paint::{DabOptions, Rect, StrokeState, SurfTri, TEX_SIZE, surface_rect};
use ui::{BrushRing, UiLayer, UiOutput};
use undo::History;
use uvmap::UvMaps;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
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
    /// Mano: con el boton izquierdo mueve la vista en vez de pintar (el derecho no pinta con ella).
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

/// Una pieza del modelo (seccion 3.3): su propio tramo de vertices, su textura en la GPU, sus capas y su historial.
/// Un modelo con 2 a 12 partes tiene una pieza por parte; si no, una sola pieza que cubre todo.
struct Piece {
    /// Tramo de vertices del modelo que cubre esta pieza (multiplos de 3).
    first_vertex: usize,
    vertex_count: usize,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    layers: LayerStack,
    history: History,
    /// Mapas de islas y cobertura UV de los triangulos de esta pieza (se arman la primera vez que se rellena).
    uv_maps: Option<UvMaps>,
}

/// Lo que toco un rayo de pantalla: en que pieza y en que parte cayo, la UV del punto, el triangulo, su posicion 3D y
/// la direccion en que miraba el rayo (unitaria).
#[derive(Clone, Copy)]
struct Pick {
    piece: usize,
    part: usize,
    u: f32,
    v: f32,
    tri: usize,
    pos: Vec3,
    view: Vec3,
}

/// Crea una pieza: textura propia (con el fondo ya compuesto) y el grupo de enlaces que la une a los uniforms.
fn new_piece(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    (first_vertex, vertex_count): (usize, usize),
) -> Piece {
    let layers = LayerStack::new(TEX_SIZE);
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("tablero"),
            size: wgpu::Extent3d { width: TEX_SIZE, height: TEX_SIZE, depth_or_array_layers: 1 },
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
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&tex_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    });
    Piece { first_vertex, vertex_count, texture, bind_group, layers, history: History::default(), uv_maps: None }
}

/// Llamadas de dibujo `(pieza, tramo de vertices)`: solo las mallas visibles. Con una pieza por parte cada parte se
/// dibuja con la textura de su pieza; con una sola pieza las partes visibles seguidas se juntan en una llamada.
fn draw_calls(piece_count: usize, parts: &[MeshPart]) -> Vec<(usize, std::ops::Range<u32>)> {
    if piece_count == parts.len() {
        parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.visible)
            .map(|(i, p)| (i, p.first_vertex as u32..(p.first_vertex + p.vertex_count) as u32))
            .collect()
    } else {
        visible_ranges(parts).into_iter().map(|r| (0, r)).collect()
    }
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    /// Llamadas de dibujo (pieza y tramo de vertices) de las mallas visibles; las ocultas no entran.
    draw_ranges: Vec<(usize, std::ops::Range<u32>)>,
    uniform_buffer: wgpu::Buffer,
    /// Para crear la textura de cada pieza nueva.
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    depth_view: wgpu::TextureView,
    /// Piezas del modelo, cada una con su textura, sus capas y su historial (seccion 3.3).
    pieces: Vec<Piece>,
    /// Malla elegida (posicion en `parts`): su pieza es la activa. Con una sola pieza no cambia nada.
    selected_part: usize,
    /// Estado del trazo en curso (se comparte entre piezas: un trazo a la vez).
    stroke: StrokeState,
    /// Pieza donde esta pintando el trazo en curso (`None` = aun no toco el modelo).
    stroke_piece: Option<usize>,
    /// Direccion del trazo en 3D (unitaria; cero = aun no se sabe) y el punto desde el que se mide: el carboncillo
    /// orienta su punta con ella. Se reinician al apoyar el boton.
    stroke_dir: Vec3,
    last_dab_pos: Option<Vec3>,
    /// Cuanto falta recorrer (en unidades del modelo) hasta el proximo toque del trazo en curso. El resto pasa de un
    /// tramo del raton al siguiente (como `next_at` de photocraft), asi los toques quedan a un espaciado exacto aunque
    /// el raton mande muchos eventos pequenos.
    stroke_to_next: f32,
    brush_color: [u8; 3],
    /// Opacidad del pincel (0..1), la barra de opacidad del panel de color.
    brush_opacity: f32,
    /// Pincel elegido en el panel de pinceles: preset (falloff y espaciado) y radio en texeles.
    brush: Brush,
    tool: Tool,
    /// Candado de la barra: con la vista bloqueada la camara no gira, no se mueve y no hace zoom.
    view_locked: bool,
    palette_index: usize,
    bvh: TriBvh,
    /// UVs del modelo (u,v por vertice); de ellas salen los mapas de islas y cobertura de cada pieza.
    uvs: Vec<f32>,
    /// Mallas del modelo (partes `o`/`g` del OBJ; si no trae partes, una sola que es todo el modelo) y si se ven. Su
    /// cantidad es el numero del boton del cubo y su lista es la del panel de mallas.
    parts: Vec<MeshPart>,
    /// Interfaz egui (barra de herramientas...), dibujada encima del modelo.
    ui: UiLayer,
    /// Ultimo radio (en pixeles de pantalla) del circulo del pincel; se usa cuando el cursor sale del modelo.
    ring_radius: f32,
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
    /// El cursor esta dentro de la ventana (para mostrar u ocultar el circulo del pincel).
    cursor_in_window: bool,
    /// Proximo toque del temporizador del aerografo (`None` = no hay trazo que lo necesite).
    next_build_up: Option<Instant>,
    /// El puntero se movio desde el ultimo toque del temporizador: ese tramo ya deposito pintura al moverse, asi que
    /// el temporizador se salta ese toque (solo deposita cuando el puntero esta quieto).
    moved_since_tick: bool,
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

/// Cuanto mide, en unidades del modelo, cada punto del tamano del pincel (el slider va de 1 a 100). El pincel mide
/// lo mismo en toda la superficie, sin importar como este estirado el UV. Con 450 puntos por modelo, el tamano 18
/// inicial cubre un 4 % del lado mas largo.
const WORLD_PER_RADIUS: f32 = FIT_SIZE / 450.0;

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

/// Nombre apto para un archivo: lo que no es letra o numero pasa a `_`.
fn slug(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

impl Gpu {
    fn new(event_loop: &ActiveEventLoop, window: Arc<Window>, mesh: &ObjMesh, model_name: &str) -> Gpu {
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

        let stroke = StrokeState::new(TEX_SIZE);
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
        let parts = mesh.scene_parts(model_name);
        // Una pieza por parte (2 a 12 partes) o una sola, cada una con su textura, capas e historial.
        let pieces: Vec<Piece> = piece_ranges(&parts)
            .into_iter()
            .map(|r| new_piece(&device, &queue, &bind_group_layout, &uniform_buffer, &sampler, r))
            .collect();
        let draw_ranges = draw_calls(pieces.len(), &parts);

        Gpu {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            draw_ranges,
            uniform_buffer,
            bind_group_layout,
            sampler,
            depth_view,
            pieces,
            selected_part: 0,
            stroke,
            stroke_piece: None,
            stroke_dir: Vec3::ZERO,
            last_dab_pos: None,
            stroke_to_next: 0.0,
            brush_color: PALETTE[0],
            brush_opacity: 1.0,
            brush: Brush::default(),
            tool: Tool::Brush,
            view_locked: false,
            palette_index: 0,
            bvh: TriBvh::new(mesh),
            uvs: mesh.uvs.clone(),
            parts,
            ui,
            ring_radius: 20.0,
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

    /// Pieza donde cae una malla: con una pieza por parte es la misma posicion; con una sola pieza, la 0.
    fn piece_for_part(&self, part: usize) -> usize {
        if self.pieces.len() == self.parts.len() { part.min(self.pieces.len() - 1) } else { 0 }
    }

    /// Pieza activa: la que muestran el panel de capas, el titulo y la exportacion (seccion 3.3).
    fn active(&self) -> usize {
        self.piece_for_part(self.selected_part)
    }

    /// Capas de la pieza activa.
    fn layers_mut(&mut self) -> &mut LayerStack {
        let a = self.active();
        &mut self.pieces[a].layers
    }

    /// Hace activa la pieza de la malla `part` (al apoyar el pincel sobre ella). Con una sola pieza no cambia nada.
    fn activate_part(&mut self, part: usize) {
        if self.pieces.len() == self.parts.len() && self.selected_part != part {
            self.selected_part = part;
            self.window.set_title(&self.status());
            self.window.request_redraw();
        }
    }

    /// Cambia el modelo que se dibuja. `model_name` es el nombre de la malla si el OBJ no trae partes. Las piezas
    /// nuevas empiezan con una textura y un historial vacios (el historial era de otro modelo).
    fn set_mesh(&mut self, mesh: &ObjMesh, model_name: &str) {
        self.vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&mesh.interleaved()),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.bvh = TriBvh::new(mesh);
        self.uvs = mesh.uvs.clone();
        self.parts = mesh.scene_parts(model_name);
        self.pieces = piece_ranges(&self.parts)
            .into_iter()
            .map(|r| {
                new_piece(&self.device, &self.queue, &self.bind_group_layout, &self.uniform_buffer, &self.sampler, r)
            })
            .collect();
        self.selected_part = 0;
        self.stroke_piece = None;
        self.apply_visibility();
    }

    /// Tras mostrar u ocultar mallas: recalcula las llamadas de dibujo y los triangulos que puede tocar el pincel
    /// (una malla oculta ni se dibuja ni se pinta: el rayo la atraviesa, seccion 3.1).
    fn apply_visibility(&mut self) {
        self.draw_ranges = draw_calls(self.pieces.len(), &self.parts);
        let tris = self.bvh.triangle_count();
        self.bvh.set_visible(triangle_visibility(&self.parts, tris));
        self.window.request_redraw();
    }

    /// Abre un .obj (arrastrado a la ventana o elegido en el panel de mallas) y reemplaza el modelo actual.
    fn open_obj(&mut self, path: &Path) {
        match load_mesh(path) {
            Ok(mesh) => {
                self.set_mesh(&mesh, &file_name(path));
                self.window.set_title(&format!("texturecraft - {}", file_name(path)));
                self.window.request_redraw();
            }
            Err(e) => {
                eprintln!("{e}");
                self.window.set_title("texturecraft - no se pudo abrir el archivo (mira la terminal)");
            }
        }
    }

    /// Rayo de pantalla -> BVH -> pieza, parte, UV, triangulo y posicion 3D del punto tocado (`None` si el cursor no
    /// esta sobre el modelo).
    fn pick_uv(&self, camera: &Camera, px: f32, py: f32) -> Option<Pick> {
        let (origin, dir) = camera.ray(px, py, self.config.width as f32, self.config.height as f32);
        self.bvh.pick(origin, dir).map(|h| {
            let part = part_of_triangle(&self.parts, h.tri);
            Pick {
                piece: self.piece_for_part(part),
                part,
                u: h.u,
                v: h.v,
                tri: h.tri,
                pos: origin + dir * h.t,
                view: dir.normalize_or_zero(),
            }
        })
    }

    /// Radio del pincel en pixeles de pantalla en el punto `(x, y)`: se mide cuantas unidades del modelo hay por pixel
    /// alrededor de ese punto (dos rayos vecinos) y se divide el radio del pincel entre eso. `None` si el cursor no
    /// esta sobre el modelo.
    fn brush_radius_px(&self, camera: &Camera, (x, y): (f32, f32)) -> Option<f32> {
        let a = self.pick_uv(camera, x, y)?;
        let step = 6.0;
        let mut per_px: Vec<f32> = Vec::with_capacity(2);
        for (dx, dy) in [(step, 0.0), (0.0, step)] {
            // Un punto vecino en otra pieza no sirve para medir.
            if let Some(b) = self.pick_uv(camera, x + dx, y + dy).filter(|b| b.piece == a.piece) {
                let d = a.pos.distance(b.pos) / step;
                if d > 0.0 {
                    per_px.push(d);
                }
            }
        }
        let units_per_px = match per_px.as_slice() {
            [] => return None,
            [one] => *one,
            [p, q] => {
                let (lo, hi) = (p.min(*q), p.max(*q));
                // Una medida mucho mayor que la otra es un salto de profundidad (borde del modelo): se descarta.
                if hi > 3.0 * lo { lo } else { (lo + hi) * 0.5 }
            }
            _ => unreachable!(),
        };
        Some(self.brush.radius * WORLD_PER_RADIUS / units_per_px)
    }

    /// Sube a la GPU el rectangulo de la textura compuesta de la pieza `piece`.
    fn upload_rect(&mut self, piece: usize, rect: Rect) {
        let (x0, y0, x1, y1) = rect;
        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let p = &self.pieces[piece];
        let bytes = p.layers.rect_bytes(rect);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &p.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: x0, y: y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    /// Tras cambiar ajustes de capas (ojo, opacidad, modo...) en la pieza `piece`: recompone todo y lo sube.
    fn refresh_piece(&mut self, piece: usize) {
        self.pieces[piece].layers.composite_all();
        let m = self.pieces[piece].layers.size - 1;
        self.upload_rect(piece, (0, 0, m, m));
        self.window.set_title(&self.status());
        self.window.request_redraw();
    }

    /// Lo mismo, para la pieza activa.
    fn refresh_all(&mut self) {
        let a = self.active();
        self.refresh_piece(a);
    }

    /// Texto de estado (en la barra de titulo, mientras no haya panel): herramienta, color y capa elegida (de la
    /// pieza activa; con varias piezas, tambien su nombre).
    fn status(&self) -> String {
        let a = self.active();
        let stack = &self.pieces[a].layers;
        let l = stack.selected_layer();
        let c = self.brush_color;
        let piece = if self.pieces.len() > 1 {
            format!(" | pieza \"{}\"", self.parts[self.selected_part.min(self.parts.len() - 1)].name)
        } else {
            String::new()
        };
        format!(
            "texturecraft - {} #{:02X}{:02X}{:02X}{} | capa {}/{} \"{}\" | {} | {}%{}{}{}{}",
            self.tool.name(),
            c[0],
            c[1],
            c[2],
            piece,
            stack.selected + 1,
            stack.layers.len(),
            l.name,
            l.blend.name(),
            l.opacity,
            if l.visible { "" } else { " | oculta" },
            if l.locked { " | bloqueada" } else { "" },
            if l.alpha_lock { " | bloqueo alfa" } else { "" },
            if l.clip_to_below { " | recorte" } else { "" },
        )
    }

    /// Color de la textura COMPUESTA (lo que se ve) bajo el punto de pantalla, o `None` si ahi no hay modelo.
    fn color_at(&self, camera: &Camera, px: f32, py: f32) -> Option<[u8; 3]> {
        let p = self.pick_uv(camera, px, py)?;
        let stack = &self.pieces[p.piece].layers;
        let max = (stack.size - 1) as f32;
        let x = ((p.u - p.u.floor()) * max).round() as u32;
        let y = ((p.v - p.v.floor()) * max).round() as u32;
        let i = ((y * stack.size + x) * 4) as usize;
        let c = &stack.composite;
        Some([c[i], c[i + 1], c[i + 2]])
    }

    /// Cuentagotas (seccion 8.2): toma el color de la textura COMPUESTA (lo que se ve, con fondo y todas las capas)
    /// en el punto de pantalla y lo pone como color del pincel.
    fn pick_color(&mut self, camera: &Camera, px: f32, py: f32) {
        let Some(c) = self.color_at(camera, px, py) else { return };
        self.brush_color = c;
        self.window.set_title(&self.status());
        // El cuadrito de color de la barra tambien debe mostrar el color nuevo.
        self.window.request_redraw();
    }

    /// El trazo en curso pasa a pintar en la pieza de `pick` (la primera vez, o al cruzar a otra pieza): cierra el
    /// registro del historial de la anterior, abre el de esta, reinicia el estado del trazo y la hace pieza activa
    /// (seccion 3.3).
    fn enter_piece(&mut self, pick: Pick) {
        if self.stroke_piece == Some(pick.piece) {
            return;
        }
        if let Some(old) = self.stroke_piece {
            self.pieces[old].history.end();
        }
        self.stroke_piece = Some(pick.piece);
        self.activate_part(pick.part);
        self.stroke.begin_stroke();
        let piece = &mut self.pieces[pick.piece];
        let id = piece.layers.selected_layer().id;
        piece.history.begin(id);
    }

    /// Un toque en el punto de `pick` sobre la capa elegida de su pieza; recompone el rectangulo y lo suma a
    /// `dirty` (que es siempre de la pieza del trazo en curso: al cruzar a otra pieza se sube primero el anterior).
    /// El toque mide en el modelo (ver `paint.rs`): se buscan los triangulos de la pieza que estan cerca del punto
    /// tocado y miran a la camara, y cada texel se mide por su distancia 3D.
    fn dab_surface(&mut self, pick: Pick, dirty: &mut Option<Rect>) {
        if self.stroke_piece != Some(pick.piece) {
            if let (Some(old), Some(r)) = (self.stroke_piece, dirty.take()) {
                self.upload_rect(old, r);
            }
            self.enter_piece(pick);
        }
        let bvh = &self.bvh;
        let piece = &mut self.pieces[pick.piece];
        let sel = piece.layers.selected;
        let size = piece.layers.size;
        let layer = &mut piece.layers.layers[sel];
        if !layer.paintable() {
            return;
        }
        let radius = self.brush.radius * WORLD_PER_RADIUS;
        // Triangulos cercanos que son de esta pieza (cada pieza tiene su textura). El pastel dispersa su toque de lado:
        // se busca con ese margen de mas.
        let mut near = Vec::new();
        bvh.query_sphere(pick.pos, radius * (1.0 + self.brush.kind.scatter()), &mut near);
        let (first_tri, end_tri) = (piece.first_vertex / 3, (piece.first_vertex + piece.vertex_count) / 3);
        // Hacia donde mira un triangulo respecto a la camara. El signo del triangulo tocado dice como esta orientado
        // el modelo (el giro de las caras varia entre OBJ): solo se pintan los que miran igual que el. Asi la parte de
        // atras de una pared fina no se pinta.
        let facing = |tri: usize| {
            let (p, _) = bvh.tri(tri);
            (p[1] - p[0]).cross(p[2] - p[0]).dot(-pick.view)
        };
        let side = if facing(pick.tri) < 0.0 { -1.0 } else { 1.0 };
        let tris: Vec<SurfTri> = near
            .iter()
            .copied()
            .filter(|&t| t >= first_tri && t < end_tri && (t == pick.tri || side * facing(t) > 0.0))
            .map(|t| {
                let (p, uv) = bvh.tri(t);
                SurfTri { p: *p, uv: *uv }
            })
            .collect();
        let Some(area) = surface_rect(size, &tris) else { return };
        // Normal del triangulo tocado: la tiza la usa para orientar su punta sobre la superficie (el signo da igual).
        let normal = {
            let (p, _) = bvh.tri(pick.tri);
            (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero()
        };
        // Direccion del trazo: del ultimo punto de referencia a este. El punto de referencia solo avanza cuando el
        // puntero se movio un poco (un cuarto de punto del tamano), asi la direccion no tiembla con pasos minusculos y
        // con el puntero quieto se conserva la ultima.
        match self.last_dab_pos {
            Some(prev) => {
                let step = pick.pos - prev;
                if step.length() > 0.25 * WORLD_PER_RADIUS {
                    self.stroke_dir = step.normalize();
                    self.last_dab_pos = Some(pick.pos);
                }
            }
            None => self.last_dab_pos = Some(pick.pos),
        }
        let opts = DabOptions {
            alpha_lock: layer.alpha_lock,
            erase: self.tool == Tool::Eraser,
            brush: self.brush.kind,
            normal,
            // Interruptor "Acumular pasada" de Opciones: apagado = el trazo no pasa de la opacidad (como photocraft).
            ceiling: self.ui.pass_ceiling(),
            // Flujo del pincel: multiplica la cobertura de cada toque.
            flow: self.brush.flow,
            // El carboncillo orienta su punta a lo largo del trazo.
            dir: self.stroke_dir,
        };
        // Antes de tocar nada: copia de las baldosas que va a cambiar el toque (deshacer).
        piece.history.save_tiles(&layer.pixels, size, area);
        if let Some(r) = self.stroke.dab_surface(
            &mut layer.pixels,
            &tris,
            pick.pos,
            radius,
            self.brush_color,
            self.brush_opacity,
            opts,
        ) {
            piece.layers.composite_rect(r);
            *dirty = Some(match *dirty {
                Some(d) => (d.0.min(r.0), d.1.min(r.1), d.2.max(r.2), d.3.max(r.3)),
                None => r,
            });
        }
    }

    /// Sube a la GPU lo que cambio el trazo en curso (siempre en la pieza donde pinta). Devuelve true si habia algo.
    fn flush(&mut self, dirty: Option<Rect>) -> bool {
        match (dirty, self.stroke_piece) {
            (Some(r), Some(p)) => {
                self.upload_rect(p, r);
                true
            }
            _ => false,
        }
    }

    /// Relleno (seccion 8.6): un solo toque por apoyo, entra al historial de la pieza como un paso. Devuelve true si
    /// cambio algo.
    fn fill_at(&mut self, camera: &Camera, px: f32, py: f32) -> bool {
        let Some(pick) = self.pick_uv(camera, px, py) else { return false };
        self.activate_part(pick.part);
        let color = self.brush_color;
        let uvs = &self.uvs;
        let piece = &mut self.pieces[pick.piece];
        let sel = piece.layers.selected;
        if !piece.layers.layers[sel].paintable() {
            return false;
        }
        let size = piece.layers.size;
        let max = (size - 1) as f32;
        let x = ((pick.u - pick.u.floor()) * max).round() as u32;
        let y = ((pick.v - pick.v.floor()) * max).round() as u32;
        // Islas y cobertura UV de esta pieza (solo sus triangulos): se arman la primera vez.
        let (a, b) = (piece.first_vertex, piece.first_vertex + piece.vertex_count);
        let maps = piece.uv_maps.get_or_insert_with(|| UvMaps::build(&uvs[a * 2..b * 2], size));
        let layer = &mut piece.layers.layers[sel];
        let Some(plan) = fill::plan(&layer.pixels, size, x, y, color, layer.alpha_lock, maps) else {
            return false;
        };
        // Antes de tocar nada: copia de las baldosas que va a cambiar (deshacer).
        piece.history.begin(layer.id);
        for r in fill::touched_tiles(&plan, size) {
            piece.history.save_tiles(&layer.pixels, size, r);
        }
        fill::apply(&mut layer.pixels, &plan, size, color, layer.alpha_lock);
        piece.history.end();
        piece.layers.composite_rect(plan.rect);
        self.upload_rect(pick.piece, plan.rect);
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
        // La pieza donde pinta el trazo se decide con el primer toque que cae sobre el modelo (`enter_piece`).
        self.stroke_piece = None;
        self.stroke_dir = Vec3::ZERO;
        self.last_dab_pos = None;
        let mut dirty = None;
        if let Some(pick) = self.pick_uv(camera, px, py) {
            self.dab_surface(pick, &mut dirty);
        }
        self.flush(dirty)
    }

    /// true si con el boton apretado la herramienta sigue depositando pintura aunque el puntero este quieto: pincel o
    /// borrador con un preset que acumula (el aerografo) y el trazo en modo "techo" (con "Acumular pasada" encendido la
    /// regla es el maximo de los toques y repetir un toque en el mismo sitio no cambia nada).
    fn builds_up(&self) -> bool {
        matches!(self.tool, Tool::Brush | Tool::Eraser) && self.brush.kind.build_up() && self.ui.pass_ceiling()
    }

    /// Un toque en el sitio donde esta el puntero, sin moverlo (el temporizador del aerografo). Devuelve true si
    /// cambio algo.
    fn stroke_hold(&mut self, camera: &Camera, px: f32, py: f32) -> bool {
        let mut dirty = None;
        if let Some(pick) = self.pick_uv(camera, px, py) {
            self.dab_surface(pick, &mut dirty);
        }
        self.flush(dirty)
    }

    /// Soltar el boton: el trazo entra al historial de su pieza.
    fn stroke_end(&mut self) {
        if let Some(p) = self.stroke_piece.take() {
            self.pieces[p].history.end();
        }
    }

    /// Exporta un PNG a la carpeta actual (seccion 13). `kind`: 0 = textura con fondo, 1 = textura transparente,
    /// 2 = solo la capa elegida (sus pixeles tal cual, sin opacidad ni visibilidad). Exporta la pieza activa; con
    /// varias piezas el nombre del archivo lleva el de la malla. Devuelve el mensaje a mostrar.
    fn export_png(&self, kind: u8) -> String {
        let stack = &self.pieces[self.active()].layers;
        let size = stack.size;
        let (base, pixels) = match kind {
            0 => ("texturecraft_textura", stack.export_composite(true)),
            1 => ("texturecraft_textura_transparente", stack.export_composite(false)),
            _ => ("texturecraft_capa", stack.selected_layer().pixels.clone()),
        };
        let mut base = base.to_string();
        if self.pieces.len() > 1 {
            base.push('_');
            base.push_str(&slug(&self.parts[self.selected_part.min(self.parts.len() - 1)].name));
        }
        let path = export::free_path(&base);
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

    /// Deshacer (`redo = false`) o rehacer el ultimo trazo de la pieza activa. Devuelve true si cambio algo.
    fn undo_redo(&mut self, redo: bool) -> bool {
        let a = self.active();
        let piece = &mut self.pieces[a];
        let rect = if redo { piece.history.redo(&mut piece.layers) } else { piece.history.undo(&mut piece.layers) };
        match rect {
            Some(r) => {
                piece.layers.composite_rect(r);
                self.upload_rect(a, r);
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
        // Distancia entre toques en unidades del modelo (una fraccion del radio del pincel, como en la seccion 7.4).
        let spacing = (self.brush.radius * WORLD_PER_RADIUS * self.brush.kind.spacing()).max(0.5 * WORLD_PER_RADIUS);
        let ua = self.pick_uv(camera, from.0, from.1);
        let ub = self.pick_uv(camera, to.0, to.1);
        let mut dirty = None;
        self.stroke_segment(camera, from, ua, to, ub, spacing, 0, &mut dirty);
        self.flush(dirty)
    }

    /// Parte el tramo por la mitad hasta que los extremos quedan a `spacing` unidades del modelo o menos; solo se
    /// pinta el extremo final de cada trozo (el inicial ya se pinto). Si un extremo cae fuera del modelo o en otra
    /// pieza se mide en pantalla.
    #[allow(clippy::too_many_arguments)]
    fn stroke_segment(
        &mut self,
        camera: &Camera,
        a: (f32, f32),
        ua: Option<Pick>,
        b: (f32, f32),
        ub: Option<Pick>,
        spacing: f32,
        depth: u32,
        dirty: &mut Option<Rect>,
    ) {
        let screen = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        let close = match (ua, ub) {
            (Some(p), Some(q)) if p.piece == q.piece => p.pos.distance(q.pos) <= spacing,
            _ => screen <= 6.0,
        };
        if close || screen < 0.05 || depth >= 12 {
            if let Some(q) = ub {
                self.dab_surface(q, dirty);
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
    fn render(&mut self, camera: &Camera, mouse: (f32, f32), cursor_in_window: bool) -> (Duration, Option<ViewAxis>) {
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
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            // Solo las mallas visibles, cada una con la textura de su pieza.
            for (piece, range) in &self.draw_ranges {
                pass.set_bind_group(0, &self.pieces[*piece].bind_group, &[]);
                pass.draw(range.clone(), 0..1);
            }
        }
        // Cursor propio sobre el modelo: circulo del pincel (pincel y borrador) o la chincheta del cuentagotas.
        let ring = if cursor_in_window && matches!(self.tool, Tool::Brush | Tool::Eraser | Tool::Eyedropper) {
            let pin = self.tool == Tool::Eyedropper;
            // Chincheta: el color que hay bajo la punta (el que se tomaria al hacer clic), para mostrarlo dentro del aro.
            let color = if pin { self.color_at(camera, mouse.0, mouse.1) } else { None };
            let on_model = if pin {
                true
            } else {
                match self.brush_radius_px(camera, mouse) {
                    Some(r) => {
                        self.ring_radius = r.clamp(3.0, 300.0);
                        true
                    }
                    None => false,
                }
            };
            Some(BrushRing { pos: [mouse.0, mouse.1], radius: self.ring_radius, on_model, pin, color })
        } else {
            None
        };
        // Interfaz (egui) encima del modelo. El panel de capas muestra las de la pieza activa.
        let active = self.active();
        let UiOutput { full_output, changed: ui_changed, view_axis, layers_dirty, parts_dirty } = self.ui.run(
            &self.window,
            &mut self.tool,
            &mut self.brush_color,
            &mut self.brush_opacity,
            &mut self.brush,
            &mut self.pieces[active].layers,
            &mut self.parts,
            &mut self.selected_part,
            camera,
            &mut self.view_locked,
            ring,
        );
        // El panel de capas cambio algo que se ve (ojo, opacidad, modo, recorte, fondo): se recompone y se sube la
        // textura de esa pieza antes de enviar este cuadro.
        if layers_dirty {
            self.refresh_piece(active);
        }
        // El panel de mallas mostro u oculto alguna: cambian lo que se dibuja y lo que el pincel puede tocar.
        if parts_dirty {
            self.apply_visibility();
        }
        // El panel de mallas eligio otra pieza: el titulo y el panel de capas deben pasar a la nueva.
        let piece_changed = self.active() != active;
        if piece_changed {
            self.window.request_redraw();
        }
        // El boton Importar del panel de mallas ya tiene el archivo elegido: se reemplaza el modelo.
        if let Some(path) = self.ui.take_import() {
            self.open_obj(&path);
        }
        if ui_changed || piece_changed {
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

    /// Temporizador del aerografo: con el boton apretado deposita un toque cada `BUILD_UP_INTERVAL` aunque el puntero
    /// este quieto (como photocraft, 20 por segundo). Si el puntero se movio desde el ultimo toque, ese tramo ya
    /// pinto por el espaciado y este toque se salta. Se llama al final de cada vuelta del bucle, asi el plazo que se
    /// pide aqui (`WaitUntil`) gana al `Wait` que deja el redibujado.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        if !(self.left_down && gpu.builds_up()) {
            self.next_build_up = None;
            return;
        }
        let now = Instant::now();
        let due = *self.next_build_up.get_or_insert(now + BUILD_UP_INTERVAL);
        let next = if now >= due {
            if !std::mem::take(&mut self.moved_since_tick) && gpu.stroke_hold(&self.camera, self.mouse.0, self.mouse.1) {
                gpu.window.request_redraw();
            }
            now + BUILD_UP_INTERVAL
        } else {
            due
        };
        self.next_build_up = Some(next);
        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
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
        let mut model_name = String::from("Esfera de prueba");
        let mesh = match std::env::args().nth(1) {
            Some(arg) => match load_mesh(Path::new(&arg)) {
                Ok(m) => {
                    title = format!("texturecraft - {}", file_name(Path::new(&arg)));
                    model_name = file_name(Path::new(&arg));
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
        self.gpu = Some(Gpu::new(event_loop, window, &mesh, &model_name));
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
            WindowEvent::DroppedFile(path) => gpu.open_obj(&path),
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
                            self.moved_since_tick = false;
                            self.next_build_up = None;
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
                self.cursor_in_window = true;
                if self.left_down && gpu.tool == Tool::Hand {
                    // Mano: el boton izquierdo mueve la vista en vez de pintar (si la vista esta bloqueada, nada).
                    if !gpu.view_locked {
                        self.camera.pan_by(dx, dy);
                        gpu.window.request_redraw();
                    }
                } else if self.left_down {
                    let from = self.last_paint.unwrap_or((x, y));
                    self.last_paint = Some((x, y));
                    if dx != 0.0 || dy != 0.0 {
                        self.moved_since_tick = true;
                    }
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
                // El cursor propio (circulo del pincel o chincheta del cuentagotas) sigue al raton.
                if matches!(gpu.tool, Tool::Brush | Tool::Eraser | Tool::Eyedropper) {
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::CursorEntered { .. } => {
                self.cursor_in_window = true;
                gpu.window.request_redraw();
            }
            WindowEvent::CursorLeft { .. } => {
                // Al salir de la ventana el circulo desaparece.
                self.cursor_in_window = false;
                gpu.window.request_redraw();
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
                    // Teclas de prueba de las capas (hasta tener el panel); actuan sobre la pieza activa.
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
                        let sel = gpu.layers_mut().selected;
                        let (mut recompose, mut retitle) = (false, false);
                        match c.to_lowercase().as_str() {
                            "n" => retitle = gpu.layers_mut().add_layer(),
                            "]" => retitle = gpu.layers_mut().select_step(1),
                            "[" => retitle = gpu.layers_mut().select_step(-1),
                            "b" => {
                                let l = &mut gpu.layers_mut().layers[sel];
                                let n = BlendMode::ALL.len();
                                let i = BlendMode::ALL.iter().position(|&m| m == l.blend).unwrap_or(0);
                                l.blend = BlendMode::ALL[if shift { (i + n - 1) % n } else { (i + 1) % n }];
                                recompose = true;
                            }
                            "v" => {
                                let l = &mut gpu.layers_mut().layers[sel];
                                l.visible = !l.visible;
                                recompose = true;
                            }
                            "c" => {
                                let l = &mut gpu.layers_mut().layers[sel];
                                l.clip_to_below = !l.clip_to_below;
                                recompose = true;
                            }
                            "l" => {
                                let l = &mut gpu.layers_mut().layers[sel];
                                l.locked = !l.locked;
                                retitle = true;
                            }
                            "a" => {
                                let l = &mut gpu.layers_mut().layers[sel];
                                l.alpha_lock = !l.alpha_lock;
                                retitle = true;
                            }
                            "-" => {
                                let l = &mut gpu.layers_mut().layers[sel];
                                l.opacity = l.opacity.saturating_sub(10);
                                recompose = true;
                            }
                            "+" | "=" => {
                                let l = &mut gpu.layers_mut().layers[sel];
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
                let (delay, view_axis) = gpu.render(&self.camera, self.mouse, self.cursor_in_window);
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
        cursor_in_window: false,
        next_build_up: None,
        moved_since_tick: false,
    };
    event_loop.run_app(&mut app).expect("error en el bucle de eventos");
}
