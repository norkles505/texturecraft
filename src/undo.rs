//! Deshacer / rehacer por baldosas (ESPECIFICACION.md, seccion 11). Cada capa se divide en baldosas de 64x64
//! texeles; la primera vez que un trazo toca una baldosa se guarda una copia de sus pixeles ANTES de cambiarlos
//! (copia al escribir). Un registro pertenece a UNA sola capa. Aplicar un registro INTERCAMBIA los pixeles actuales
//! con los guardados, asi que el mismo registro sirve para rehacer.
//!
//! Por ahora solo trazos (mover capas, limpiar e invertir vendran con esas operaciones).

use std::collections::HashSet;

use crate::layers::LayerStack;
use crate::paint::Rect;

pub const TILE: u32 = 64;
/// Maximo de pasos y de memoria de la pila de deshacer (los mas viejos se descartan).
const MAX_STEPS: usize = 50;
const MAX_BYTES: usize = 64 * 1024 * 1024;

struct Tile {
    tx: u32,
    ty: u32,
    /// Filas de la baldosa, una tras otra (RGBA).
    data: Vec<u8>,
}

struct StrokeEdit {
    layer_id: u32,
    tiles: Vec<Tile>,
}

impl StrokeEdit {
    fn bytes(&self) -> usize {
        self.tiles.iter().map(|t| t.data.len()).sum()
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<StrokeEdit>,
    redo: Vec<StrokeEdit>,
    /// Registro del trazo en curso.
    pending: Option<StrokeEdit>,
    /// Baldosas ya guardadas en el trazo en curso.
    saved: HashSet<(u32, u32)>,
}

/// Rectangulo de texeles de una baldosa (recortado al borde de la textura).
fn tile_rect(size: u32, tx: u32, ty: u32) -> Rect {
    let (x0, y0) = (tx * TILE, ty * TILE);
    (x0, y0, (x0 + TILE).min(size) - 1, (y0 + TILE).min(size) - 1)
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    match a {
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        None => b,
    }
}

/// Intercambia los pixeles de la capa con los guardados. `None` si la capa ya no existe.
fn apply(edit: &mut StrokeEdit, layers: &mut LayerStack) -> Option<Rect> {
    let idx = layers.index_of(edit.layer_id)?;
    let size = layers.size;
    let pixels = &mut layers.layers[idx].pixels;
    let mut changed: Option<Rect> = None;
    for t in edit.tiles.iter_mut() {
        let (x0, y0, x1, y1) = tile_rect(size, t.tx, t.ty);
        let row = ((x1 - x0 + 1) * 4) as usize;
        for (n, y) in (y0..=y1).enumerate() {
            let s = ((y * size + x0) * 4) as usize;
            t.data[n * row..(n + 1) * row].swap_with_slice(&mut pixels[s..s + row]);
        }
        changed = Some(union(changed, (x0, y0, x1, y1)));
    }
    changed
}

impl History {
    /// Al apoyar el boton: empieza el registro de un trazo en la capa dada.
    pub fn begin(&mut self, layer_id: u32) {
        self.pending = Some(StrokeEdit {
            layer_id,
            tiles: Vec::new(),
        });
        self.saved.clear();
    }

    /// Guarda las baldosas que toca `rect` si aun no estan guardadas en este trazo. Hay que llamarla ANTES de pintar.
    pub fn save_tiles(&mut self, pixels: &[u8], size: u32, (x0, y0, x1, y1): Rect) {
        let Some(edit) = self.pending.as_mut() else {
            return;
        };
        for ty in y0 / TILE..=y1 / TILE {
            for tx in x0 / TILE..=x1 / TILE {
                if !self.saved.insert((tx, ty)) {
                    continue;
                }
                let (a, b, c, d) = tile_rect(size, tx, ty);
                let mut data = Vec::with_capacity(((c - a + 1) * (d - b + 1) * 4) as usize);
                for y in b..=d {
                    let s = ((y * size + a) * 4) as usize;
                    let e = ((y * size + c + 1) * 4) as usize;
                    data.extend_from_slice(&pixels[s..e]);
                }
                edit.tiles.push(Tile { tx, ty, data });
            }
        }
    }

