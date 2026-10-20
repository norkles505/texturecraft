//! La punta de bitmap que comparten los pinceles de grano (tiza, carboncillo, pastel y punteado). Cada pincel arma
//! su propia punta en su archivo; aqui solo esta el bitmap y como se lee.

use super::azar::lerp;

/// Lado del bitmap de la punta.
pub(super) const TIP_SIZE: usize = 64;

/// Una punta: `TIP_SIZE` x `TIP_SIZE` valores entre 0 y 1 (1 = pinta del todo).
pub struct Punta {
    pub(super) data: Vec<f32>,
}

impl Punta {
    pub(super) fn new(data: Vec<f32>) -> Punta {
        Punta { data }
    }

    /// Valor de la punta en `(u, v)` (0..1 sobre el bitmap, interpolado). Fuera del bitmap es 0.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return 0.0;
        }
        let n = TIP_SIZE as i32;
        let (fx, fy) = (u * n as f32 - 0.5, v * n as f32 - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= n || y >= n { 0.0 } else { self.data[(y * n + x) as usize] };
        let a = lerp(at(x0, y0), at(x0 + 1, y0), tx);
        let b = lerp(at(x0, y0 + 1), at(x0 + 1, y0 + 1), tx);
        lerp(a, b, ty)
    }
}
