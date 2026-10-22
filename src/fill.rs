//! Relleno (ESPECIFICACION.md, seccion 8.6, tolerancia 32).
//!
//! Se hace en dos pasos para poder guardar las baldosas de deshacer ANTES de escribir: `plan` calcula que texeles
//! cambian (sin tocar la capa) y `apply` los escribe.
//!
//! * Zona (marca 1): texeles conectados (4 vecinos) con alfa y color parecidos al del texel tocado (tolerancia 32),
//!   dentro de la cobertura UV y de la MISMA isla. No cruza costuras entre islas.
//! * Borde suave (marca 2, solo sin bloqueo alfa): desde el borde de la zona se avanza mientras el alfa siga
//!   subiendo (sin tocar lo ya opaco). En esos texeles el relleno va DEBAJO, asi no queda un anillo sin rellenar
//!   en trazos desvanecidos.

use crate::paint::Rect;
use crate::undo::TILE;
use crate::uvmap::UvMaps;

pub const TOLERANCE: i32 = 32;

pub struct FillPlan {
    /// Por texel: 0 = no se toca, 1 = zona, 2 = borde suave (el relleno va debajo).
    pub mask: Vec<u8>,
    /// Caja de los texeles marcados (extremos incluidos).
    pub rect: Rect,
}

/// Calcula el relleno de un toque en el texel `(x, y)` de `pixels` (RGBA recto). `None` si no hay nada que hacer:
/// texel fuera de la cobertura UV, ya es de ese color opaco, o bloqueo alfa sobre un texel vacio.
pub fn plan(
    pixels: &[u8],
    size: u32,
    x: u32,
    y: u32,
    color: [u8; 3],
    alpha_lock: bool,
    maps: &UvMaps,
) -> Option<FillPlan> {
    let s = size as usize;
    let seed = y as usize * s + x as usize;
    if maps.coverage[seed] == 0 {
        return None;
    }
    let isl = maps.island[seed];
    let p0 = [
        pixels[seed * 4],
        pixels[seed * 4 + 1],
        pixels[seed * 4 + 2],
        pixels[seed * 4 + 3],
    ];
    if p0[3] == 255 && p0[0] == color[0] && p0[1] == color[1] && p0[2] == color[2] {
        return None;
    }
    if alpha_lock && p0[3] == 0 {
        return None;
    }

    let near = |a: u8, b: u8| (a as i32 - b as i32).abs() <= TOLERANCE;
    // Texel que entra en la zona: dentro de la cobertura, de la isla y parecido al inicial.
    let ok = |i: usize| {
        if maps.coverage[i] == 0 || maps.island[i] != isl {
            return false;
        }
        let q = &pixels[i * 4..i * 4 + 4];
        near(q[3], p0[3])
            && (p0[3] == 0 || (near(q[0], p0[0]) && near(q[1], p0[1]) && near(q[2], p0[2])))
    };

    let mut mask = vec![0u8; s * s];
    let mut bbox = (x, y, x, y);
    let grow = |bbox: &mut Rect, px: usize, py: usize| {
        let (px, py) = (px as u32, py as u32);
        *bbox = (
            bbox.0.min(px),
            bbox.1.min(py),
            bbox.2.max(px),
            bbox.3.max(py),
        );
    };

    // Zona: relleno por franjas horizontales con pila de semillas.
    let mut stack = vec![(x as usize, y as usize)];
    while let Some((sx, sy)) = stack.pop() {
        let row = sy * s;
        if mask[row + sx] != 0 || !ok(row + sx) {
            continue;
        }
        let mut l = sx;
        while l > 0 && mask[row + l - 1] == 0 && ok(row + l - 1) {
            l -= 1;
        }
        let mut r = sx;
        while r + 1 < s && mask[row + r + 1] == 0 && ok(row + r + 1) {
            r += 1;
        }
        for xx in l..=r {
            mask[row + xx] = 1;
        }
        grow(&mut bbox, l, sy);
        grow(&mut bbox, r, sy);
        for ny in [sy.wrapping_sub(1), sy + 1] {
            if ny >= s {
                continue;
            }
            let nrow = ny * s;
            let mut prev_ok = false;
            for xx in l..=r {
                let can = mask[nrow + xx] == 0 && ok(nrow + xx);
                if can && !prev_ok {
                    stack.push((xx, ny));
                }
                prev_ok = can;
            }
        }
    }

    // Borde suave: desde la zona, mientras el alfa siga subiendo y no sea ya opaco.
    if !alpha_lock {
        let mut queue: Vec<usize> = Vec::new();
        for yy in bbox.1..=bbox.3 {
            for xx in bbox.0..=bbox.2 {
                let i = yy as usize * s + xx as usize;
                if mask[i] == 1 {
                    queue.push(i);
                }
            }
        }
        while let Some(cur) = queue.pop() {
            let (cx, cy) = (cur % s, cur / s);
            let cur_a = pixels[cur * 4 + 3];
            let mut neighbors = [usize::MAX; 4];
            if cx > 0 {
                neighbors[0] = cur - 1;
            }
            if cx + 1 < s {
                neighbors[1] = cur + 1;
            }
            if cy > 0 {
                neighbors[2] = cur - s;
            }
            if cy + 1 < s {
                neighbors[3] = cur + s;
            }
            for m in neighbors {
                if m == usize::MAX || mask[m] != 0 || maps.coverage[m] == 0 || maps.island[m] != isl
                {
                    continue;
                }
                let a = pixels[m * 4 + 3];
                if a > cur_a && a < 255 {
                    mask[m] = 2;
                    grow(&mut bbox, m % s, m / s);
                    queue.push(m);
                }
            }
        }
    }

    Some(FillPlan { mask, rect: bbox })
}