    /// Al soltar el boton: el trazo entra al historial (si cambio algo) y se vacia la pila de rehacer.
    pub fn end(&mut self) {
        if let Some(edit) = self.pending.take() {
            if !edit.tiles.is_empty() {
                self.undo.push(edit);
                self.redo.clear();
                self.trim();
            }
        }
        self.saved.clear();
    }

    fn trim(&mut self) {
        while !self.undo.is_empty()
            && (self.undo.len() > MAX_STEPS
                || self.undo.iter().map(StrokeEdit::bytes).sum::<usize>() > MAX_BYTES)
        {
            self.undo.remove(0);
        }
    }

    /// Deshace el ultimo paso aplicable. Devuelve el rectangulo cambiado (hay que recomponerlo y subirlo).
    pub fn undo(&mut self, layers: &mut LayerStack) -> Option<Rect> {
        while let Some(mut edit) = self.undo.pop() {
            // Un paso cuya capa ya no existe se descarta y se prueba con el anterior.
            if let Some(r) = apply(&mut edit, layers) {
                self.redo.push(edit);
                return Some(r);
            }
        }
        None
    }

    pub fn redo(&mut self, layers: &mut LayerStack) -> Option<Rect> {
        while let Some(mut edit) = self.redo.pop() {
            if let Some(r) = apply(&mut edit, layers) {
                self.undo.push(edit);
                return Some(r);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(layers: &LayerStack, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * layers.size + x) * 4) as usize;
        let p = &layers.layers[0].pixels;
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    /// Simula un trazo: guarda baldosas, pinta un pixel y cierra el registro.
    fn stroke(h: &mut History, layers: &mut LayerStack, x: u32, y: u32, rgba: [u8; 4]) {
        let id = layers.layers[0].id;
        h.begin(id);
        let size = layers.size;
        h.save_tiles(&layers.layers[0].pixels, size, (x, y, x, y));
        let i = ((y * size + x) * 4) as usize;
        layers.layers[0].pixels[i..i + 4].copy_from_slice(&rgba);
        h.end();
    }

    #[test]
    fn deshacer_y_rehacer_un_trazo() {
        let mut layers = LayerStack::new(128);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 70, 5, [255, 0, 0, 255]);
        assert_eq!(px(&layers, 70, 5), [255, 0, 0, 255]);
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(px(&layers, 70, 5), [0, 0, 0, 0]);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(px(&layers, 70, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn varios_pasos_y_trazo_nuevo_vacia_el_rehacer() {
        let mut layers = LayerStack::new(128);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 1, 1, [1, 1, 1, 255]);
        stroke(&mut h, &mut layers, 2, 2, [2, 2, 2, 255]);
        h.undo(&mut layers);
        h.undo(&mut layers);
        assert_eq!(px(&layers, 1, 1), [0, 0, 0, 0]);
        assert!(h.undo(&mut layers).is_none());
        h.redo(&mut layers);
        // Un trazo nuevo vacia el rehacer.
        stroke(&mut h, &mut layers, 3, 3, [3, 3, 3, 255]);
        assert!(h.redo(&mut layers).is_none());
        assert_eq!(px(&layers, 1, 1), [1, 1, 1, 255]);
    }

    #[test]
    fn trazo_sin_cambios_no_entra_al_historial() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        h.begin(layers.layers[0].id);
        h.end();
        assert!(h.undo(&mut layers).is_none());
    }

    #[test]
    fn limite_de_pasos() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        for i in 0..(MAX_STEPS as u32 + 10) {
            stroke(&mut h, &mut layers, i % 64, 0, [9, 9, 9, 255]);
        }
        let mut n = 0;
        while h.undo(&mut layers).is_some() {
            n += 1;
        }
        assert_eq!(n, MAX_STEPS);
    }
}
