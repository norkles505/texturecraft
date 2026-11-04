// Visor del modelo con su textura. Portado del shader de TexturedMeshGeometry.kt (ESPECIFICACION.md, seccion 14.2).
// La luz esta fija respecto a la CAMARA: las normales llegan en espacio de vista (+X derecha, +Y arriba, +Z hacia
// quien mira). No es PBR completo: con la "Vista: material" se usan la rugosidad, el metalico y la altura de la
// textura de material (R, G, B) con una luz sencilla de tipo Blinn-Phong.

struct Uniforms {
    mvp: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
    // x = 1.0 con la vista de material; 0.0 = la luz de siempre.
    // y = 1.0 en el modo edicion de caras: la malla se ve gris liso, sin textura (como el modo edicion de Blender).
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// Gris de la malla en el modo edicion. Las aristas ya no se acercan a la camara (0.0): quien se aparta es la superficie
// (`bias` de profundidad del pipeline del modelo en main.rs). Si hiciera falta, un valor muy chico (0.00002) ayuda
// con las lineas que se vean cortadas; subirlo mucho deja ver las aristas de la parte de atras.
const EDIT_GRAY: f32 = 0.62;
const EDGE_DEPTH_BIAS: f32 = 0.0;

// Intensidad global de la luz (1.0 = valores exactos de la especificacion, que se saturan). Subir = mas claro.
const LIGHT_GAIN: f32 = 0.7;
// Cuanto inclina la altura la normal (relieve). Subir = mas relieve.
const BUMP_STRENGTH: f32 = 24.0;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
// Material: R = rugosidad, G = metalico, B = altura (0.5 = plano).
@group(0) @binding(3) var mat_tex: texture_2d<f32>;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    // Modo edicion de caras: 0 = normal, 1 = cara elegida (se tine), 2 = cara oculta (no se dibuja).
    @location(3) flag: f32,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) n: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) flag: f32,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = u.mvp * vec4<f32>(in.pos, 1.0);
    // Una cara oculta manda sus tres vertices al mismo punto fuera de pantalla: el triangulo queda vacio y no se dibuja.
    if (in.flag > 1.5) {
        out.clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    out.n = (u.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    out.flag = in.flag;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Las lecturas y las derivadas van antes de cualquier rama (necesitan flujo uniforme).
    let tex_base = textureSample(tex, samp, in.uv).rgb;
    let editing = u.params.y > 0.5;
    // En el modo edicion no se ve la textura: la malla es gris liso y solo se ven la luz, las aristas y la seleccion.
    let base = select(tex_base, vec3<f32>(EDIT_GRAY), editing);
    let m = textureSample(mat_tex, samp, in.uv);
    let dh_dx = dpdx(m.b);
    let dh_dy = dpdy(m.b);

    var n = normalize(in.n);
    // Luz principal arriba-izquierda-frente, y relleno suave desde la derecha-abajo.
    let key_dir = normalize(vec3<f32>(-0.45, 0.65, 0.75));
    let fill_dir = normalize(vec3<f32>(0.70, -0.25, 0.50));
    // Mitad del camino entre la luz principal y el ojo (en +Z).
    let half_dir = normalize(key_dir + vec3<f32>(0.0, 0.0, 1.0));

    var color: vec3<f32>;
    if (u.params.x < 0.5 || editing) {
        // Ambiente: mas claro hacia arriba, mas oscuro hacia abajo.
        let ambient = mix(0.42, 0.78, n.y * 0.5 + 0.5);
        let key = max(dot(n, key_dir), 0.0) * 0.50;
        let fill = max(dot(n, fill_dir), 0.0) * 0.14;
        // Brillo suave (Blinn-Phong) con el ojo en +Z.
        let spec = pow(max(dot(n, half_dir), 0.0), 28.0) * 0.12;
        // La luz de la especificacion suma hasta ~1.4 (ambiente + principal + relleno) y se recortaba a blanco en la
        // zona iluminada. LIGHT_GAIN la baja para que el maximo quede cerca de 1.0 y se vea el sombreado.
        let light = (ambient + key + fill) * LIGHT_GAIN;
        color = clamp(base * light + vec3<f32>(spec), vec3<f32>(0.0), vec3<f32>(1.0));
    } else {
        let rough = m.r;
        let metal = m.g;
        // Relieve: la altura sube hacia la derecha -> la normal se inclina a la izquierda; en pantalla Y crece hacia
        // abajo y en el espacio de vista hacia arriba, de ahi el cambio de signo.
        n = normalize(n + vec3<f32>(-dh_dx, dh_dy, 0.0) * BUMP_STRENGTH);
        let ambient = mix(0.42, 0.78, n.y * 0.5 + 0.5);
        let key = max(dot(n, key_dir), 0.0) * 0.50;
        let fill = max(dot(n, fill_dir), 0.0) * 0.14;
        // Brillo: liso = punto chico e intenso, rugoso = amplio y tenue.
        let shin = mix(160.0, 6.0, rough);
        let spec_amt = pow(max(dot(n, half_dir), 0.0), shin);
        // Reflectancia: un dielectrico refleja ~4 % en blanco; un metal refleja su propio color y no tiene difuso.
        let f0 = mix(vec3<f32>(0.04), base, metal);
        let diffuse = base * (1.0 - metal) * (ambient + key + fill) * LIGHT_GAIN;
        // El metal tambien refleja el "ambiente" con su color, mas nitido cuanto menos rugoso.
        let env = f0 * ambient * metal * mix(0.9, 0.35, rough);
        let spec = f0 * spec_amt * mix(6.0, 0.5, rough);
        color = clamp(diffuse + env + spec, vec3<f32>(0.0), vec3<f32>(1.0));
    }
    // Cara elegida en el modo edicion: se tine de naranja por encima de la textura.
    if (in.flag > 0.5 && in.flag < 1.5) {
        color = mix(color, vec3<f32>(1.0, 0.55, 0.1), 0.55);
    }
    // La textura y la luz se calculan "tal cual" (como en Android). La superficie es sRGB y el hardware codifica al
    // escribir, asi que se pasa a lineal aqui para que lo que llega a pantalla sea justo `color`.
    return vec4<f32>(srgb_to_linear(color), 1.0);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Aristas del modo edicion: lineas sueltas. Cada vertice trae x y z y, en w, 1 si la arista es de una cara elegida.
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) sel: f32,
};

@vertex
fn vs_line(@location(0) p: vec4<f32>) -> LineOut {
    var out: LineOut;
    out.clip = u.mvp * vec4<f32>(p.xyz, 1.0);
    // Un poquito hacia la camara, para que la linea no pelee con la superficie sobre la que esta.
    out.clip.z = out.clip.z - EDGE_DEPTH_BIAS * out.clip.w;
    out.sel = p.w;
    return out;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    // Negras, y naranjas las de las caras elegidas.
    let c = select(vec3<f32>(0.04), vec3<f32>(1.0, 0.6, 0.1), in.sel > 0.5);
    return vec4<f32>(srgb_to_linear(c), 1.0);
}
