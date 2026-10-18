//! Modos de mezcla y la formula "over" (ESPECIFICACION.md, seccion 6). Todo en enteros 0..255 salvo donde se
//! indica (soft light y los modos no separables van en float). Alfa recto (sin premultiplicar).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Darken,
    Multiply,
    ColorBurn,
    LinearBurn,
    DarkerColor,
    Lighten,
    Screen,
    ColorDodge,
    LinearDodge,
    Add,
    LighterColor,
    Overlay,
    SoftLight,
    HardLight,
    VividLight,
    LinearLight,
    PinLight,
    HardMix,
    Invert,
    Difference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// En el orden de la especificacion (Oscurecer, Aclarar, Contraste, Diferencia, Color).
    pub const ALL: [BlendMode; 28] = [
        BlendMode::Normal,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::LinearDodge,
        BlendMode::Add,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::VividLight,
        BlendMode::LinearLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Invert,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];

    /// Nombre que se guarda en el proyecto (los del enum de la app Android).
    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "NORMAL",
            BlendMode::Darken => "DARKEN",
            BlendMode::Multiply => "MULTIPLY",
            BlendMode::ColorBurn => "COLOR_BURN",
            BlendMode::LinearBurn => "LINEAR_BURN",
            BlendMode::DarkerColor => "DARKER_COLOR",
            BlendMode::Lighten => "LIGHTEN",
            BlendMode::Screen => "SCREEN",
            BlendMode::ColorDodge => "COLOR_DODGE",
            BlendMode::LinearDodge => "LINEAR_DODGE",
            BlendMode::Add => "ADD",
            BlendMode::LighterColor => "LIGHTER_COLOR",
            BlendMode::Overlay => "OVERLAY",
            BlendMode::SoftLight => "SOFT_LIGHT",
            BlendMode::HardLight => "HARD_LIGHT",
            BlendMode::VividLight => "VIVID_LIGHT",
            BlendMode::LinearLight => "LINEAR_LIGHT",
            BlendMode::PinLight => "PIN_LIGHT",
            BlendMode::HardMix => "HARD_MIX",
            BlendMode::Invert => "INVERT",
            BlendMode::Difference => "DIFFERENCE",
            BlendMode::Exclusion => "EXCLUSION",
            BlendMode::Subtract => "SUBTRACT",
            BlendMode::Divide => "DIVIDE",
            BlendMode::Hue => "HUE",
            BlendMode::Saturation => "SATURATION",
            BlendMode::Color => "COLOR",
            BlendMode::Luminosity => "LUMINOSITY",
        }
    }

    /// Un nombre desconocido cae en `Normal` (como al cargar un proyecto).
    pub fn from_name(name: &str) -> BlendMode {
        BlendMode::ALL.iter().copied().find(|m| m.name() == name).unwrap_or(BlendMode::Normal)
    }
}

fn burn(b: i32, s: i32) -> i32 {
    if b >= 255 {
        255
    } else if s <= 0 {
        0
    } else {
        255 - 255.min((255 - b) * 255 / s)
    }
}

fn dodge(b: i32, s: i32) -> i32 {
    if b <= 0 {
        0
    } else if s >= 255 {
        255
    } else {
        255.min(b * 255 / (255 - s))
    }
}

fn hard_light(b: i32, s: i32) -> i32 {
    if s < 128 {
        b * (2 * s) / 255
    } else {
        let s2 = 2 * s - 255;
        b + s2 - b * s2 / 255
    }
}

fn soft_light(b: i32, s: i32) -> i32 {
    let cb = b as f32 / 255.0;
    let cs = s as f32 / 255.0;
    let r = if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
        cb + (2.0 * cs - 1.0) * (d - cb)
    };
    ((r * 255.0 + 0.5) as i32).clamp(0, 255)
}

