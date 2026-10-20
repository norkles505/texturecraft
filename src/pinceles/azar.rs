//! Azar determinista y ruido, compartidos por todos los pinceles (sin archivos ni azar de verdad: el mismo numero de
//! toque da siempre el mismo resultado).

use std::f32::consts::PI;

use glam::Vec3;

/// Sal del angulo de cada toque (la usan la tiza, el pastel y el punteado).
const DAB_ANGLE_SALT: u64 = 0x414E_474C;

/// Mezcla de 64 bits (splitmix64): pocos cambios en la entrada dan una salida muy distinta.
fn mix(z: u64) -> u64 {
    let z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Numero entre 0 y 1 (sin incluir el 1) a partir de una semilla.
pub(super) fn unit(z: u64) -> f32 {
    (mix(z) >> 40) as f32 / 16_777_216.0
}

pub(super) fn hash2(x: i32, y: i32, salt: u64) -> f32 {
    unit(mix(salt ^ u64::from(x as u32)) ^ (u64::from(y as u32) << 32))
}

fn hash3(x: i32, y: i32, z: i32, salt: u64) -> f32 {
    unit(mix(mix(salt ^ u64::from(x as u32)) ^ (u64::from(y as u32) << 32)) ^ u64::from(z as u32))
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Ruido de valores 2D que se repite cada `period` celdas.
pub(super) fn value_noise2(x: f32, y: f32, period: i32, salt: u64) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (smooth(x - x0), smooth(y - y0));
    let (xi, yi) = (x0 as i32, y0 as i32);
    let h = |i: i32, j: i32| hash2(i.rem_euclid(period), j.rem_euclid(period), salt);
    let a = lerp(h(xi, yi), h(xi + 1, yi), tx);
    let b = lerp(h(xi, yi + 1), h(xi + 1, yi + 1), tx);
    lerp(a, b, ty)
}

/// Ruido de valores 3D.
fn value_noise3(p: Vec3, salt: u64) -> f32 {
    let f = p.floor();
    let t = p - f;
    let (tx, ty, tz) = (smooth(t.x), smooth(t.y), smooth(t.z));
    let (xi, yi, zi) = (f.x as i32, f.y as i32, f.z as i32);
    let h = |dx: i32, dy: i32, dz: i32| hash3(xi + dx, yi + dy, zi + dz, salt);
    let x00 = lerp(h(0, 0, 0), h(1, 0, 0), tx);
    let x10 = lerp(h(0, 1, 0), h(1, 1, 0), tx);
    let x01 = lerp(h(0, 0, 1), h(1, 0, 1), tx);
    let x11 = lerp(h(0, 1, 1), h(1, 1, 1), tx);
    lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
}

/// Ruido fractal 3D entre 0 y 1: varias octavas, cada una al doble de frecuencia y la mitad de fuerza.
pub(super) fn fbm3(p: Vec3, octaves: u32, salt: u64) -> f32 {
    let (mut sum, mut amp, mut norm, mut freq) = (0.0f32, 1.0f32, 0.0f32, 1.0f32);
    for o in 0..octaves {
        sum += value_noise3(p * freq, salt.wrapping_add(u64::from(o) * 7919)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Angulo al azar (radianes, de -pi a pi) del toque numero `dab`.
pub(super) fn dab_angle(dab: u32) -> f32 {
    (unit(DAB_ANGLE_SALT.wrapping_add(u64::from(dab))) * 2.0 - 1.0) * PI
}
