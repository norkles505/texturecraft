//! Pincel en CPU (ESPECIFICACION.md, secciones 7.1 y 7.2): estado del trazo + un toque `SOFT`, modo Normal.
//! Los pixeles de la capa los pone quien llama; el estado del trazo se comparte entre capas (una pieza = un trazo
//! a la vez), como `strokeAlpha`/`strokeBase`/`strokeLastDab` de la app Android.
//!
//! Dentro de una misma pasada la opacidad es el MAXIMO de los toques (no se acumula); si el trazo se va y vuelve
//! (hueco de mas de 3 toques) la segunda pasada si suma.

/// Lado de la textura de cada pieza (fijo en la app Android).
pub const TEX_SIZE: u32 = 1024;

/// Un hueco de mas de tantos toques seguidos sin tocar un texel empieza una pasada nueva en ese texel.
const PASS_GAP: u32 = 3;

pub type Rect = (u32, u32, u32, u32);

/// Opciones de la capa donde se pinta (seccion 4.2): con bloqueo alfa solo cambia el color de lo que ya tiene algo.
/// `erase` convierte el toque en borrador (seccion 7.2): baja el alfa y no toca el color.
#[derive(Clone, Copy, Default)]
pub struct DabOptions {
    pub alpha_lock: bool,
    pub erase: bool,
}

pub struct StrokeState {
    pub size: u32,
    /// Mayor opacidad aplicada a cada texel en la pasada actual (0 = sin tocar).
    stroke_alpha: Vec<f32>,
    /// Color de la capa ANTES del trazo.
    stroke_base: Vec<u8>,
    /// Ultimo toque que paso por cada texel.
    stroke_last: Vec<u32>,
    stroke_dab: u32,
    /// Rectangulo tocado por el trazo (para limpiar `stroke_alpha` sin recorrer todo).
    touched: Option<Rect>,
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    match a {
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        None => b,
    }
}

/// Caja de texeles que tocara un toque en `(u, v)` (extremos incluidos). Sirve para guardar las baldosas de
/// deshacer ANTES de modificar nada.
pub fn dab_box(size: u32, u: f32, v: f32, radius: f32) -> Rect {
    let max = (size - 1) as f32;
    let (u, v) = (u - u.floor(), v - v.floor());
    let (cx, cy) = (u * max, v * max);
    (
        (cx - radius).floor().max(0.0) as u32,
        (cy - radius).floor().max(0.0) as u32,
        (cx + radius).ceil().min(max) as u32,
        (cy + radius).ceil().min(max) as u32,
    )
}

impl StrokeState {
    pub fn new(size: u32) -> StrokeState {
        let n = (size * size) as usize;
        StrokeState {
            size,
            stroke_alpha: vec![0.0; n],
            stroke_base: vec![0; n * 4],
            stroke_last: vec![0; n],
            stroke_dab: 0,
            touched: None,
        }
    }

    /// Al apoyar el boton: limpia el estado del trazo anterior (solo donde se toco).
    pub fn begin_stroke(&mut self) {
        if let Some((x0, y0, x1, y1)) = self.touched.take() {
            for y in y0..=y1 {
                let a = (y * self.size + x0) as usize;
                let b = (y * self.size + x1) as usize;
                self.stroke_alpha[a..=b].fill(0.0);
            }
        }
    }

