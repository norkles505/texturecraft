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
//! Herramientas (boton izquierdo): P = pincel, G = borrador (goma), F = relleno (balde), R = degradado (arrastrar de un punto a otro del modelo), I = cuentagotas (toma el color
//! de lo que ves), D = difuminar (arrastra el color del trazo), U = borrosidad (mezcla cada punto con sus vecinos), H = mano (el boton izquierdo mueve la vista en vez de pintar).
//! Exportar PNG a la carpeta actual: E = textura con fondo, Mayus+E = transparente, X = solo la capa elegida.

mod blend;
mod brush;
mod bvh;
mod camera;
mod export;
mod face_edit;
mod fill;
mod layers;
mod obj;
mod paint;
mod pinceles;
mod proyecto;
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
use export::{EnginePreset, SaveKind};
use face_edit::FaceEdit;
use glam::Vec3;
use layers::{ChannelKind, LayerOp, LayerStack, NORMAL_STRENGTH, PaintTarget};
use obj::{
    MeshPart, ObjMesh, load_obj, part_of_triangle, piece_ranges, triangle_visibility,
    visible_ranges,
};
use paint::{
    DabOptions, FilterMode, GradientSpec, Rect, StrokeState, SurfTri, TEX_SIZE, surface_rect,
};
use ui::{BrushRing, UiLayer, UiOutput};
use undo::History;
use uvmap::UvMaps;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Colores de prueba para distinguir capas (tecla K).
const PALETTE: [[u8; 3]; 6] = [
    [225, 70, 60],
    [60, 120, 230],
    [70, 190, 90],
    [240, 200, 50],
    [20, 20, 20],
    [255, 255, 255],
];
const CLEAR: wgpu::Color = wgpu::Color {
    r: 0.16,
    g: 0.16,
    b: 0.18,
    a: 1.0,
};

/// Herramienta activa con el boton izquierdo (seccion 8).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Brush,
    Eraser,
    Fill,
    /// Degradado: arrastrar de un punto del modelo a otro pinta del color del pincel a transparente (en una mascara,
    /// del gris del pincel a su opuesto).
    Gradient,
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
            Tool::Gradient => "degradado",
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
    /// x = 1.0 con la vista de material (el shader usa rugosidad, metalico y altura); 0.0 = la luz de siempre.
    params: [f32; 4],
}

/// Una pieza del modelo (seccion 3.3): su propio tramo de vertices, su textura en la GPU, sus capas y su historial.
/// Un modelo con 2 a 12 partes tiene una pieza por parte; si no, una sola pieza que cubre todo.
struct Piece {
    /// Tramo de vertices del modelo que cubre esta pieza (multiplos de 3).
    first_vertex: usize,
    vertex_count: usize,
    texture: wgpu::Texture,
    /// Textura de material: R = rugosidad, G = metalico, B = altura. Solo se mantiene al dia con la vista de material.
    material: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    layers: LayerStack,
    history: History,
    /// Ajustes de las capas en el cuadro anterior (para registrar sus cambios en el historial).
    settings_seen: Vec<layers::LayerSettings>,
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

/// Desde que distancia (hacia atras del punto espejo) parte el rayo del toque espejo: mas que el tamano del modelo,
/// para que el rayo empiece siempre fuera de el.
const MIRROR_BACKOFF: f32 = 4.0 * FIT_SIZE;

/// Medidor de rendimiento (solo escribe algo con la variable de entorno `TEXTURECRAFT_PERF`): cuantos toques se
/// pintaron y cuantos nanosegundos se fueron en pintar y en recomponer las capas. `stroke_move` los lee antes y
/// despues de cada movimiento del raton y escribe la diferencia por la salida de errores.
static PERF_DABS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static PERF_PAINT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PERF_COMP_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn perf_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("TEXTURECRAFT_PERF").is_some())
}

/// `v` reflejado en los planos X, Y y Z marcados en `flips` (el modelo esta centrado en el origen, asi que los planos
/// de espejo pasan por el).
fn mirror_vec(v: Vec3, flips: [bool; 3]) -> Vec3 {
    Vec3::new(
        if flips[0] { -v.x } else { v.x },
        if flips[1] { -v.y } else { v.y },
        if flips[2] { -v.z } else { v.z },
    )
}

/// Crea una pieza: textura propia (con el fondo ya compuesto) y el grupo de enlaces que la une a los uniforms.
fn new_piece(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    (first_vertex, vertex_count): (usize, usize),
    size: u32,
) -> Piece {
    let layers = LayerStack::new(size);
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("tablero"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
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
    // Material sin pintar: rugosidad y altura a la mitad, nada de metal (igual que `LayerStack::material_bytes`).
    let flat = [128u8, 0, 128, 255].repeat((size * size) as usize);
    let material = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some("material"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &flat,
    );
    let mat_view = material.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&tex_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&mat_view),
            },
        ],
    });
    Piece {
        first_vertex,
        vertex_count,
        texture,
        material,
        bind_group,
        settings_seen: layers.settings_snapshot(),
        layers,
        history: History::default(),
        uv_maps: None,
    }
}

