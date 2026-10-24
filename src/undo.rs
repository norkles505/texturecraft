//! Deshacer / rehacer por baldosas (ESPECIFICACION.md, seccion 11). Cada capa se divide en baldosas de 64x64
//! texeles; la primera vez que un trazo toca una baldosa se guarda una copia de sus pixeles ANTES de cambiarlos
//! (copia al escribir). Un registro pertenece a UNA sola capa. Aplicar un registro INTERCAMBIA los pixeles actuales
//! con los guardados, asi que el mismo registro sirve para rehacer.
//!
//! Por ahora trazos y "limpiar capa" (mover capas e invertir vendran con esas operaciones).

use std::collections::HashSet;

use crate::layers::{LayerStack, PaintTarget};
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
    /// Donde fue el trazo: los pixeles de la capa, su mascara o la hoja de un canal.
    target: PaintTarget,
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

/// Intercambia los pixeles de la capa (o de su mascara) con los guardados. `None` si la capa o su mascara ya no existen.
fn apply(edit: &mut StrokeEdit, layers: &mut LayerStack) -> Option<Rect> {
    let idx = layers.index_of(edit.layer_id)?;
    let size = layers.size;
    let layer = &mut layers.layers[idx];
    let pixels = match edit.target {
        PaintTarget::Mask => &mut layer.mask.as_mut()?.pixels,
        PaintTarget::Channel(s) => layer.chans[s].as_mut()?,
        PaintTarget::Layer => &mut layer.pixels,
    };
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
    /// Al apoyar el boton: empieza el registro de un trazo en la capa dada y en la hoja `target`.
    pub fn begin(&mut self, layer_id: u32, target: PaintTarget) {
        self.pending = Some(StrokeEdit {
            layer_id,
            target,
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

    /// Devuelve a `pixels` (los de la capa y hoja del trazo en curso) lo que tenian ANTES del trazo en todas las baldosas
    /// que el trazo ya toco, y devuelve el rectangulo que cubren (`None` si no hay trazo en curso o aun no toco nada).
    /// No cierra el registro: el trazo puede seguir pintando (o repintarse, como el desvanecido del final en `main.rs`).
    pub fn restore_pending(&self, pixels: &mut [u8], size: u32) -> Option<Rect> {
        let edit = self.pending.as_ref()?;
        let mut rect: Option<Rect> = None;
        for t in &edit.tiles {
            let (x0, y0, x1, y1) = tile_rect(size, t.tx, t.ty);
            let row = ((x1 - x0 + 1) * 4) as usize;
            for (n, y) in (y0..=y1).enumerate() {
                let s = ((y * size + x0) * 4) as usize;
                pixels[s..s + row].copy_from_slice(&t.data[n * row..(n + 1) * row]);
            }
            rect = Some(union(rect, (x0, y0, x1, y1)));
        }
        rect
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

/// Edita de una vez toda la hoja que se pinta ahora en la capa elegida (sus pixeles, la hoja del canal elegido o su
/// mascara) como un solo paso del historial. `needed` dice si hay algo que cambiar (si no, no se toca nada ni entra al
/// historial) y `edit` lo cambia; los dos reciben los pixeles, la hoja que es y el gris con que empieza una mascara
/// (blanco; negro en una capa de color liso). Devuelve la zona que cambio (toda la textura: hay que recomponerla y
/// subirla) o `None` si no se puede pintar en esa capa (oculta, bloqueada...) o no habia nada que cambiar.
fn edit_whole_sheet(
    layers: &mut LayerStack,
    history: &mut History,
    needed: impl Fn(&[u8], PaintTarget, u8) -> bool,
    edit: impl Fn(&mut [u8], PaintTarget, u8),
) -> Option<Rect> {
    let size = layers.size;
    let channel = layers.channel;
    let layer = &mut layers.layers[layers.selected];
    if !layer.accepts(channel) {
        return None;
    }
    let (id, target) = (layer.id, layer.target(channel));
    let mask_rest = if layer.is_solid() { 0 } else { 255 };
    let pixels = layer.target_pixels_mut(target);
    if !needed(pixels, target, mask_rest) {
        return None;
    }
    let all = (0, 0, size - 1, size - 1);
    history.begin(id, target);
    history.save_tiles(pixels, size, all);
    edit(pixels, target, mask_rest);
    history.end();
    Some(all)
}

/// "Limpiar capa": deja vacio lo que se pinta ahora en la capa elegida (sus pixeles o la hoja del canal elegido) o, si
/// se pinta su mascara, la devuelve a como empezo (blanca; negra en una capa de color liso). Entra al historial como un
/// paso, asi que Ctrl+Z lo trae de vuelta. `None` si no se puede pintar en esa capa o ya estaba limpia.
pub fn clear_selected(layers: &mut LayerStack, history: &mut History) -> Option<Rect> {
    edit_whole_sheet(
        layers,
        history,
        |px, target, rest| match target {
            PaintTarget::Mask => px
                .chunks_exact(4)
                .any(|p| p[0] != rest || p[1] != rest || p[2] != rest),
            _ => px.chunks_exact(4).any(|p| p[3] != 0),
        },
        |px, target, rest| match target {
            PaintTarget::Mask => {
                for p in px.chunks_exact_mut(4) {
                    p.copy_from_slice(&[rest, rest, rest, 255]);
                }
            }
            _ => px.fill(0),
        },
    )
}

/// "Invertir colores": cambia cada color de lo que se pinta ahora por su opuesto (255 - valor en rojo, verde y azul; el
/// alfa no cambia). En una capa o un canal solo donde hay pintura; en una mascara, en todos los texeles (el blanco pasa
/// a negro y al reves). Entra al historial como un paso (Ctrl+Z). `None` si no se puede pintar en esa capa o esta vacia.
pub fn invert_selected(layers: &mut LayerStack, history: &mut History) -> Option<Rect> {
    edit_whole_sheet(
        layers,
        history,
        |px, target, _| target == PaintTarget::Mask || px.chunks_exact(4).any(|p| p[3] != 0),
        |px, target, _| {
            for p in px.chunks_exact_mut(4) {
                if target == PaintTarget::Mask || p[3] != 0 {
                    p[0] = 255 - p[0];
                    p[1] = 255 - p[1];
                    p[2] = 255 - p[2];
                }
            }
        },
    )
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
        h.begin(id, PaintTarget::Layer);
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
        h.begin(layers.layers[0].id, PaintTarget::Layer);
        h.end();
        assert!(h.undo(&mut layers).is_none());
    }

    #[test]
    fn deshacer_un_trazo_en_la_mascara_no_toca_la_capa() {
        let mut layers = LayerStack::new(128);
        layers.add_mask();
        let mut h = History::default();
        let id = layers.layers[0].id;
        let size = layers.size;
        // Trazo en la mascara: un texel pasa de blanco a negro.
        h.begin(id, PaintTarget::Mask);
        let i = ((5 * size + 70) * 4) as usize;
        h.save_tiles(&layers.layers[0].mask.as_ref().unwrap().pixels, size, (70, 5, 70, 5));
        layers.layers[0].mask.as_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        h.end();
        assert!(h.undo(&mut layers).is_some());
        let m = &layers.layers[0].mask.as_ref().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[255, 255, 255, 255]);
        // La capa no se toco.
        assert_eq!(px(&layers, 70, 5), [0, 0, 0, 0]);
        assert!(h.redo(&mut layers).is_some());
        let m = &layers.layers[0].mask.as_ref().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[0, 0, 0, 255]);
        // Si la mascara ya no existe, el paso se descarta sin fallar.
        layers.remove_mask();
        assert!(h.undo(&mut layers).is_none());
    }

    #[test]
    fn deshacer_un_trazo_en_un_canal_no_toca_el_color() {
        let mut layers = LayerStack::new(128);
        let mut h = History::default();
        let id = layers.layers[0].id;
        let size = layers.size;
        let t = PaintTarget::Channel(0);
        h.begin(id, t);
        let i = ((5 * size + 70) * 4) as usize;
        let pixels = layers.layers[0].target_pixels_mut(t);
        h.save_tiles(pixels, size, (70, 5, 70, 5));
        pixels[i..i + 4].copy_from_slice(&[30, 30, 30, 255]);
        h.end();
        assert!(h.undo(&mut layers).is_some());
        let c = layers.layers[0].chans[0].as_ref().unwrap();
        assert_eq!(&c[i..i + 4], &[0, 0, 0, 0]);
        assert_eq!(px(&layers, 70, 5), [0, 0, 0, 0]);
        assert!(h.redo(&mut layers).is_some());
        let c = layers.layers[0].chans[0].as_ref().unwrap();
        assert_eq!(&c[i..i + 4], &[30, 30, 30, 255]);
    }

    #[test]
    fn limpiar_la_capa_se_deshace_y_se_rehace() {
        let mut layers = LayerStack::new(128);
        let mut h = History::default();
        // Dos texeles en baldosas distintas: se limpia toda la capa, no solo una baldosa.
        stroke(&mut h, &mut layers, 5, 5, [255, 0, 0, 255]);
        stroke(&mut h, &mut layers, 100, 100, [0, 255, 0, 255]);
        assert_eq!(clear_selected(&mut layers, &mut h), Some((0, 0, 127, 127)));
        assert_eq!(px(&layers, 5, 5), [0, 0, 0, 0]);
        assert_eq!(px(&layers, 100, 100), [0, 0, 0, 0]);
        // Ctrl+Z trae todo de vuelta y Ctrl+Mayus+Z lo vuelve a limpiar.
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(px(&layers, 5, 5), [255, 0, 0, 255]);
        assert_eq!(px(&layers, 100, 100), [0, 255, 0, 255]);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(px(&layers, 5, 5), [0, 0, 0, 0]);
        assert_eq!(px(&layers, 100, 100), [0, 0, 0, 0]);
    }

    #[test]
    fn limpiar_una_capa_vacia_no_hace_nada_ni_entra_al_historial() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        assert!(clear_selected(&mut layers, &mut h).is_none());
        assert!(h.undo(&mut layers).is_none());
    }

    #[test]
    fn limpiar_no_vale_en_una_capa_bloqueada_ni_oculta() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 5, 5, [255, 0, 0, 255]);
        layers.layers[0].locked = true;
        assert!(clear_selected(&mut layers, &mut h).is_none());
        layers.layers[0].locked = false;
        layers.layers[0].visible = false;
        assert!(clear_selected(&mut layers, &mut h).is_none());
        assert_eq!(px(&layers, 5, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn limpiar_la_mascara_la_deja_como_empezo() {
        let mut layers = LayerStack::new(64);
        layers.add_mask();
        let mut h = History::default();
        let i = ((3 * 64 + 3) * 4) as usize;
        layers.layers[0].mask.as_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        assert!(clear_selected(&mut layers, &mut h).is_some());
        let m = &layers.layers[0].mask.as_ref().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[255, 255, 255, 255]);
        // La capa en si no se toco y la mascara sigue ahi.
        assert_eq!(px(&layers, 3, 3), [0, 0, 0, 0]);
        // Ya estaba limpia: no hay segundo paso.
        assert!(clear_selected(&mut layers, &mut h).is_none());
        assert!(h.undo(&mut layers).is_some());
        let m = &layers.layers[0].mask.as_ref().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn limpiar_la_mascara_de_una_capa_de_color_la_vuelve_negra() {
        let mut layers = LayerStack::new(64);
        layers.add_solid_layer([10, 20, 30]);
        let mut h = History::default();
        let i = ((3 * 64 + 3) * 4) as usize;
        layers.layers[1].mask.as_mut().unwrap().pixels[i..i + 4]
            .copy_from_slice(&[255, 255, 255, 255]);
        assert!(clear_selected(&mut layers, &mut h).is_some());
        let m = &layers.layers[1].mask.as_ref().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn limpiar_un_canal_no_toca_el_color() {
        use crate::layers::ChannelKind;
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 5, 5, [255, 0, 0, 255]);
        let t = PaintTarget::Channel(0);
        let i = ((6 * 64 + 6) * 4) as usize;
        layers.layers[0].target_pixels_mut(t)[i..i + 4].copy_from_slice(&[200, 200, 200, 255]);
        layers.channel = ChannelKind::Roughness;
        assert!(clear_selected(&mut layers, &mut h).is_some());
        assert_eq!(&layers.layers[0].chans[0].as_ref().unwrap()[i..i + 4], &[0, 0, 0, 0]);
        assert_eq!(px(&layers, 5, 5), [255, 0, 0, 255]);
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(&layers.layers[0].chans[0].as_ref().unwrap()[i..i + 4], &[200, 200, 200, 255]);
    }

    #[test]
    fn invertir_cambia_los_colores_pero_no_el_alfa_ni_lo_vacio() {
        let mut layers = LayerStack::new(128);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 5, 5, [255, 0, 0, 255]);
        stroke(&mut h, &mut layers, 100, 100, [10, 20, 30, 100]);
        assert_eq!(invert_selected(&mut layers, &mut h), Some((0, 0, 127, 127)));
        assert_eq!(px(&layers, 5, 5), [0, 255, 255, 255]);
        // El alfa parcial se conserva; solo cambian los colores.
        assert_eq!(px(&layers, 100, 100), [245, 235, 225, 100]);
        // Donde no habia pintura sigue sin haberla (no aparece un negro opaco ni un blanco).
        assert_eq!(px(&layers, 50, 50), [0, 0, 0, 0]);
        // Ctrl+Z y rehacer.
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(px(&layers, 5, 5), [255, 0, 0, 255]);
        assert_eq!(px(&layers, 100, 100), [10, 20, 30, 100]);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(px(&layers, 5, 5), [0, 255, 255, 255]);
    }

    #[test]
    fn invertir_dos_veces_deja_todo_como_estaba() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 5, 5, [12, 200, 77, 255]);
        invert_selected(&mut layers, &mut h);
        invert_selected(&mut layers, &mut h);
        assert_eq!(px(&layers, 5, 5), [12, 200, 77, 255]);
    }

    #[test]
    fn invertir_una_capa_vacia_o_bloqueada_no_hace_nada() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        assert!(invert_selected(&mut layers, &mut h).is_none());
        stroke(&mut h, &mut layers, 5, 5, [255, 0, 0, 255]);
        layers.layers[0].locked = true;
        assert!(invert_selected(&mut layers, &mut h).is_none());
        assert_eq!(px(&layers, 5, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn invertir_la_mascara_cambia_todos_sus_texeles() {
        let mut layers = LayerStack::new(64);
        layers.add_mask();
        let mut h = History::default();
        let i = ((3 * 64 + 3) * 4) as usize;
        layers.layers[0].mask.as_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        assert!(invert_selected(&mut layers, &mut h).is_some());
        let m = &layers.layers[0].mask.as_ref().unwrap().pixels;
        // El negro paso a blanco y el resto (blanco) a negro; sigue siendo opaca.
        assert_eq!(&m[i..i + 4], &[255, 255, 255, 255]);
        assert_eq!(&m[0..4], &[0, 0, 0, 255]);
        assert!(h.undo(&mut layers).is_some());
        let m = &layers.layers[0].mask.as_ref().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[0, 0, 0, 255]);
        assert_eq!(&m[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn invertir_un_canal_no_toca_el_color() {
        use crate::layers::ChannelKind;
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 5, 5, [255, 0, 0, 255]);
        let t = PaintTarget::Channel(0);
        let i = ((6 * 64 + 6) * 4) as usize;
        layers.layers[0].target_pixels_mut(t)[i..i + 4].copy_from_slice(&[200, 200, 200, 255]);
        layers.channel = ChannelKind::Roughness;
        assert!(invert_selected(&mut layers, &mut h).is_some());
        assert_eq!(&layers.layers[0].chans[0].as_ref().unwrap()[i..i + 4], &[55, 55, 55, 255]);
        assert_eq!(px(&layers, 5, 5), [255, 0, 0, 255]);
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

#[cfg(test)]
mod restore_tests {
    use super::*;

    #[test]
    fn restore_pending_devuelve_las_baldosas_tocadas_a_como_estaban_antes_del_trazo() {
        let size = 128;
        let mut pixels = vec![7u8; (size * size * 4) as usize];
        let mut h = History::default();
        // Sin trazo en curso no hay nada que devolver.
        assert_eq!(h.restore_pending(&mut pixels, size), None);
        h.begin(1, PaintTarget::Layer);
        // Aun no toco nada.
        assert_eq!(h.restore_pending(&mut pixels, size), None);
        h.save_tiles(&pixels, size, (10, 10, 20, 20));
        // El trazo pinta en la baldosa (0, 0) y tambien cambia un pixel de otra baldosa (que no guardo).
        for y in 0..TILE {
            for x in 0..TILE {
                let i = ((y * size + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let fuera = (((TILE + 5) * size + 100) * 4) as usize;
        pixels[fuera] = 99;
        let r = h.restore_pending(&mut pixels, size);
        assert_eq!(r, Some((0, 0, TILE - 1, TILE - 1)));
        assert!(pixels[..(TILE * 4) as usize].iter().all(|v| *v == 7));
        let i = (((TILE - 1) * size + (TILE - 1)) * 4) as usize;
        assert_eq!(&pixels[i..i + 4], &[7, 7, 7, 7]);
        // Lo que no estaba en una baldosa guardada no se toca.
        assert_eq!(pixels[fuera], 99);
        // Al cerrar el registro ya no hay nada que devolver.
        h.end();
        assert_eq!(h.restore_pending(&mut pixels, size), None);
    }
}