/// Formula por canal de los modos separables (`b` = fondo, `s` = fuente). `None` si el modo no es separable.
fn channel(mode: BlendMode, b: i32, s: i32) -> Option<i32> {
    use BlendMode::*;
    Some(match mode {
        Normal => s,
        Darken => b.min(s),
        Multiply => b * s / 255,
        ColorBurn => burn(b, s),
        LinearBurn => (b + s - 255).max(0),
        Lighten => b.max(s),
        Screen => b + s - b * s / 255,
        ColorDodge => dodge(b, s),
        LinearDodge | Add => 255.min(b + s),
        Overlay => hard_light(s, b),
        SoftLight => soft_light(b, s),
        HardLight => hard_light(b, s),
        VividLight => {
            if s < 128 {
                burn(b, 2 * s)
            } else {
                dodge(b, 2 * (s - 128))
            }
        }
        LinearLight => (b + 2 * s - 255).clamp(0, 255),
        PinLight => {
            if s < 128 {
                b.min(2 * s)
            } else {
                b.max(2 * s - 255)
            }
        }
        HardMix => {
            if b + s >= 255 {
                255
            } else {
                0
            }
        }
        Invert => 255 - b,
        Difference => (b - s).abs(),
        Exclusion => b + s - 2 * b * s / 255,
        Subtract => (b - s).max(0),
        Divide => {
            if s == 0 {
                255
            } else {
                255.min(b * 255 / s)
            }
        }
        DarkerColor | LighterColor | Hue | Saturation | Color | Luminosity => return None,
    })
}