/// Rectangulos de un texel, uno por cada baldosa de deshacer que el relleno va a tocar (para `save_tiles`).
pub fn touched_tiles(plan: &FillPlan, size: u32) -> Vec<Rect> {
    let (x0, y0, x1, y1) = plan.rect;
    let mut out = Vec::new();
    for ty in y0 / TILE..=y1 / TILE {
        for tx in x0 / TILE..=x1 / TILE {
            let (a, b) = (tx * TILE, ty * TILE);
            let (c, d) = ((a + TILE).min(size) - 1, (b + TILE).min(size) - 1);
            let any = (b..=d).any(|y| (a..=c).any(|x| plan.mask[(y * size + x) as usize] != 0));
            if any {
                out.push((a, b, a, b));
            }
        }
    }
    out
}

/// Escribe el relleno en la capa. Con bloqueo alfa solo cambia el color donde ya hay algo.
pub fn apply(pixels: &mut [u8], plan: &FillPlan, size: u32, color: [u8; 3], alpha_lock: bool) {
    let (x0, y0, x1, y1) = plan.rect;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let idx = (y * size + x) as usize;
            let i = idx * 4;
            match plan.mask[idx] {
                1 => {
                    if alpha_lock {
                        if pixels[i + 3] == 0 {
                            continue;
                        }
                        pixels[i..i + 3].copy_from_slice(&color);
                    } else {
                        pixels[i..i + 3].copy_from_slice(&color);
                        pixels[i + 3] = 255;
                    }
                }
                2 => {
                    // El relleno va debajo: rgb = viejo * ea + color * (1 - ea), alfa = 255.
                    let ea = pixels[i + 3] as f32 / 255.0;
                    for k in 0..3 {
                        let v = pixels[i + k] as f32 * ea + color[k] as f32 * (1.0 - ea);
                        pixels[i + k] = v.round().clamp(0.0, 255.0) as u8;
                    }
                    pixels[i + 3] = 255;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: u32 = 64;

    fn maps_all_one() -> UvMaps {
        let n = (N * N) as usize;
        UvMaps {
            size: N,
            island: vec![1; n],
            coverage: vec![1; n],
        }
    }

    fn blank() -> Vec<u8> {
        vec![0; (N * N * 4) as usize]
    }

    fn set(p: &mut [u8], x: u32, y: u32, rgba: [u8; 4]) {
        let i = ((y * N + x) * 4) as usize;
        p[i..i + 4].copy_from_slice(&rgba);
    }

    fn get(p: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * N + x) * 4) as usize;
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    fn run(p: &mut [u8], x: u32, y: u32, color: [u8; 3], lock: bool, maps: &UvMaps) -> bool {
        match plan(p, N, x, y, color, lock, maps) {
            Some(pl) => {
                apply(p, &pl, N, color, lock);
                true
            }
            None => false,
        }
    }

    #[test]
    fn rellena_dentro_de_un_contorno_cerrado_y_no_fuera() {
        let mut p = blank();
        for i in 20..=40 {
            for (x, y) in [(i, 20), (i, 40), (20, i), (40, i)] {
                set(&mut p, x, y, [0, 0, 0, 255]);
            }
        }
        assert!(run(&mut p, 30, 30, [255, 0, 0], false, &maps_all_one()));
        assert_eq!(get(&p, 30, 30), [255, 0, 0, 255]);
        assert_eq!(get(&p, 21, 21), [255, 0, 0, 255]);
        assert_eq!(get(&p, 10, 10), [0, 0, 0, 0]);
        assert_eq!(get(&p, 20, 30), [0, 0, 0, 255]);
    }

    #[test]
    fn no_hace_nada_si_ya_es_de_ese_color_opaco() {
        let mut p = blank();
        set(&mut p, 5, 5, [9, 9, 9, 255]);
        assert!(!run(&mut p, 5, 5, [9, 9, 9], false, &maps_all_one()));
    }

    #[test]
    fn con_bloqueo_alfa_no_rellena_texeles_vacios_y_conserva_el_alfa() {
        let maps = maps_all_one();
        let mut p = blank();
        assert!(!run(&mut p, 5, 5, [255, 0, 0], true, &maps));
        // Una zona semitransparente: se cambia el color y el alfa se queda.
        for y in 0..N {
            for x in 0..N {
                set(&mut p, x, y, [10, 10, 10, 128]);
            }
        }
        assert!(run(&mut p, 5, 5, [255, 0, 0], true, &maps));
        assert_eq!(get(&p, 5, 5), [255, 0, 0, 128]);
    }

    #[test]
    fn no_cruza_a_otra_isla_ni_sale_de_la_cobertura() {
        let n = (N * N) as usize;
        let mut maps = UvMaps {
            size: N,
            island: vec![1; n],
            coverage: vec![1; n],
        };
        for y in 0..N {
            for x in 32..N {
                maps.island[(y * N + x) as usize] = 2;
            }
        }
        // Los ultimos 4 texeles de cada fila quedan fuera de la cobertura.
        for y in 0..N {
            for x in 60..N {
                maps.coverage[(y * N + x) as usize] = 0;
            }
        }
        let mut p = blank();
        assert!(run(&mut p, 5, 5, [255, 0, 0], false, &maps));
        assert_eq!(get(&p, 31, 63), [255, 0, 0, 255]);
        assert_eq!(get(&p, 32, 5), [0, 0, 0, 0]);
        // Un texel fuera de la cobertura no se puede rellenar.
        assert!(!run(&mut p, 62, 5, [255, 0, 0], false, &maps));
    }

    #[test]
    fn el_borde_semitransparente_se_rellena_por_debajo() {
        // Barra vertical de rojo con bordes suaves: x=30 (alfa 128), 31-32 (255), 33 (128); el resto transparente.
        let mut p = blank();
        for y in 0..N {
            set(&mut p, 30, y, [255, 0, 0, 128]);
            set(&mut p, 31, y, [255, 0, 0, 255]);
            set(&mut p, 32, y, [255, 0, 0, 255]);
            set(&mut p, 33, y, [255, 0, 0, 128]);
        }
        assert!(run(&mut p, 5, 5, [0, 0, 255], false, &maps_all_one()));
        assert_eq!(get(&p, 5, 5), [0, 0, 255, 255]);
        let b = get(&p, 30, 5);
        // 50 % de rojo viejo + 50 % de azul nuevo, y ya opaco.
        assert_eq!(b[3], 255);
        assert!(
            (b[0] as i32 - 128).abs() <= 2 && (b[2] as i32 - 127).abs() <= 2,
            "borde = {b:?}"
        );
        // Lo ya opaco no se toca y el otro lado de la barra queda como estaba.
        assert_eq!(get(&p, 31, 5), [255, 0, 0, 255]);
        assert_eq!(get(&p, 33, 5), [255, 0, 0, 128]);
        assert_eq!(get(&p, 50, 5), [0, 0, 0, 0]);
    }

    #[test]
    fn las_baldosas_tocadas_son_solo_las_que_cambian() {
        let mut p = blank();
        // Una region encerrada en una sola baldosa (0..63 en una textura de 64 es una unica baldosa).
        for i in 10..=20 {
            for (x, y) in [(i, 10), (i, 20), (10, i), (20, i)] {
                set(&mut p, x, y, [0, 0, 0, 255]);
            }
        }
        let pl = plan(&p, N, 15, 15, [1, 2, 3], false, &maps_all_one()).unwrap();
        assert_eq!(touched_tiles(&pl, N).len(), 1);
    }
}