/// Llamadas de dibujo `(pieza, tramo de vertices)`: solo las mallas visibles. Con una pieza por parte cada parte se
/// dibuja con la textura de su pieza; con una sola pieza las partes visibles seguidas se juntan en una llamada.
fn draw_calls(piece_count: usize, parts: &[MeshPart]) -> Vec<(usize, std::ops::Range<u32>)> {
    if piece_count == parts.len() {
        parts
            .iter()
            .enumerate()
            .filter(|(_, p)| p.visible)
            .map(|(i, p)| {
                (
                    i,
                    p.first_vertex as u32..(p.first_vertex + p.vertex_count) as u32,
                )
            })
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
    /// Una bandera (f32) por vertice para el shader: 0 normal, 1 cara elegida, 2 cara oculta (modo edicion de caras).
    flag_buffer: wgpu::Buffer,
    /// Modo edicion de caras (Tab): caras elegidas y ocultas.
    faces: FaceEdit,
    /// Dibujo de las aristas de las caras en el modo edicion: lineas negras (naranjas las de caras elegidas).
    line_pipeline: wgpu::RenderPipeline,
    /// Vertices de las aristas (`x y z flag`, 2 por arista); `None` fuera del modo o si no hay aristas que dibujar.
    edge_buffer: Option<wgpu::Buffer>,
    edge_vertex_count: u32,
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
    /// Pinceles con desvanecido (`BrushType::fades`): los toques del trazo en curso, para repintarlo al soltar con el
    /// desvanecido del final.
    stroke_dabs: Vec<Pick>,
    /// false si el trazo paso por otra pieza (no se repinta: cada pieza lleva su propio registro y estado de trazo).
    stroke_fade_ok: bool,
    /// Degradado en curso: el punto del modelo donde se apoyo el boton (el inicio del degradado). `None` si no hay.
    gradient_from: Option<Pick>,
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
    /// Proyecto en curso: su carpeta en `proyecto::projects_dir()` (`None` = aun no se guardo; la primera vez que se
    /// pulsa "Regresar a mi galería" se le pone un identificador y las siguientes sobrescriben la misma carpeta).
    project_id: Option<String>,
    /// Nombre del proyecto (el del modelo, sin la extension).
    project_name: String,
    /// Ruta del .obj abierto (`None` = la esfera de prueba), para guardarla en el proyecto.
    model_path: Option<std::path::PathBuf>,
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
    /// Donde termino el ultimo trazo de pincel o borrador, con la vista de ese momento (`view_key`): Mayus+clic traza
    /// una recta desde ahi hasta el clic. Solo vale mientras la vista no cambie (si se gira, mueve o acerca, ese punto de
    /// pantalla ya no es el mismo sitio del modelo).
    line_anchor: Option<((f32, f32), [f32; 8])>,
    modifiers: ModifiersState,
    /// El cursor esta dentro de la ventana (para mostrar u ocultar el circulo del pincel).
    cursor_in_window: bool,
    /// Proximo toque del temporizador del aerografo (`None` = no hay trazo que lo necesite).
    next_build_up: Option<Instant>,
    /// El puntero se movio desde el ultimo toque del temporizador: ese tramo ya deposito pintura al moverse, asi que
    /// el temporizador se salta ese toque (solo deposita cuando el puntero esta quieto).
    moved_since_tick: bool,
}

/// Buffer con una bandera (f32) por vertice para el modo edicion de caras; empieza todo en 0 (normal).
fn new_flag_buffer(device: &wgpu::Device, vertex_count: usize) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("banderas"),
        contents: bytemuck::cast_slice(&vec![0.0f32; vertex_count]),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("profundidad"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
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

/// Cada cuantos puntos de pantalla se mide el recorrido del raton para colocar los toques (mas fino = toques mas
/// exactos sobre curvas y bordes del modelo).
const WALK_STEP_PX: f32 = 2.0;

/// Reparte un tramo de `dist` unidades en toques separados exactamente `spacing` (como `next_at` de photocraft).
/// `to_next` es cuanto falta recorrer hasta el proximo toque: se actualiza, asi el sobrante pasa de un tramo al
/// siguiente y los toques quedan igual de separados sin importar cuantos eventos mande el raton (trazo rapido o
/// lento). Devuelve, para cada toque que cae en este tramo, la fraccion (0..=1) del tramo donde queda.
fn dab_points(to_next: &mut f32, dist: f32, spacing: f32) -> Vec<f32> {
    let mut points = Vec::new();
    if dist <= 0.0 || spacing <= 0.0 {
        return points;
    }
    let mut walked = 0.0;
    while dist - walked >= *to_next {
        walked += *to_next;
        points.push((walked / dist).min(1.0));
        *to_next = spacing;
    }
    *to_next -= dist - walked;
    points
}

#[cfg(test)]
mod dab_points_tests {
    use super::dab_points;

    /// Posiciones (a lo largo de un camino de `total` unidades) de los toques, recorriendo el camino en tramos de `chunk`.
    fn posiciones(total: f32, chunk: f32, spacing: f32) -> Vec<f32> {
        let (mut to_next, mut at, mut out) = (spacing, 0.0f32, Vec::new());
        while at < total - 1e-6 {
            let d = chunk.min(total - at);
            for t in dab_points(&mut to_next, d, spacing) {
                out.push(at + d * t);
            }
            at += d;
        }
        out
    }

    #[test]
    fn los_toques_quedan_igual_de_separados_con_pasos_chicos_o_grandes() {
        let (total, spacing) = (10.0, 0.7);
        let lento = posiciones(total, 0.05, spacing);
        let rapido = posiciones(total, 3.3, spacing);
        // 10 / 0.7 = 14 toques (el ultimo en 9.8), y los mismos en los dos casos.
        assert_eq!(lento.len(), 14);
        assert_eq!(rapido.len(), lento.len());
        for (i, (a, b)) in lento.iter().zip(&rapido).enumerate() {
            let esperado = 0.7 * (i + 1) as f32;
            assert!(
                (a - esperado).abs() < 1e-3 && (b - esperado).abs() < 1e-3,
                "toque {i}: {a} y {b}, esperado {esperado}"
            );
        }
    }

    #[test]
    fn el_sobrante_pasa_de_un_tramo_al_siguiente() {
        let mut to_next = 1.0;
        // Tres pasos de 0.4 no completan 1.0 hasta el tercero.
        assert!(dab_points(&mut to_next, 0.4, 1.0).is_empty());
        assert!(dab_points(&mut to_next, 0.4, 1.0).is_empty());
        let p = dab_points(&mut to_next, 0.4, 1.0);
        assert_eq!(p.len(), 1);
        assert!((p[0] - 0.5).abs() < 1e-5, "{p:?}");
        // Un tramo sin longitud no cambia nada.
        let antes = to_next;
        assert!(dab_points(&mut to_next, 0.0, 1.0).is_empty());
        assert_eq!(to_next, antes);
    }
}

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
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer {}: {e}", path.display()))?;
    let mesh = load_obj(&text, FIT_SIZE, true).map_err(|e| format!("{}: {e}", path.display()))?;
    if !mesh.has_uvs {
        eprintln!(
            "Aviso: {} no trae UVs, no se podra pintar bien.",
            path.display()
        );
    }
    Ok(mesh)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Nombre apto para un archivo: lo que no es letra o numero pasa a `_`.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// Luminosidad (0..=255) de un color: el gris con que se pinta en una mascara (blanco = se ve, negro = se oculta).
fn luma(c: [u8; 3]) -> u8 {
    ((77 * c[0] as u32 + 150 * c[1] as u32 + 29 * c[2] as u32) >> 8) as u8
}

#[cfg(test)]
mod luma_tests {
    use super::luma;

    #[test]
    fn blanco_negro_y_gris() {
        assert_eq!(luma([255, 255, 255]), 255);
        assert_eq!(luma([0, 0, 0]), 0);
        assert_eq!(luma([128, 128, 128]), 128);
        // El verde pesa mas que el azul.
        assert!(luma([0, 255, 0]) > luma([0, 0, 255]));
    }
}

impl Gpu {
    fn new(
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        mesh: &ObjMesh,
        model_name: &str,
    ) -> Gpu {
        // winit recomienda pasar su handle de pantalla al crear la instancia (Wayland/X11/GL lo necesitan).
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));
        let surface = instance
            .create_surface(window.clone())
            .expect("no se pudo crear la superficie");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("no hay GPU compatible");
        log::info!("GPU: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
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
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
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
        // Segundo buffer: una bandera por vertice (modo edicion de caras).
        let flag_attrs = wgpu::vertex_attr_array![3 => Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("modelo"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 32,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: 4,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &flag_attrs,
                    }),
                ],
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
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                // Las superficies se empujan un poco hacia atras en la profundidad (mas cuanto mas de canto se ven):
                // asi las aristas del modo edicion, que van sobre la superficie, ganan a la superficie sin tener que
                // acercarlas a la camara (con eso se colaban las aristas de la parte de atras en los bordes).
                bias: wgpu::DepthBiasState {
                    constant: 4,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Aristas del modo edicion: lineas con un solo buffer de posicion (x y z flag). Se dibujan en el mismo pase que el
        // modelo, sin escribir profundidad (para no taparse entre ellas) y con la misma prueba que el modelo.
        let line_attrs = wgpu::vertex_attr_array![0 => Float32x4];
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("aristas"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &line_attrs,
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
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
        let flag_buffer = new_flag_buffer(&device, mesh.vertex_count());

        let ui = UiLayer::new(&window, &device, config.format.remove_srgb_suffix());
        let parts = mesh.scene_parts(model_name);
        // Una pieza por parte (2 a 12 partes) o una sola, cada una con su textura, capas e historial.
        let pieces: Vec<Piece> = piece_ranges(&parts)
            .into_iter()
            .map(|r| {
                new_piece(
                    &device,
                    &queue,
                    &bind_group_layout,
                    &uniform_buffer,
                    &sampler,
                    r,
                    TEX_SIZE,
                )
            })
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
            flag_buffer,
            faces: FaceEdit::new(mesh),
            line_pipeline,
            edge_buffer: None,
            edge_vertex_count: 0,
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
            stroke_dabs: Vec::new(),
            stroke_fade_ok: true,
            gradient_from: None,
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
            project_id: None,
            project_name: Path::new(model_name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| model_name.to_string()),
            model_path: None,
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
        if self.pieces.len() == self.parts.len() {
            part.min(self.pieces.len() - 1)
        } else {
            0
        }
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
    /// nuevas empiezan con una textura y un historial vacios (el historial era de otro modelo). `size` es el lado de la
    /// textura de cada pieza: el estado del trazo se rehace con ese lado.
    fn set_mesh(&mut self, mesh: &ObjMesh, model_name: &str, size: u32) {
        self.stroke = StrokeState::new(size);
        self.vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&mesh.interleaved()),
                usage: wgpu::BufferUsages::VERTEX,
            });
        self.bvh = TriBvh::new(mesh);
        self.uvs = mesh.uvs.clone();
        // Modelo nuevo: sus caras empiezan sin elegir ni ocultar, fuera del modo edicion.
        self.flag_buffer = new_flag_buffer(&self.device, mesh.vertex_count());
        self.faces = FaceEdit::new(mesh);
        self.parts = mesh.scene_parts(model_name);
        self.pieces = piece_ranges(&self.parts)
            .into_iter()
            .map(|r| {
                new_piece(
                    &self.device,
                    &self.queue,
                    &self.bind_group_layout,
                    &self.uniform_buffer,
                    &self.sampler,
                    r,
                    size,
                )
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
        // Un triangulo se puede pintar si su malla es visible Y su cara no esta oculta en el modo edicion.
        let mut vis = triangle_visibility(&self.parts, tris);
        for (t, v) in vis.iter_mut().enumerate() {
            *v = *v && self.faces.tri_visible(t);
        }
        self.rebuild_edges(&vis);
        self.bvh.set_visible(vis);
        self.upload_face_flags();
        self.window.request_redraw();
    }

    /// Rearma las aristas que se dibujan en el modo edicion (`vis` = triangulos que se ven). Fuera del modo no hay
    /// ninguna. Se llama al entrar al modo y cada vez que cambia la seleccion o lo oculto.
    fn rebuild_edges(&mut self, vis: &[bool]) {
        self.edge_buffer = None;
        self.edge_vertex_count = 0;
        if !self.faces.active {
            return;
        }
        let bvh = &self.bvh;
        let lines = self.faces.edge_lines(
            |t| {
                let (p, _) = bvh.tri(t);
                [p[0].to_array(), p[1].to_array(), p[2].to_array()]
            },
            |t| vis.get(t).copied().unwrap_or(false),
        );
        if lines.is_empty() {
            return;
        }
        self.edge_vertex_count = (lines.len() / 4) as u32;
        self.edge_buffer = Some(self.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("aristas"),
                contents: bytemuck::cast_slice(&lines),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));
    }

    /// Sube a la GPU la bandera de cada vertice (cara elegida u oculta) tras cambiar la seleccion o el modo.
    fn upload_face_flags(&self) {
        self.queue.write_buffer(
            &self.flag_buffer,
            0,
            bytemuck::cast_slice(&self.faces.vertex_flags()),
        );
    }

    /// Tab: entra o sale del modo edicion de caras. Las caras ocultas siguen ocultas al salir.
    fn toggle_face_edit(&mut self) {
        self.faces.active = !self.faces.active;
        // Entrar o salir cambia las banderas y las aristas dibujadas (`apply_visibility` rehace ambas).
        self.apply_visibility();
        self.window.set_title(&self.status());
        self.window.request_redraw();
    }

    /// Cambio de la seleccion o de lo oculto en el modo edicion: recalcula lo que se ve y se puede pintar.
    fn faces_changed(&mut self) {
        self.apply_visibility();
        self.window.set_title(&self.status());
    }

    /// Abre un .obj (arrastrado a la ventana o elegido en el panel de mallas o con el "+" de la galeria) y reemplaza el
    /// modelo actual. Es un proyecto nuevo: todavia no tiene carpeta guardada.
    fn open_obj(&mut self, path: &Path) {
        match load_mesh(path) {
            Ok(mesh) => {
                // Proyecto nuevo: lleva el tamano de textura que se eligio en la galeria.
                let size = self.ui.new_texture_size();
                self.set_mesh(&mesh, &file_name(path), size);
                self.project_id = None;
                self.project_name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| file_name(path));
                self.model_path = Some(path.to_path_buf());
                self.window
                    .set_title(&format!("texturecraft - {}", file_name(path)));
                self.window.request_redraw();
            }
            Err(e) => {
                eprintln!("{e}");
                self.window
                    .set_title("texturecraft - no se pudo abrir el archivo (mira la terminal)");
            }
        }
    }

    /// Abre un proyecto guardado (el que se pulso en la galeria): carga su modelo, repone las capas de cada pieza y
    /// deja el id para que "Regresar a mi galería" sobrescriba esa misma carpeta. Si algo falla no cambia nada.
    fn open_project(&mut self, id: &str) -> Result<(), String> {
        let proyecto::Loaded {
            name,
            model,
            active,
            size,
            stacks,
        } = proyecto::load(id, TEX_SIZE)?;
        let (mesh, model_name) = match &model {
            Some(path) => (load_mesh(path)?, file_name(path)),
            None => (test_sphere(), String::from("Esfera de prueba")),
        };
        // El modelo debe seguir teniendo las mismas piezas que cuando se guardo (si el .obj cambio, no encajan).
        let needed = piece_ranges(&mesh.scene_parts(&model_name)).len();
        if stacks.len() != needed {
            return Err(format!(
                "El proyecto tiene {} piezas y el modelo ahora tiene {needed}",
                stacks.len()
            ));
        }
        self.set_mesh(&mesh, &model_name, size);
        for (i, stack) in stacks.into_iter().enumerate() {
            let m = stack.size - 1;
            self.pieces[i].layers = stack;
            self.upload_rect(i, (0, 0, m, m));
        }
        // Con una pieza por parte, la pieza activa es la malla elegida; con una sola pieza no hay nada que elegir.
        if self.pieces.len() == self.parts.len() {
            self.selected_part = active.min(self.parts.len() - 1);
        }
        self.project_id = Some(id.to_string());
        self.project_name = name;
        self.model_path = model;
        self.window.set_title(&self.status());
        self.window.request_redraw();
        Ok(())
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
            if let Some(b) = self
                .pick_uv(camera, x + dx, y + dy)
                .filter(|b| b.piece == a.piece)
            {
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
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        // Con la vista de material tambien se sube el material (rugosidad, metalico, altura) de ese rectangulo.
        if p.layers.material_view {
            let mat = p.layers.material_bytes(rect);
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &p.material,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: x0, y: y0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &mat,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
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
        // El canal que se pinta y la vista de material son los mismos en todas las piezas: si cambian en la activa,
        // se copian a las demas.
        let channel = self.pieces[a].layers.channel;
        let material_view = self.pieces[a].layers.material_view;
        for p in 0..self.pieces.len() {
            let l = &self.pieces[p].layers;
            if p != a && (l.channel != channel || l.material_view != material_view) {
                let l = &mut self.pieces[p].layers;
                l.channel = channel;
                l.material_view = material_view;
                self.refresh_piece(p);
            }
        }
        self.refresh_piece(a);
    }

    /// Texto de estado (en la barra de titulo, mientras no haya panel): herramienta, color y capa elegida (de la
    /// pieza activa; con varias piezas, tambien su nombre).
    fn status(&self) -> String {
        if self.faces.active {
            return format!("texturecraft - {}", self.faces.status());
        }
        let a = self.active();
        let stack = &self.pieces[a].layers;
        let l = stack.selected_layer();
        let c = self.brush_color;
        let piece = if self.pieces.len() > 1 {
            format!(
                " | pieza \"{}\"",
                self.parts[self.selected_part.min(self.parts.len() - 1)].name
            )
        } else {
            String::new()
        };
        format!(
            "texturecraft - {} #{:02X}{:02X}{:02X}{} | capa {}/{} \"{}\" | {} | {}%{}{}{}{}{}{}{}",
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
            if l.is_solid() {
                " | capa de color"
            } else if l.is_group() && !l.painting_mask() {
                " | carpeta: elige una capa de dentro para pintar"
            } else {
                ""
            },
            if l.painting_mask() { " | pintando la mascara" } else { "" },
            if stack.solo { " | MODO SOLO (solo cambia lo que ves)" } else { "" },
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
        let Some(c) = self.color_at(camera, px, py) else {
            return;
        };
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
        let layer = piece.layers.selected_layer();
        let (id, target) = (layer.id, layer.target(piece.layers.channel));
        piece.history.begin(id, target);
    }

    /// Un toque en `pick` y, con la simetria encendida (panel de reglas), sus espejos: una copia del toque por cada
    /// combinacion de los planos X, Y y Z encendidos. Difuminar no se refleja (arrastra desde el toque anterior y los
    /// saltos de un lado a otro lo romperian) y un espejo que cae en OTRA pieza tampoco (un trazo es de una sola pieza).
    fn dab_surface_mult(&mut self, pick: Pick, mult: f32, dirty: &mut Option<Rect>) {
        self.dab_surface_one(pick, mult, dirty, [false; 3]);
        let sym = self.ui.symmetry();
        if !sym.iter().any(|s| *s) || self.tool == Tool::Smudge {
            return;
        }
        // Un espejo que cae casi sobre un toque ya pintado (el toque esta sobre un plano de simetria, o junto a el)
        // no cambiaria nada: se salta. Con tres planos encendidos ahorra muchas copias en la linea central.
        let cerca = 0.05 * self.brush.radius * WORLD_PER_RADIUS;
        let mut pintados = vec![pick.pos];
        for bits in 1u8..8 {
            let flips = [bits & 1 != 0, bits & 2 != 0, bits & 4 != 0];
            if (0..3).any(|k| flips[k] && !sym[k]) {
                continue;
            }
            let Some(m) = self.mirror_pick(pick, flips) else {
                continue;
            };
            if self.stroke_piece != Some(m.piece) {
                continue;
            }
            if pintados.iter().any(|q| q.distance(m.pos) < cerca) {
                continue;
            }
            pintados.push(m.pos);
            self.dab_surface_one(m, mult, dirty, flips);
        }
    }

    /// El punto espejo de `pick` sobre el modelo: se refleja el rayo que toco (su punto y su direccion) y se lanza
    /// contra el modelo; en un modelo simetrico cae justo en el punto simetrico. `None` si el rayo no toca nada.
    fn mirror_pick(&self, pick: Pick, flips: [bool; 3]) -> Option<Pick> {
        let dir = mirror_vec(pick.view, flips);
        let origin = mirror_vec(pick.pos, flips) - dir * MIRROR_BACKOFF;
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

    /// Un toque en el punto de `pick` sobre la capa elegida de su pieza; recompone el rectangulo y lo suma a
    /// `dirty` (que es siempre de la pieza del trazo en curso: al cruzar a otra pieza se sube primero el anterior).
    /// El toque mide en el modelo (ver `paint.rs`): se buscan los triangulos de la pieza que estan cerca del punto
    /// tocado y miran a la camara, y cada texel se mide por su distancia 3D.
    /// `mirror` dice en que planos esta reflejado este toque (todo `false` = el toque original): un espejo no mueve
    /// el punto de referencia de la direccion del trazo y usa esa direccion reflejada.
    fn dab_surface_one(
        &mut self,
        pick: Pick,
        mult: f32,
        dirty: &mut Option<Rect>,
        mirror: [bool; 3],
    ) {
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
        let channel = piece.layers.channel;
        let layer = &mut piece.layers.layers[sel];
        if !layer.accepts(channel) {
            return;
        }
        let radius = self.brush.radius * WORLD_PER_RADIUS;
        // Triangulos cercanos que son de esta pieza (cada pieza tiene su textura). El pastel dispersa su toque de lado:
        // se busca con ese margen de mas.
        let mut near = Vec::new();
        bvh.query_sphere(
            pick.pos,
            radius * (1.0 + self.brush.kind.scatter()),
            &mut near,
        );
        let (first_tri, end_tri) = (
            piece.first_vertex / 3,
            (piece.first_vertex + piece.vertex_count) / 3,
        );
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
        let Some(area) = surface_rect(size, &tris) else {
            return;
        };
        // Normal del triangulo tocado: la tiza la usa para orientar su punta sobre la superficie (el signo da igual).
        let normal = {
            let (p, _) = bvh.tri(pick.tri);
            (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero()
        };
        // Direccion del trazo: del ultimo punto de referencia a este. El punto de referencia solo avanza cuando el
        // puntero se movio un poco (un cuarto de punto del tamano), asi la direccion no tiembla con pasos minusculos y
        // con el puntero quieto se conserva la ultima.
        if mirror == [false; 3] {
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
        }
        // En la mascara se pinta en grises: el color del pincel pasa a su luminosidad y el borrador pinta negro (oculta
        // la capa). Ahi no hay bloqueo alfa ni borrado: la mascara siempre es opaca.
        // En un canal (rugosidad, metalico, altura) tambien se pinta en grises, pero ahi si hay bloqueo alfa y borrador:
        // la hoja del canal guarda el alfa de donde la capa pinta ese canal.
        let target = layer.target(channel);
        let on_mask = matches!(target, PaintTarget::Mask(_));
        let alpha_lock = layer.alpha_lock && !on_mask;
        let (color, erase) = if on_mask {
            // En la mascara de una capa de color liso el pincel siempre "pinta el color" (blanco = se ve) y el
            // borrador lo quita; en una mascara normal el gris sale del color del pincel.
            let g = if self.tool == Tool::Eraser {
                0
            } else if layer.mask_reveals() {
                255
            } else {
                luma(self.brush_color)
            };
            ([g, g, g], false)
        } else if target != PaintTarget::Layer {
            let g = luma(self.brush_color);
            ([g, g, g], self.tool == Tool::Eraser)
        } else {
            (self.brush_color, self.tool == Tool::Eraser)
        };
        let opts = DabOptions {
            alpha_lock,
            erase,
            brush: self.brush.kind,
            normal,
            // Interruptor "Acumular pasada" de Opciones: apagado = el trazo no pasa de la opacidad (como photocraft).
            ceiling: self.ui.pass_ceiling(),
            // Flujo del pincel: multiplica la cobertura de cada toque.
            flow: self.brush.flow,
            // El carboncillo orienta su punta a lo largo del trazo (reflejada en un toque espejo).
            dir: mirror_vec(self.stroke_dir, mirror),
        };
        // Antes de tocar nada: copia de las baldosas que va a cambiar el toque (deshacer). Los pixeles son los de la
        // mascara si se pinta en ella, y si no los de la capa.
        let pixels = layer.target_pixels_mut(target);
        piece.history.save_tiles(pixels, size, area);
        // Difuminar y Borrosidad no pintan color: cambian lo que ya hay bajo el pincel (`StrokeState::dab_filter`).
        // Entran al historial igual que un trazo (arriba se guardaron las baldosas), asi Ctrl+Z los deshace.
        let filter = match self.tool {
            Tool::Smudge => Some(FilterMode::Smudge),
            Tool::Blur => Some(FilterMode::Blur),
            _ => None,
        };
        PERF_DABS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let perf_paint = std::time::Instant::now();
        let touched = match filter {
            Some(mode) => self.stroke.dab_filter(
                pixels,
                &tris,
                pick.pos,
                radius,
                mode,
                self.brush_opacity * mult * self.brush.flow,
                alpha_lock,
            ),
            None => self.stroke.dab_surface(
                pixels,
                &tris,
                pick.pos,
                radius,
                color,
                self.brush_opacity * mult,
                opts,
            ),
        };
        PERF_PAINT_NS.fetch_add(
            perf_paint.elapsed().as_nanos() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        if let Some(r) = touched {
            let perf_comp = std::time::Instant::now();
            piece.layers.composite_rect(r);
            PERF_COMP_NS.fetch_add(
                perf_comp.elapsed().as_nanos() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            *dirty = Some(match *dirty {
                Some(d) => (d.0.min(r.0), d.1.min(r.1), d.2.max(r.2), d.3.max(r.3)),
                None => r,
            });
        }
    }

    /// Un toque del trazo en curso (`dab_surface_mult` con opacidad plena). Si el pincel tiene desvanecido
    /// (`BrushType::fades`) guarda el toque y lo pinta con el desvanecido de INICIO; el de final se aplica al soltar el
    /// boton, repintando el trazo (`replay_fade`).
    fn dab_surface(&mut self, pick: Pick, dirty: &mut Option<Rect>) {
        if !self.brush.kind.fades() {
            self.dab_surface_mult(pick, 1.0, dirty);
            return;
        }
        if self.stroke_piece.is_some_and(|p| p != pick.piece) {
            self.stroke_fade_ok = false;
        }
        let paso = self.fade_step();
        let mult = crate::pinceles::desvanecido_entrada(self.stroke_dabs.len() as f32 * paso);
        self.stroke_dabs.push(pick);
        self.dab_surface_mult(pick, mult, dirty);
    }

    /// Espaciado entre toques en radios del pincel (para medir lo recorrido en el desvanecido).
    fn fade_step(&self) -> f32 {
        self.dab_spacing() / (self.brush.radius * WORLD_PER_RADIUS).max(1e-6)
    }

    /// Al soltar el boton con un pincel de desvanecido: devuelve la capa a como estaba antes del trazo (con el registro
    /// de deshacer) y vuelve a pintar todos sus toques, ahora con el desvanecido de inicio Y de final.
    fn replay_fade(&mut self) {
        let Some(p) = self.stroke_piece else { return };
        let dabs = std::mem::take(&mut self.stroke_dabs);
        let n = dabs.len();
        if n < 2 {
            return;
        }
        let restored = {
            let piece = &mut self.pieces[p];
            let sel = piece.layers.selected;
            let size = piece.layers.size;
            let channel = piece.layers.channel;
            let target = piece.layers.layers[sel].target(channel);
            let pixels = piece.layers.layers[sel].target_pixels_mut(target);
            let r = piece.history.restore_pending(pixels, size);
            if let Some(r) = r {
                piece.layers.composite_rect(r);
            }
            r
        };
        let Some(restored) = restored else { return };
        self.stroke.begin_stroke();
        self.stroke_dir = Vec3::ZERO;
        self.last_dab_pos = None;
        let paso = self.fade_step();
        let mut dirty = Some(restored);
        for (k, pick) in dabs.iter().enumerate() {
            let mult = crate::pinceles::desvanecido_toque(k, n, paso);
            self.dab_surface_mult(*pick, mult, &mut dirty);
        }
        self.flush(dirty);
        self.window.request_redraw();
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
        let Some(pick) = self.pick_uv(camera, px, py) else {
            return false;
        };
        self.activate_part(pick.part);
        let color = self.brush_color;
        let uvs = &self.uvs;
        let piece = &mut self.pieces[pick.piece];
        let sel = piece.layers.selected;
        let channel = piece.layers.channel;
        if !piece.layers.layers[sel].accepts(channel) {
            return false;
        }
        let size = piece.layers.size;
        let max = (size - 1) as f32;
        let x = ((pick.u - pick.u.floor()) * max).round() as u32;
        let y = ((pick.v - pick.v.floor()) * max).round() as u32;
        // Islas y cobertura UV de esta pieza (solo sus triangulos): se arman la primera vez.
        let (a, b) = (piece.first_vertex, piece.first_vertex + piece.vertex_count);
        let maps = piece
            .uv_maps
            .get_or_insert_with(|| UvMaps::build(&uvs[a * 2..b * 2], size));
        let layer = &mut piece.layers.layers[sel];
        // En la mascara y en los canales se rellena con el gris del color del pincel; la mascara no lleva bloqueo alfa.
        // En la mascara de una capa de color liso se rellena de blanco (se ve el color).
        let (layer_id, target) = (layer.id, layer.target(channel));
        let solid = layer.mask_reveals();
        let (color, alpha_lock) = if target != PaintTarget::Layer {
            let on_mask = matches!(target, PaintTarget::Mask(_));
            let g = if on_mask && solid {
                255
            } else {
                luma(color)
            };
            ([g, g, g], layer.alpha_lock && !on_mask)
        } else {
            (color, layer.alpha_lock)
        };
        let pixels = layer.target_pixels_mut(target);
        let Some(plan) = fill::plan(pixels, size, x, y, color, alpha_lock, maps) else {
            return false;
        };
        // Antes de tocar nada: copia de las baldosas que va a cambiar (deshacer).
        piece.history.begin(layer_id, target);
        for r in fill::touched_tiles(&plan, size) {
            piece.history.save_tiles(pixels, size, r);
        }
        fill::apply(pixels, &plan, size, color, alpha_lock);
        piece.history.end();
        piece.layers.composite_rect(plan.rect);
        self.upload_rect(pick.piece, plan.rect);
        true
    }

    /// Degradado (herramienta Degradado), al apoyar el boton: fija el punto de inicio sobre el modelo y abre el registro de
    /// deshacer de su pieza. No pinta nada hasta que el puntero se mueve (`gradient_move`). Devuelve siempre false.
    fn gradient_begin(&mut self, camera: &Camera, px: f32, py: f32) -> bool {
        self.stroke_piece = None;
        self.gradient_from = None;
        let Some(pick) = self.pick_uv(camera, px, py) else {
            return false;
        };
        let piece = &self.pieces[pick.piece];
        let sel = piece.layers.selected;
        if !piece.layers.layers[sel].accepts(piece.layers.channel) {
            return false;
        }
        self.enter_piece(pick);
        self.gradient_from = Some(pick);
        false
    }

    /// Degradado, al mover el boton apretado: lleva el final del degradado al punto del modelo bajo el puntero y lo vuelve
    /// a pintar. Si el puntero sale del modelo o pasa a otra pieza se queda el ultimo degradado.
    fn gradient_move(&mut self, camera: &Camera, to: (f32, f32)) -> bool {
        let Some(from) = self.gradient_from else {
            return false;
        };
        let Some(end) = self.pick_uv(camera, to.0, to.1) else {
            return false;
        };
        if end.piece != from.piece {
            return false;
        }
        self.gradient_apply(from.piece, from.pos, end.pos)
    }

    /// Pinta el degradado de `from` a `to` (puntos 3D del modelo) en toda la pieza. Antes devuelve la capa a como estaba
    /// al apoyar el boton (como `replay_fade`), asi cada movimiento del raton recalcula el degradado entero y todo el
    /// arrastre entra al historial como un solo paso. En la mascara y en los canales se usa el gris del color del pincel
    /// (la mascara no lleva bloqueo alfa; en la mascara de una capa de color liso el gris de partida es blanco).
    fn gradient_apply(&mut self, piece_index: usize, from: Vec3, to: Vec3) -> bool {
        let brush_color = self.brush_color;
        let opacity = self.brush_opacity;
        let bvh = &self.bvh;
        let piece = &mut self.pieces[piece_index];
        let sel = piece.layers.selected;
        let size = piece.layers.size;
        let channel = piece.layers.channel;
        let (first_tri, end_tri) = (
            piece.first_vertex / 3,
            (piece.first_vertex + piece.vertex_count) / 3,
        );
        let layer = &mut piece.layers.layers[sel];
        if !layer.accepts(channel) {
            return false;
        }
        let target = layer.target(channel);
        let on_mask = matches!(target, PaintTarget::Mask(_));
        let (color, alpha_lock) = if target != PaintTarget::Layer {
            let g = if on_mask && layer.mask_reveals() {
                255
            } else {
                luma(brush_color)
            };
            ([g, g, g], layer.alpha_lock && !on_mask)
        } else {
            (brush_color, layer.alpha_lock)
        };
        let tris: Vec<SurfTri> = (first_tri..end_tri)
            .map(|t| {
                let (p, uv) = bvh.tri(t);
                SurfTri { p: *p, uv: *uv }
            })
            .collect();
        let Some(area) = surface_rect(size, &tris) else {
            return false;
        };
        let pixels = layer.target_pixels_mut(target);
        // Devuelve la capa a como estaba antes del degradado y guarda las baldosas que falten para deshacer.
        let restored = piece.history.restore_pending(pixels, size);
        piece.history.save_tiles(pixels, size, area);
        let spec = GradientSpec {
            from,
            to,
            color,
            opacity,
            alpha_lock,
            mask: on_mask,
        };
        let changed = self.stroke.dab_gradient(pixels, &tris, spec);
        let rect = match (restored, changed) {
            (Some(a), Some(b)) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
            (Some(r), None) | (None, Some(r)) => r,
            (None, None) => return false,
        };
        piece.layers.composite_rect(rect);
        self.upload_rect(piece_index, rect);
        true
    }

    /// Pasa al historial de cada pieza los cambios de estructura que hicieron las operaciones de su pila de capas (crear,
    /// duplicar, borrar y mover capas; ver `LayerStack::take_layout_log`). Se llama antes de pintar o de deshacer y despues
    /// de atender el panel de capas, para que los pasos queden en el orden en que ocurrieron y las capas borradas no
    /// se acumulen sin que el historial las vea.
    fn flush_layout_logs(&mut self) {
        for p in &mut self.pieces {
            for edit in p.layers.take_layout_log() {
                p.history.push_layout(edit);
            }
        }
    }

    /// Apoyar el boton: empieza un trazo nuevo y pinta el primer toque. Devuelve true si cambio algo.
    fn stroke_begin(&mut self, camera: &Camera, px: f32, py: f32) -> bool {
        self.flush_layout_logs();
        self.stroke_dabs.clear();
        self.stroke_fade_ok = true;
        // El cuentagotas no pinta ni entra al historial: solo toma el color (tambien al arrastrar).
        if self.tool == Tool::Eyedropper {
            self.pick_color(camera, px, py);
            return false;
        }
        // La mano solo mueve la vista: no pinta.
        if self.tool == Tool::Hand {
            return false;
        }
        if self.tool == Tool::Fill {
            return self.fill_at(camera, px, py);
        }
        // El degradado se fija al apoyar y se pinta al arrastrar (`gradient_move`).
        if self.tool == Tool::Gradient {
            return self.gradient_begin(camera, px, py);
        }
        // La pieza donde pinta el trazo se decide con el primer toque que cae sobre el modelo (`enter_piece`).
        self.stroke_piece = None;
        self.stroke_dir = Vec3::ZERO;
        self.last_dab_pos = None;
        // El primer toque se pinta aqui; el siguiente cae a un espaciado de distancia.
        self.stroke_to_next = self.dab_spacing();
        let mut dirty = None;
        if let Some(pick) = self.pick_uv(camera, px, py) {
            self.dab_surface(pick, &mut dirty);
        }
        self.flush(dirty)
    }

    /// Mayus+clic: empieza un trazo nuevo que recorre la recta de `from` a `to` (pantalla). A diferencia de
    /// `stroke_begin` no pone un toque en `from`: ahi ya quedo el final del trazo anterior, y repetirlo oscureceria ese
    /// punto con una opacidad menor que 1. El primer toque cae a un espaciado de `from`, como en cualquier trazo.
    /// Devuelve true si cambio algo.
    fn stroke_begin_line(&mut self, camera: &Camera, from: (f32, f32), to: (f32, f32)) -> bool {
        self.flush_layout_logs();
        self.stroke_dabs.clear();
        self.stroke_fade_ok = true;
        self.stroke_piece = None;
        self.stroke_dir = Vec3::ZERO;
        self.last_dab_pos = None;
        self.stroke_to_next = self.dab_spacing();
        self.stroke_move(camera, from, to)
    }

    /// true si con el boton apretado la herramienta sigue depositando pintura aunque el puntero este quieto: pincel o
    /// borrador con un preset que acumula (el aerografo) y el trazo en modo "techo" (con "Acumular pasada" encendido la
    /// regla es el maximo de los toques y repetir un toque en el mismo sitio no cambia nada).
    fn builds_up(&self) -> bool {
        matches!(self.tool, Tool::Brush | Tool::Eraser)
            && self.brush.kind.build_up()
            && self.ui.pass_ceiling()
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
        if self.brush.kind.fades()
            && self.stroke_fade_ok
            && matches!(self.tool, Tool::Brush | Tool::Eraser)
        {
            self.replay_fade();
        }
        self.stroke_dabs.clear();
        self.gradient_from = None;
        if let Some(p) = self.stroke_piece.take() {
            self.pieces[p].history.end();
        }
    }

    /// Guarda el proyecto en curso en disco (capas de todas las piezas y miniatura) y pide a la galeria que relea la
    /// lista. La primera vez le asigna carpeta; despues sobrescribe la misma.
    fn save_project(&mut self) {
        let id = self
            .project_id
            .get_or_insert_with(proyecto::new_id)
            .clone();
        let stacks: Vec<&LayerStack> = self.pieces.iter().map(|p| &p.layers).collect();
        let result = proyecto::save(
            &id,
            &self.project_name,
            self.model_path.as_deref(),
            &stacks,
            self.active(),
        );
        match result {
            Ok(dir) => println!("Proyecto guardado: {}", dir.display()),
            Err(e) => eprintln!("{e}"),
        }
        self.ui.reload_gallery();
        self.window.request_redraw();
    }

    /// Lo elegido en la ventana de Exportar: el PNG de siempre o los mapas de un motor. Devuelve el mensaje a mostrar en
    /// el titulo de la ventana.
    fn save_image(&self, kind: SaveKind) -> String {
        match kind {
            SaveKind::Png => self.save_png_image(),
            SaveKind::Engine(preset) => self.save_engine_maps(preset),
        }
    }

    /// Exportar para un motor: pregunta en que carpeta guardar y crea ahi una subcarpeta (`<nombre>_<motor>`, con un
    /// numero si ya existe, asi no pisa nada) con los mapas de la pieza activa empaquetados como los espera ese motor
    /// (`export::stack_engine_maps`). Devuelve el mensaje a mostrar en el titulo de la ventana.
    fn save_engine_maps(&self, preset: EnginePreset) -> String {
        let stack = &self.pieces[self.active()].layers;
        let mut base = slug(&self.project_name);
        if self.pieces.len() > 1 {
            base.push('_');
            base.push_str(&slug(
                &self.parts[self.selected_part.min(self.parts.len() - 1)].name,
            ));
        }
        let Some(parent) = rfd::FileDialog::new()
            .set_title(format!("Carpeta donde guardar los mapas de {}", preset.label()))
            .pick_folder()
        else {
            return self.status();
        };
        let dir = export::free_dir(&parent, &format!("{base}_{}", preset.folder_tag()));
        let maps = export::stack_engine_maps(preset, stack);
        match export::write_maps(&dir, &base, stack.size, &maps) {
            Ok(files) => {
                for f in &files {
                    println!("Mapa exportado: {}", f.display());
                }
                format!(
                    "texturecraft - {} mapas para {} en {}",
                    files.len(),
                    preset.label(),
                    dir.display()
                )
            }
            Err(e) => {
                eprintln!("{e}");
                format!("texturecraft - no se pudo exportar para {} ({e})", preset.label())
            }
        }
    }

    /// "Imagen PNG" de la ventana de Exportar: pregunta donde guardar y escribe la textura de la pieza activa como PNG
    /// transparente (sin el fondo de visualizacion). Devuelve el mensaje a mostrar en el titulo de la ventana.
    fn save_png_image(&self) -> String {
        let stack = &self.pieces[self.active()].layers;
        let mut default = slug(&self.project_name);
        if self.pieces.len() > 1 {
            default.push('_');
            default.push_str(&slug(
                &self.parts[self.selected_part.min(self.parts.len() - 1)].name,
            ));
        }
        let Some(mut path) = rfd::FileDialog::new()
            .set_title("Guardar imagen")
            .set_file_name(format!("{default}.png"))
            .add_filter("Imagen PNG", &["png"])
            .save_file()
        else {
            return self.status();
        };
        if path.extension().is_none() {
            path.set_extension("png");
        }
        // Sin fondo: el blanco o los cuadros del panel de capas son solo para ver lo pintado; el PNG sale transparente
        // donde no hay pintura.
        match export::write_png(&path, stack.size, &stack.export_composite(false), true) {
            Ok(()) => {
                println!("Imagen guardada: {}", path.display());
                // Los canales pintados (rugosidad, metalico, altura) salen al lado: <nombre>_<canal>.png.
                let n = Self::export_channels(stack, &path, false);
                let extra = if n > 0 {
                    format!(" (y {n} canales)")
                } else {
                    String::new()
                };
                format!("texturecraft - imagen guardada: {}{extra}", path.display())
            }
            Err(e) => {
                eprintln!("{e}");
                format!("texturecraft - no se pudo guardar la imagen ({e})")
            }
        }
    }

    /// Guarda junto a `path` un PNG en grises por cada canal extra que tenga pintura: `<ruta sin .png>_<canal>.png`.
    /// Con `avoid_overwrite` busca un nombre libre. Devuelve cuantos guardo.
    fn export_channels(stack: &LayerStack, path: &Path, avoid_overwrite: bool) -> usize {
        let stem = path.with_extension("");
        let mut n = 0;
        for k in ChannelKind::EXTRA {
            let Some(pixels) = stack.export_channel(k) else {
                continue;
            };
            let base = format!("{}_{}", stem.display(), k.file_tag());
            let out = if avoid_overwrite {
                export::free_path(&base)
            } else {
                std::path::PathBuf::from(format!("{base}.png"))
            };
            match export::write_png(&out, stack.size, &pixels, true) {
                Ok(()) => {
                    println!("Canal exportado: {}", out.display());
                    n += 1;
                }
                Err(e) => eprintln!("{e}"),
            }
        }
        // Con la altura pintada tambien sale el mapa de normales (azul-violeta) calculado a partir de ella.
        if let Some(pixels) = stack.export_normal(NORMAL_STRENGTH) {
            let base = format!("{}_normal", stem.display());
            let out = if avoid_overwrite {
                export::free_path(&base)
            } else {
                std::path::PathBuf::from(format!("{base}.png"))
            };
            match export::write_png(&out, stack.size, &pixels, true) {
                Ok(()) => {
                    println!("Mapa de normales exportado: {}", out.display());
                    n += 1;
                }
                Err(e) => eprintln!("{e}"),
            }
        }
        n
    }

    /// Exporta un PNG a la carpeta actual (seccion 13). `kind`: 0 = textura con fondo, 1 = textura transparente,
    /// 2 = solo la capa elegida (sus pixeles tal cual, sin opacidad ni visibilidad). Exporta la pieza activa; con
    /// varias piezas el nombre del archivo lleva el de la malla. Devuelve el mensaje a mostrar.
    fn export_png(&self, kind: u8) -> String {
        let stack = &self.pieces[self.active()].layers;
        let size = stack.size;
        let (base, pixels) = match kind {
            0 => ("texturecraft_textura", stack.export_composite(true)),
            1 => (
                "texturecraft_textura_transparente",
                stack.export_composite(false),
            ),
            _ => ("texturecraft_capa", stack.selected_layer().pixels.clone()),
        };
        let mut base = base.to_string();
        if self.pieces.len() > 1 {
            base.push('_');
            base.push_str(&slug(
                &self.parts[self.selected_part.min(self.parts.len() - 1)].name,
            ));
        }
        let path = export::free_path(&base);
        match export::write_png(&path, size, &pixels, true) {
            Ok(()) => {
                let full = std::fs::canonicalize(&path).unwrap_or(path.clone());
                println!("Exportado: {}", full.display());
                // Con la textura de color tambien salen los canales pintados, con nombres libres (no pisan nada).
                let n = if kind == 0 {
                    Self::export_channels(stack, &path, true)
                } else {
                    0
                };
                let extra = if n > 0 {
                    format!(" (y {n} canales)")
                } else {
                    String::new()
                };
                format!("texturecraft - exportado: {}{extra}", path.display())
            }
            Err(e) => {
                eprintln!("{e}");
                format!("texturecraft - no se pudo exportar ({e})")
            }
        }
    }

    /// Deshacer (`redo = false`) o rehacer el ultimo paso de la pieza activa: un trazo, una operacion sobre toda la hoja o
    /// un cambio de estructura de las capas (crear, duplicar, borrar, mover). Devuelve true si cambio algo.
    fn undo_redo(&mut self, redo: bool) -> bool {
        self.flush_layout_logs();
        let a = self.active();
        let piece = &mut self.pieces[a];
        let rect = if redo {
            piece.history.redo(&mut piece.layers)
        } else {
            piece.history.undo(&mut piece.layers)
        };
        match rect {
            Some(r) => {
                // Deshacer o rehacer cambia los ajustes por su cuenta: se toman como los "vistos" para que el
                // siguiente cuadro no los registre como un cambio nuevo (vaciaria el rehacer).
                piece.settings_seen = piece.layers.settings_snapshot();
                piece.layers.composite_rect(r);
                self.upload_rect(a, r);
                // Si el paso cambio las capas, la capa elegida (y el titulo con su nombre) pudo cambiar.
                self.window.set_title(&self.status());
                self.window.request_redraw();
                true
            }
            None => false,
        }
    }

    /// Levanta el pincel sin soltar el boton: el trazo en curso sigue, pero lo que se pinte a continuacion no se une con
    /// lo anterior (ni en direccion ni en espaciado). Se usa cuando una regla curva hace saltar el punto de un lado a
    /// otro y no debe quedar una cuerda recta entre los dos.
    fn stroke_lift(&mut self) {
        self.stroke_dir = Vec3::ZERO;
        self.last_dab_pos = None;
        self.stroke_to_next = self.dab_spacing();
    }

    /// Mover con el boton apretado: rellena el tramo `from` -> `to` con toques espaciados (seccion 7.4).
    fn stroke_move(&mut self, camera: &Camera, from: (f32, f32), to: (f32, f32)) -> bool {
        if self.tool == Tool::Eyedropper {
            self.pick_color(camera, to.0, to.1);
            return false;
        }
        // El degradado recalcula su final bajo el puntero (no se pinta por toques).
        if self.tool == Tool::Gradient {
            return self.gradient_move(camera, to);
        }
        // El relleno es de un solo toque por apoyo: arrastrar no vuelve a rellenar. La mano tampoco pinta.
        if self.tool == Tool::Fill || self.tool == Tool::Hand {
            return false;
        }
        // Se recorre el tramo de pantalla en pasos chicos y se mide la distancia 3D sobre el modelo entre cada par de
        // puntos; los toques se colocan cada `spacing` unidades arrastrando el sobrante (`dab_points`), asi quedan igual
        // de separados con un trazo rapido o lento (como en photocraft).
        let perf_t0 = std::time::Instant::now();
        let perf_d0 = PERF_DABS.load(std::sync::atomic::Ordering::Relaxed);
        let perf_p0 = PERF_PAINT_NS.load(std::sync::atomic::Ordering::Relaxed);
        let perf_c0 = PERF_COMP_NS.load(std::sync::atomic::Ordering::Relaxed);
        let spacing = self.dab_spacing();
        let screen = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
        let steps = (screen / WALK_STEP_PX).ceil().max(1.0) as usize;
        let (mut a, mut ua) = (from, self.pick_uv(camera, from.0, from.1));
        let mut dirty = None;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let b = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            let ub = self.pick_uv(camera, b.0, b.1);
            self.stroke_step(camera, a, ua, b, ub, spacing, &mut dirty);
            (a, ua) = (b, ub);
        }
        let changed = self.flush(dirty);
        if perf_on() {
            let ord = std::sync::atomic::Ordering::Relaxed;
            eprintln!(
                "[perf] mover {:.0} px: {} toques, total {:.2} ms (pintar {:.2} ms, recomponer {:.2} ms), simetria {:?}",
                screen,
                PERF_DABS.load(ord) - perf_d0,
                perf_t0.elapsed().as_secs_f64() * 1e3,
                (PERF_PAINT_NS.load(ord) - perf_p0) as f64 / 1e6,
                (PERF_COMP_NS.load(ord) - perf_c0) as f64 / 1e6,
                self.ui.symmetry(),
            );
        }
        changed
    }

    /// Distancia entre toques en unidades del modelo (una fraccion del radio del pincel, como en la seccion 7.4).
    fn dab_spacing(&self) -> f32 {
        (self.brush.radius * WORLD_PER_RADIUS * self.brush.kind.spacing())
            .max(0.5 * WORLD_PER_RADIUS)
    }

    /// Un paso corto del raton `a` -> `b`: pone los toques que le tocan segun lo recorrido sobre el modelo. Si `a` cae
    /// fuera del modelo o en otra pieza (el trazo entra o cambia de pieza), o si hay un salto enorme entre los dos
    /// puntos (el borde del modelo, donde un punto es de la parte de delante y otro de la de atras), se pinta un toque
    /// en `b` y se vuelve a contar desde ahi. Si `b` cae fuera del modelo no se pinta nada.
    #[allow(clippy::too_many_arguments)]
    fn stroke_step(
        &mut self,
        camera: &Camera,
        a: (f32, f32),
        ua: Option<Pick>,
        b: (f32, f32),
        ub: Option<Pick>,
        spacing: f32,
        dirty: &mut Option<Rect>,
    ) {
        let Some(q) = ub else { return };
        let jump = 8.0 * self.brush.radius * WORLD_PER_RADIUS;
        let dist = match ua {
            Some(p) if p.piece == q.piece && p.pos.distance(q.pos) <= jump => p.pos.distance(q.pos),
            _ => {
                self.dab_surface(q, dirty);
                self.stroke_to_next = spacing;
                return;
            }
        };
        for t in dab_points(&mut self.stroke_to_next, dist, spacing) {
            // Con t = 1 el toque cae justo en `b`: ya se tiene el punto.
            let pick = if t >= 0.9999 {
                q
            } else {
                self.pick_uv(camera, a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
                    .unwrap_or(q)
            };
            self.dab_surface(pick, dirty);
        }
    }

    /// Dibuja un cuadro (modelo + interfaz). Devuelve cuanto falta para el proximo cuadro que pide egui
    /// (`ZERO` = ya mismo, `Duration::MAX` = ninguno) y el eje del gizmo al que se hizo clic, si hubo (la camara
    /// vive en `App`, asi que es quien la gira).
    fn render(
        &mut self,
        camera: &Camera,
        mouse: (f32, f32),
        cursor_in_window: bool,
        dragging: bool,
    ) -> (Duration, Option<ViewAxis>) {
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
        let uniforms = Uniforms {
            mvp: mvp.to_cols_array_2d(),
            normal_matrix: normal.to_cols_array_2d(),
            params: [
                if self.pieces[self.active()].layers.material_view {
                    1.0
                } else {
                    0.0
                },
                // Modo edicion de caras: la malla se ve gris liso, sin textura.
                if self.faces.active { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        // Misma textura vista sin sRGB, solo para la interfaz (ver el comentario en `Gpu::new`).
        let ui_view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.remove_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cuadro"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("modelo"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, self.flag_buffer.slice(..));
            // Solo las mallas visibles, cada una con la textura de su pieza.
            for (piece, range) in &self.draw_ranges {
                pass.set_bind_group(0, &self.pieces[*piece].bind_group, &[]);
                pass.draw(range.clone(), 0..1);
            }
            // Modo edicion de caras: las aristas encima de la malla (el bind group solo aporta los uniforms).
            if self.faces.active {
                if let Some(edges) = &self.edge_buffer {
                    pass.set_pipeline(&self.line_pipeline);
                    pass.set_bind_group(0, &self.pieces[0].bind_group, &[]);
                    pass.set_vertex_buffer(0, edges.slice(..));
                    pass.draw(0..self.edge_vertex_count, 0..1);
                }
            }
        }
        // Cursor propio sobre el modelo: circulo del pincel (pincel y borrador) o la chincheta del cuentagotas.
        // Sin círculo del pincel en modo edición (no se pinta).
        // Con un trazo pegado a la regla recta, el circulo sigue la linea (donde cae el toque), no al raton.
        let mouse = self.ui.ruler_snap(mouse);
        let ring = if cursor_in_window
            && !self.ui.edit_mode()
            && !self.faces.active
            && matches!(
                self.tool,
                Tool::Brush | Tool::Eraser | Tool::Eyedropper | Tool::Smudge | Tool::Blur
            )
        {
            let pin = self.tool == Tool::Eyedropper;
            // Chincheta: el color que hay bajo la punta (el que se tomaria al hacer clic), para mostrarlo dentro del aro.
            let color = if pin {
                self.color_at(camera, mouse.0, mouse.1)
            } else {
                None
            };
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
            Some(BrushRing {
                pos: [mouse.0, mouse.1],
                radius: self.ring_radius,
                on_model,
                pin,
                color,
            })
        } else {
            None
        };
        // Interfaz (egui) encima del modelo. El panel de capas muestra las de la pieza activa.
        let active = self.active();
        let UiOutput {
            full_output,
            changed: ui_changed,
            view_axis,
            layers_dirty,
            parts_dirty,
            save_project,
            save_image,
        } = self.ui.run(
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
        // El panel de capas pudo crear, duplicar, borrar o mover capas: esos cambios pasan al historial de la pieza
        // (Ctrl+Z los deshace) antes de cualquier otra cosa que tambien entre a el.
        self.flush_layout_logs();
        // Ajustes de capas (opacidad, modo, nombre, ojo, candados, recorte, color liso, mascara encendida o invertida):
        // se comparan con los del cuadro anterior y el cambio entra al historial (un arrastre de deslizador es un paso).
        {
            let p = &mut self.pieces[active];
            let now = p.layers.settings_snapshot();
            p.history
                .record_settings(layers::settings_diff(&p.settings_seen, &now), dragging);
            p.settings_seen = now;
        }
        // El panel de capas cambio algo que se ve (ojo, opacidad, modo, recorte, fondo): se recompone y se sube la
        // textura de esa pieza antes de enviar este cuadro.
        if layers_dirty {
            self.refresh_piece(active);
        }
        // Modo Solo: se encendio, se apago o se eligio otra capa con el modo encendido (por el panel o por teclado): se
        // recompone y se sube la textura para que se vea la capa que toca. Si ya se recompuso arriba, no hace nada.
        if self.pieces[active].layers.solo_dirty() {
            self.refresh_piece(active);
        }
        // "Limpiar capa" o "Invertir colores" del panel de capas: se atienden aqui porque necesitan el historial de la
        // pieza (Ctrl+Z los deshace).
        if let Some(op) = self.pieces[active].layers.op_requested.take() {
            let p = &mut self.pieces[active];
            let changed = match op {
                LayerOp::Clear => undo::clear_selected(&mut p.layers, &mut p.history),
                LayerOp::Invert => undo::invert_selected(&mut p.layers, &mut p.history),
            };
            if let Some(r) = changed {
                p.layers.composite_rect(r);
                self.upload_rect(active, r);
                self.window.request_redraw();
            }
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
        // "Regresar a mi galería": antes de volver se guarda el proyecto (capas y miniatura).
        if save_project {
            self.save_project();
        }
        // Ventana de Exportar: diálogo para elegir dónde guardar y exportación de la textura (PNG o mapas de un motor).
        if let Some(kind) = save_image {
            let msg = self.save_image(kind);
            self.window.set_title(&msg);
        }
        // El boton Importar del panel de mallas ya tiene el archivo elegido: se reemplaza el modelo.
        if let Some(path) = self.ui.take_import() {
            self.open_obj(&path);
        }
        // Se pulso la tarjeta de un proyecto guardado en la galeria: se abre. Si falla, la galeria vuelve a mostrarse.
        if let Some(id) = self.ui.take_open_project() {
            if let Err(e) = self.open_project(&id) {
                eprintln!("{e}");
                self.ui.show_gallery();
                self.window.request_redraw();
            }
        }
        if ui_changed || piece_changed {
            self.window.set_title(&self.status());
        }
        let size = [self.config.width, self.config.height];
        let (extra, repaint_delay) = self.ui.paint(
            &self.device,
            &self.queue,
            &mut encoder,
            &ui_view,
            size,
            full_output,
        );
        self.queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        self.window.pre_present_notify();
        self.queue.present(frame);
        (repaint_delay, view_axis)
    }
}

/// Resumen de la vista (camara y tamano de la ventana): dos resumenes iguales significan que un punto de la pantalla sigue
/// siendo el mismo sitio del modelo. Lo usa `App::line_anchor` para Mayus+clic.
fn view_key(camera: &Camera, width: u32, height: u32) -> [f32; 8] {
    [
        camera.angle_x,
        camera.angle_y,
        camera.distance(),
        camera.pan_x,
        camera.pan_z,
        if camera.orthographic { 1.0 } else { 0.0 },
        width as f32,
        height as f32,
    ]
}

#[cfg(test)]
mod view_key_tests {
    use super::*;

    #[test]
    fn el_resumen_de_la_vista_cambia_con_cualquier_movimiento_de_camara_o_de_ventana() {
        let base = Camera::default();
        let k = view_key(&base, 1280, 720);
        // La misma vista da el mismo resumen (asi Mayus+clic reconoce que nada se movio).
        assert_eq!(k, view_key(&Camera::default(), 1280, 720));
        let mut girada = Camera::default();
        girada.rotate_by(10.0, 0.0);
        assert_ne!(k, view_key(&girada, 1280, 720));
        let mut movida = Camera::default();
        movida.pan_by(10.0, 10.0);
        assert_ne!(k, view_key(&movida, 1280, 720));
        let mut acercada = Camera::default();
        acercada.zoom(1.0);
        assert_ne!(k, view_key(&acercada, 1280, 720));
        let mut ortografica = Camera::default();
        ortografica.orthographic = true;
        assert_ne!(k, view_key(&ortografica, 1280, 720));
        assert_ne!(k, view_key(&base, 1920, 1080));
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
            // Pegado a la regla recta, el toque cae sobre su linea.
            let at = gpu.ui.ruler_snap(self.mouse);
            if !std::mem::take(&mut self.moved_since_tick)
                && gpu.stroke_hold(&self.camera, at.0, at.1)
            {
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
        let mut model_path = None;
        let mesh = match std::env::args().nth(1) {
            Some(arg) => match load_mesh(Path::new(&arg)) {
                Ok(m) => {
                    title = format!("texturecraft - {}", file_name(Path::new(&arg)));
                    model_name = file_name(Path::new(&arg));
                    model_path = Some(std::path::PathBuf::from(&arg));
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
        if let Some(g) = self.gpu.as_mut() {
            g.model_path = model_path;
            g.window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        // egui ve cada evento primero. Si lo "consume" (cursor sobre la barra, etc.), la camara y el pincel no
        // reaccionan. Los gestos que empezaron fuera de la interfaz siguen su curso aunque pasen por encima.
        // Tab no se le pasa a egui (salvo que se este escribiendo en un campo de texto): egui lo usa para mover el foco
        // entre widgets y despues de eso marca como "usadas" las demas teclas (A, H, Ctrl+I...), que dejarian de llegar.
        let is_tab = matches!(
            &event,
            WindowEvent::KeyboardInput { event: k, .. }
                if k.logical_key == Key::Named(NamedKey::Tab)
        );
        let response = if is_tab && !gpu.ui.wants_keyboard() {
            egui_winit::EventResponse {
                repaint: false,
                consumed: false,
            }
        } else {
            gpu.ui.on_window_event(&gpu.window, &event)
        };
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
                        // Modo edición: el botón izquierdo no pinta (solo sirve para elegir cosas de la interfaz).
                        if down && gpu.ui.edit_mode() {
                            return;
                        }
                        // Modo edicion de caras: el clic elige la cara tocada (Mayus suma o quita) y no pinta. Con Alt
                        // elige el bucle de caras (la fila o columna) que pasa por ella, hacia la arista mas cercana. Con
                        // Ctrl elige el camino mas corto desde la ultima cara tocada (Mayus+Ctrl lo suma a lo elegido).
                        if gpu.faces.active {
                            if down {
                                let pick = gpu.pick_uv(&self.camera, self.mouse.0, self.mouse.1);
                                let shift = self.modifiers.shift_key();
                                if self.modifiers.alt_key() {
                                    gpu.faces.click_loop(
                                        pick.map(|p| p.tri),
                                        pick.map(|p| p.pos.to_array()),
                                        shift,
                                    );
                                } else if self.modifiers.control_key() {
                                    gpu.faces.click_path(pick.map(|p| p.tri), shift);
                                } else {
                                    gpu.faces.click(pick.map(|p| p.tri), shift);
                                }
                                gpu.faces_changed();
                            }
                            return;
                        }
                        let was_down = self.left_down;
                        self.left_down = down;
                        if down {
                            // Mayus+clic con pincel o borrador: recta desde donde termino el ultimo trazo hasta el clic
                            // (solo si la vista sigue como estaba; si no, se pinta un trazo normal).
                            let line_from = if self.modifiers.shift_key()
                                && matches!(gpu.tool, Tool::Brush | Tool::Eraser)
                            {
                                let key =
                                    view_key(&self.camera, gpu.config.width, gpu.config.height);
                                self.line_anchor
                                    .filter(|(_, k)| *k == key)
                                    .map(|(p, _)| p)
                            } else {
                                None
                            };
                            if let Some(from) = line_from {
                                self.last_paint = Some(self.mouse);
                                self.moved_since_tick = false;
                                self.next_build_up = None;
                                if gpu.stroke_begin_line(&self.camera, from, self.mouse) {
                                    gpu.window.request_redraw();
                                }
                            } else {
                                // Con la regla recta, un trazo que empieza cerca de ella queda pegado a su linea.
                                let start = if matches!(gpu.tool, Tool::Brush | Tool::Eraser) {
                                    gpu.ui.ruler_begin(self.mouse)
                                } else {
                                    self.mouse
                                };
                                self.last_paint = Some(start);
                                self.moved_since_tick = false;
                                self.next_build_up = None;
                                if gpu.stroke_begin(&self.camera, start.0, start.1) {
                                    gpu.window.request_redraw();
                                }
                            }
                        } else if was_down {
                            // Donde termina este trazo: desde ahi sale la recta del proximo Mayus+clic.
                            if matches!(gpu.tool, Tool::Brush | Tool::Eraser) {
                                let key =
                                    view_key(&self.camera, gpu.config.width, gpu.config.height);
                                self.line_anchor = self.last_paint.map(|p| (p, key));
                            }
                            self.last_paint = None;
                            gpu.ui.ruler_end();
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
                    let mut from = self.last_paint.unwrap_or((x, y));
                    if dx != 0.0 || dy != 0.0 {
                        self.moved_since_tick = true;
                    }
                    // Pegado a una regla, el recorrido del raton (de la posicion anterior a esta) se corrige punto por
                    // punto: sobre la recta es un solo tramo; sobre el circulo o la elipse, varios pasos chicos.
                    let mut changed = false;
                    for (to, jump) in gpu.ui.ruler_path((x - dx, y - dy), (x, y)) {
                        if jump {
                            // El raton cruzo el centro de un circulo o elipse y el punto de la regla paso al otro lado:
                            // no se pinta la cuerda entre los dos, el trazo se retoma desde el punto nuevo.
                            gpu.stroke_lift();
                        } else {
                            changed |= gpu.stroke_move(&self.camera, from, to);
                        }
                        from = to;
                    }
                    self.last_paint = Some(from);
                    if changed {
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
                if matches!(
                    gpu.tool,
                    Tool::Brush | Tool::Eraser | Tool::Eyedropper | Tool::Smudge | Tool::Blur
                ) {
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
            // Tab: entra o sale del modo edicion de caras (como en Blender). Va aparte porque egui marca la tecla Tab
            // como "usada" siempre (la usa para pasar de un control a otro) y el filtro de abajo la dejaria pasar sin
            // hacer nada.
            WindowEvent::KeyboardInput { ref event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && event.logical_key == Key::Named(NamedKey::Tab)
                    && !gpu.ui.wants_keyboard() =>
            {
                gpu.toggle_face_edit();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !ui_has_it =>
            {
                match event.logical_key {
                    // Esc dentro del modo edicion solo sale del modo; fuera de el cierra el programa.
                    Key::Named(NamedKey::Escape) if gpu.faces.active => gpu.toggle_face_edit(),
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
                    // Alt+H: muestra todas las caras ocultas (dentro y fuera del modo edicion).
                    Key::Character(ref c)
                        if self.modifiers.alt_key() && c.eq_ignore_ascii_case("h") =>
                    {
                        if gpu.faces.reveal_all() {
                            gpu.faces_changed();
                        }
                    }
                    // Dentro del modo edicion de caras las letras valen las de Blender; las demas no hacen nada.
                    Key::Character(ref c) if gpu.faces.active => {
                        let (shift, ctrl, alt) = (
                            self.modifiers.shift_key(),
                            self.modifiers.control_key(),
                            self.modifiers.alt_key(),
                        );
                        let handled = match (c.to_lowercase().as_str(), ctrl, alt, shift) {
                            ("a", false, false, _) => {
                                gpu.faces.select_all();
                                true
                            }
                            ("a", false, true, _) => {
                                gpu.faces.deselect_all();
                                true
                            }
                            ("i", true, false, _) => {
                                gpu.faces.invert();
                                true
                            }
                            ("h", false, false, false) => {
                                gpu.faces.hide_selected();
                                true
                            }
                            ("h", false, false, true) => {
                                gpu.faces.hide_unselected();
                                true
                            }
                            _ => false,
                        };
                        if handled {
                            gpu.faces_changed();
                        }
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
                                let i = BlendMode::ALL
                                    .iter()
                                    .position(|&m| m == l.blend)
                                    .unwrap_or(0);
                                l.blend = BlendMode::ALL
                                    [if shift { (i + n - 1) % n } else { (i + 1) % n }];
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
                            "r" => {
                                gpu.tool = Tool::Gradient;
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
                let (delay, view_axis) =
                    gpu.render(
                        &self.camera,
                        self.mouse,
                        self.cursor_in_window,
                        self.left_down,
                    );
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
        line_anchor: None,
        modifiers: ModifiersState::default(),
        cursor_in_window: false,
        next_build_up: None,
        moved_since_tick: false,
    };
    event_loop
        .run_app(&mut app)
        .expect("error en el bucle de eventos");
}

#[cfg(test)]
mod mirror_tests {
    use super::*;

    #[test]
    fn mirror_vec_refleja_solo_los_ejes_marcados() {
        let v = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(mirror_vec(v, [false; 3]), v);
        assert_eq!(mirror_vec(v, [true, false, false]), Vec3::new(-1.0, 2.0, 3.0));
        assert_eq!(mirror_vec(v, [false, true, true]), Vec3::new(1.0, -2.0, -3.0));
        assert_eq!(mirror_vec(v, [true; 3]), -v);
        // Reflejar dos veces devuelve el original.
        let f = [true, false, true];
        assert_eq!(mirror_vec(mirror_vec(v, f), f), v);
    }
}