fn luma(c: [i32; 3]) -> i32 {
    77 * c[0] + 151 * c[1] + 28 * c[2]
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn clip(mut c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    // Las guardas evitan dividir entre ~0 (grises casi exactos) y producir NaN.
    if n < 0.0 && l - n > 1e-9 {
        for v in c.iter_mut() {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1.0 && x - l > 1e-9 {
        for v in c.iter_mut() {
            *v = l + (*v - l) * (1.0 - l) / (x - l);
        }
    }
    c
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip([c[0] + d, c[1] + d, c[2] + d])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    // Indices ordenados de menor a mayor valor.
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&a, &b| c[a].partial_cmp(&c[b]).unwrap_or(std::cmp::Ordering::Equal));
    let (imin, imid, imax) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0f32; 3];
    if c[imax] > c[imin] {
        out[imid] = (c[imid] - c[imin]) * s / (c[imax] - c[imin]);
        out[imax] = s;
    }
    out
}

fn to_f(c: [u8; 3]) -> [f32; 3] {
    [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
}

fn from_f(c: [f32; 3]) -> [u8; 3] {
    [
        (c[0] * 255.0 + 0.5).clamp(0.0, 255.0) as u8,
        (c[1] * 255.0 + 0.5).clamp(0.0, 255.0) as u8,
        (c[2] * 255.0 + 0.5).clamp(0.0, 255.0) as u8,
    ]
}

/// Color mezclado de `src` sobre `dst` con el modo dado (sin alfa).
pub fn mix(mode: BlendMode, dst: [u8; 3], src: [u8; 3]) -> [u8; 3] {
    let d = [dst[0] as i32, dst[1] as i32, dst[2] as i32];
    let s = [src[0] as i32, src[1] as i32, src[2] as i32];
    if let (Some(r), Some(g), Some(b)) = (channel(mode, d[0], s[0]), channel(mode, d[1], s[1]), channel(mode, d[2], s[2])) {
        return [r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8];
    }
    match mode {
        BlendMode::DarkerColor => {
            if luma(s) < luma(d) {
                src
            } else {
                dst
            }
        }
        BlendMode::LighterColor => {
            if luma(s) > luma(d) {
                src
            } else {
                dst
            }
        }
        BlendMode::Hue => from_f(set_lum(set_sat(to_f(src), sat(to_f(dst))), lum(to_f(dst)))),
        BlendMode::Saturation => from_f(set_lum(set_sat(to_f(dst), sat(to_f(src))), lum(to_f(dst)))),
        BlendMode::Color => from_f(set_lum(to_f(src), lum(to_f(dst)))),
        BlendMode::Luminosity => from_f(set_lum(to_f(dst), lum(to_f(src)))),
        _ => src,
    }
}

/// "Over" con alfa recto (seccion 6.3). `dst` = `[r, g, b, a]`, `src` = color de la fuente y `sa` su alfa (0..255)
/// ya multiplicado por la opacidad de la capa. Devuelve `[r, g, b, a]`.
pub fn over(dst: [u8; 4], src: [u8; 3], sa: u32, mode: BlendMode) -> [u8; 4] {
    let da = dst[3] as u32;
    let mut c = [src[0] as u32, src[1] as u32, src[2] as u32];
    if mode != BlendMode::Normal && da > 0 {
        let m = mix(mode, [dst[0], dst[1], dst[2]], src);
        for k in 0..3 {
            c[k] = (src[k] as u32 * (255 - da) + m[k] as u32 * da) / 255;
        }
    }
    if da == 0 {
        return [c[0] as u8, c[1] as u8, c[2] as u8, sa.min(255) as u8];
    }
    let keep = da * (255 - sa) / 255;
    let out_a = sa + keep;
    if out_a == 0 {
        return [0, 0, 0, 0];
    }
    let mut out = [0u8, 0, 0, out_a.min(255) as u8];
    for k in 0..3 {
        out[k] = ((c[k] * sa + dst[k] as u32 * keep) / out_a).min(255) as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_ida_y_vuelta() {
        for m in BlendMode::ALL {
            assert_eq!(BlendMode::from_name(m.name()), m);
        }
        assert_eq!(BlendMode::from_name("NO_EXISTE"), BlendMode::Normal);
    }

    #[test]
    fn multiplicar_y_trama() {
        assert_eq!(mix(BlendMode::Multiply, [255, 128, 0], [128, 128, 128]), [128, 64, 0]);
        assert_eq!(mix(BlendMode::Screen, [0, 0, 0], [10, 20, 30]), [10, 20, 30]);
        assert_eq!(mix(BlendMode::Difference, [200, 50, 0], [50, 200, 0]), [150, 150, 0]);
        assert_eq!(mix(BlendMode::HardMix, [200, 100, 0], [100, 100, 0]), [255, 0, 0]);
        assert_eq!(mix(BlendMode::Invert, [10, 255, 0], [0, 0, 0]), [245, 0, 255]);
    }

    #[test]
    fn luminosidad_de_gris_sobre_color_mantiene_gris_en_su_brillo() {
        // Un gris no tiene saturacion: Color sobre fondo blanco con fuente roja da el brillo del fondo.
        let r = mix(BlendMode::Color, [255, 255, 255], [255, 0, 0]);
        assert_eq!(r, [255, 255, 255]);
        // Luminosidad de fuente negra sobre un gris da negro.
        assert_eq!(mix(BlendMode::Luminosity, [128, 128, 128], [0, 0, 0]), [0, 0, 0]);
    }

    #[test]
    fn over_normal() {
        // Fondo transparente: la capa queda tal cual.
        assert_eq!(over([0, 0, 0, 0], [10, 20, 30], 200, BlendMode::Normal), [10, 20, 30, 200]);
        // Fuente opaca tapa todo.
        assert_eq!(over([5, 5, 5, 255], [100, 110, 120], 255, BlendMode::Normal), [100, 110, 120, 255]);
        // Fuente transparente deja el fondo.
        assert_eq!(over([5, 6, 7, 255], [100, 110, 120], 0, BlendMode::Normal), [5, 6, 7, 255]);
    }

    #[test]
    fn over_con_modo_sobre_fondo_transparente_no_mezcla() {
        // Donde el fondo no tiene nada, Multiplicar no oscurece.
        assert_eq!(over([0, 0, 0, 0], [200, 100, 50], 255, BlendMode::Multiply), [200, 100, 50, 255]);
    }
}
