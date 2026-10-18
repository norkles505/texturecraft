// Visor del modelo con su textura. Portado del shader de TexturedMeshGeometry.kt (ESPECIFICACION.md, seccion 14.2).
// La luz esta fija respecto a la CAMARA: las normales llegan en espacio de vista (+X derecha, +Y arriba, +Z hacia
// quien mira). No es PBR.

struct Uniforms {
    mvp: mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

// Intensidad global de la luz (1.0 = valores exactos de la especificacion, que se saturan). Subir = mas claro.
const LIGHT_GAIN: f32 = 0.7;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) n: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = u.mvp * vec4<f32>(in.pos, 1.0);
    out.n = (u.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.n);
    // Ambiente: mas claro hacia arriba, mas oscuro hacia abajo.
    let ambient = mix(0.42, 0.78, n.y * 0.5 + 0.5);
    // Luz principal arriba-izquierda-frente, y relleno suave desde la derecha-abajo.
    let key_dir = normalize(vec3<f32>(-0.45, 0.65, 0.75));
    let fill_dir = normalize(vec3<f32>(0.70, -0.25, 0.50));
    let key = max(dot(n, key_dir), 0.0) * 0.50;
    let fill = max(dot(n, fill_dir), 0.0) * 0.14;
    // Brillo suave (Blinn-Phong) con el ojo en +Z.
    let half_dir = normalize(key_dir + vec3<f32>(0.0, 0.0, 1.0));
    let spec = pow(max(dot(n, half_dir), 0.0), 28.0) * 0.12;
    let base = textureSample(tex, samp, in.uv).rgb;
    // La luz de la especificacion suma hasta ~1.4 (ambiente + principal + relleno) y se recortaba a blanco en la
    // zona iluminada. LIGHT_GAIN la baja para que el maximo quede cerca de 1.0 y se vea el sombreado.
    let light = (ambient + key + fill) * LIGHT_GAIN;
    let color = clamp(base * light + vec3<f32>(spec), vec3<f32>(0.0), vec3<f32>(1.0));
    // La textura y la luz se calculan "tal cual" (como en Android). La superficie es sRGB y el hardware codifica al
    // escribir, asi que se pasa a lineal aqui para que lo que llega a pantalla sea justo `color`.
    return vec4<f32>(srgb_to_linear(color), 1.0);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}