    /// Un toque de pincel suave en `(u, v)` sobre `pixels` (RGBA recto de la capa): `falloff = min(1, 2t)` con
    /// `t = 1 - d/radio`. Devuelve el rectangulo cambiado `(x0, y0, x1, y1)`, ambos extremos incluidos.
    pub fn dab(
        &mut self,
        pixels: &mut [u8],
        u: f32,
        v: f32,
        radius: f32,
        color: [u8; 3],
        opacity: f32,
        opts: DabOptions,
    ) -> Option<Rect> {
        let max = (self.size - 1) as f32;
        // La textura se repite fuera de 0..1.
        let (u, v) = (u - u.floor(), v - v.floor());
        let (cx, cy) = (u * max, v * max);
        let (x0, y0, x1, y1) = dab_box(self.size, u, v, radius);
        self.stroke_dab += 1;
        let mut changed = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                if d >= radius {
                    continue;
                }
                let t = 1.0 - d / radius;
                let dab_alpha = (2.0 * t).min(1.0) * opacity;
                if dab_alpha <= 0.0 {
                    continue;
                }
                let idx = (y * self.size + x) as usize;
                let mut prev = self.stroke_alpha[idx];
                // El trazo se fue y volvio: pasada nueva.
                if prev > 0.0 && self.stroke_dab - self.stroke_last[idx] > PASS_GAP {
                    prev = 0.0;
                }
                self.stroke_last[idx] = self.stroke_dab;
                if dab_alpha <= prev {
                    continue;
                }
                let i = idx * 4;
                if prev == 0.0 {
                    self.stroke_base[i..i + 4].copy_from_slice(&pixels[i..i + 4]);
                }
                self.stroke_alpha[idx] = dab_alpha;
                // Siempre se mezcla contra el color de ANTES del trazo.
                let base_a = self.stroke_base[i + 3] as f32 / 255.0;
                if opts.erase {
                    // Borrar: alfa = alfaBase * (1 - dabAlpha); el color no cambia. Con bloqueo alfa no se borra
                    // (borrar cambia el alfa).
                    if opts.alpha_lock {
                        continue;
                    }
                    pixels[i + 3] = (base_a * (1.0 - dab_alpha) * 255.0 + 0.5) as u8;
                    changed = true;
                    continue;
                }
                if opts.alpha_lock {
                    // Bloqueo alfa: si no habia nada, no se hace nada; si no, solo cambia el color.
                    if self.stroke_base[i + 3] == 0 {
                        continue;
                    }
                    for k in 0..3 {
                        let old = self.stroke_base[i + k] as f32;
                        let c = old + (color[k] as f32 - old) * dab_alpha;
                        pixels[i + k] = c.round().clamp(0.0, 255.0) as u8;
                    }
                    changed = true;
                    continue;
                }
                let keep = base_a * (1.0 - dab_alpha);
                let out_a = dab_alpha + keep;
                for k in 0..3 {
                    let old = self.stroke_base[i + k] as f32;
                    let c = (color[k] as f32 * dab_alpha + old * keep) / out_a;
                    pixels[i + k] = c.round().clamp(0.0, 255.0) as u8;
                }
                pixels[i + 3] = (out_a * 255.0 + 0.5).min(255.0) as u8;
                changed = true;
            }
        }
        // El rectangulo del trazo incluye toda la caja (stroke_alpha pudo cambiar aunque no cambie el pixel).
        self.touched = Some(union(self.touched, (x0, y0, x1, y1)));
        changed.then_some((x0, y0, x1, y1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(size: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.iter().copied().cycle().take((size * size * 4) as usize).collect()
    }

    fn px(p: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    #[test]
    fn el_borrador_baja_el_alfa_en_el_centro_y_no_toca_fuera() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [200, 10, 10, 255]);
        s.begin_stroke();
        // Centro en el texel (32, 32): u = v = 32/63.
        let c = 32.0 / 63.0;
        let r = s.dab(&mut pixels, c, c, 10.0, [0, 0, 0], 1.0, DabOptions { alpha_lock: false, erase: true });
        assert!(r.is_some());
        assert_eq!(px(&pixels, 64, 32, 32), [200, 10, 10, 0]);
        // Fuera del radio no cambia nada.
        assert_eq!(px(&pixels, 64, 5, 5), [200, 10, 10, 255]);
    }

    #[test]
    fn el_borrador_no_borra_con_bloqueo_alfa() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [200, 10, 10, 255]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        s.dab(&mut pixels, c, c, 10.0, [0, 0, 0], 1.0, DabOptions { alpha_lock: true, erase: true });
        assert_eq!(px(&pixels, 64, 32, 32), [200, 10, 10, 255]);
    }

    #[test]
    fn la_opacidad_no_se_acumula_dentro_de_un_trazo() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        let opts = DabOptions::default();
        // Varios toques seguidos en el mismo punto con opacidad 0.5: el alfa se queda en ~0.5, no sube.
        for _ in 0..5 {
            s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        }
        let a = px(&pixels, 64, 32, 32)[3];
        assert!((a as i32 - 128).abs() <= 1, "alfa = {a}");
    }

    #[test]
    fn un_trazo_nuevo_si_suma_sobre_el_anterior() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        let c = 32.0 / 63.0;
        let opts = DabOptions::default();
        s.begin_stroke();
        s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        s.begin_stroke();
        s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        // 0.5 + 0.5 * (1 - 0.5) = 0.75.
        let a = px(&pixels, 64, 32, 32)[3];
        assert!((a as i32 - 191).abs() <= 1, "alfa = {a}");
    }
}
