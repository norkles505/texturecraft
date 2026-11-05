//! Deshacer / rehacer por baldosas (ESPECIFICACION.md, seccion 11). Cada capa se divide en baldosas de 64x64
//! texeles; la primera vez que un trazo toca una baldosa se guarda una copia de sus pixeles ANTES de cambiarlos
//! (copia al escribir). Un registro pertenece a UNA sola capa. Aplicar un registro INTERCAMBIA los pixeles actuales
//! con los guardados, asi que el mismo registro sirve para rehacer.
//!
//! Por ahora trazos, "limpiar capa", "invertir colores", los cambios de estructura de la pila (crear, duplicar, borrar y
//! mover capas, meterlas o sacarlas de una carpeta; ver `layers::LayoutEdit`), anadir y quitar mascaras
//! (`layers::MaskEdit`) y los ajustes de las capas (opacidad, modo de mezcla, nombre, ojo, candados, recorte, color liso,
//! mascara encendida o invertida; ver `layers::SettingsEdit`). Combinar hacia abajo todavia no se deshace.

use std::collections::HashSet;

use crate::layers::{LayerStack, PaintTarget, SettingsEdit, StackEdit};
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

/// Un paso del historial: un trazo (o una operacion sobre toda la hoja) o un cambio de estructura de la pila de capas.
/// Los dos tipos van en la misma lista, en el orden en que se hicieron.
enum Step {
    Stroke(StrokeEdit),
    /// Cambio de estructura de la lista de capas o mascara anadida o quitada.
    Layout(StackEdit),
    /// Cambio de los ajustes de una o mas capas.
    Settings(SettingsEdit),
}

impl Step {
    fn bytes(&self) -> usize {
        match self {
            Step::Stroke(s) => s.bytes(),
            Step::Layout(l) => l.bytes(),
            Step::Settings(_) => 0,
        }
    }

    /// Intercambia lo guardado con el estado actual (deshacer y rehacer son lo mismo). Devuelve la zona que cambio, o
    /// `None` si el paso ya no se puede aplicar. Un cambio de estructura recompone la textura entera.
    fn apply_to(&mut self, layers: &mut LayerStack) -> Option<Rect> {
        match self {
            Step::Stroke(edit) => apply(edit, layers),
            Step::Layout(edit) => edit.swap(layers).then(|| {
                let last = layers.size - 1;
                (0, 0, last, last)
            }),
            Step::Settings(edit) => edit.swap(layers).then(|| {
                let last = layers.size - 1;
                (0, 0, last, last)
            }),
        }
    }
}

#[derive(Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// Registro del trazo en curso.
    pending: Option<StrokeEdit>,
    /// Baldosas ya guardadas en el trazo en curso.
    saved: HashSet<(u32, u32)>,
    /// Hay un arrastre (de un deslizador, por ejemplo) en curso que esta cambiando ajustes: sus cambios se unen en un solo
    /// paso (ver `record_settings`).
    settings_open: bool,
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
        PaintTarget::Mask(k) => &mut layer.masks.get_mut(k)?.pixels,
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
                self.undo.push(Step::Stroke(edit));
                self.redo.clear();
                self.trim();
            }
        }
        self.saved.clear();
    }

    /// Un cambio de estructura de la pila de capas (los recoge `main` con `LayerStack::take_layout_log`) entra al
    /// historial y se vacia la pila de rehacer. Hay que meterlos ANTES de empezar un trazo o de deshacer, para que
    /// queden en el orden en que ocurrieron.
    pub fn push_layout(&mut self, edit: StackEdit) {
        self.undo.push(Step::Layout(edit));
        self.redo.clear();
        self.settings_open = false;
        self.trim();
    }

    /// Un cambio de ajustes de capas (`layers::settings_diff`, o `None` si en este cuadro no cambio nada) entra al
    /// historial. `drag` dice que el boton del raton sigue apretado: los cambios seguidos de un mismo arrastre sobre las
    /// mismas capas (un deslizador de opacidad, por ejemplo) se unen en un solo paso en vez de uno por cuadro. El paso
    /// guarda como estaba todo al empezar, asi que unir es simplemente no anadir otro.
    pub fn record_settings(&mut self, edit: Option<SettingsEdit>, drag: bool) {
        match edit {
            Some(e) => {
                let merge = drag
                    && self.settings_open
                    && matches!(self.undo.last(), Some(Step::Settings(last)) if last.same_layers(&e));
                if !merge {
                    self.undo.push(Step::Settings(e));
                    self.redo.clear();
                    self.trim();
                }
                self.settings_open = drag;
            }
            None => {
                if !drag {
                    self.settings_open = false;
                }
            }
        }
    }

    fn trim(&mut self) {
        while !self.undo.is_empty()
            && (self.undo.len() > MAX_STEPS
                || self.undo.iter().map(Step::bytes).sum::<usize>() > MAX_BYTES)
        {
            self.undo.remove(0);
        }
    }

    /// Deshace el ultimo paso aplicable. Devuelve el rectangulo cambiado (hay que recomponerlo y subirlo).
    pub fn undo(&mut self, layers: &mut LayerStack) -> Option<Rect> {
        self.settings_open = false;
        while let Some(mut step) = self.undo.pop() {
            // Un paso cuya capa ya no existe se descarta y se prueba con el anterior.
            if let Some(r) = step.apply_to(layers) {
                self.redo.push(step);
                return Some(r);
            }
        }
        None
    }

    pub fn redo(&mut self, layers: &mut LayerStack) -> Option<Rect> {
        self.settings_open = false;
        while let Some(mut step) = self.redo.pop() {
            if let Some(r) = step.apply_to(layers) {
                self.undo.push(step);
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
    let mask_rest = if layer.mask_reveals() { 0 } else { 255 };
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
            PaintTarget::Mask(_) => px
                .chunks_exact(4)
                .any(|p| p[0] != rest || p[1] != rest || p[2] != rest),
            _ => px.chunks_exact(4).any(|p| p[3] != 0),
        },
        |px, target, rest| match target {
            PaintTarget::Mask(_) => {
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
        |px, target, _| {
            matches!(target, PaintTarget::Mask(_)) || px.chunks_exact(4).any(|p| p[3] != 0)
        },
        |px, target, _| {
            let on_mask = matches!(target, PaintTarget::Mask(_));
            for p in px.chunks_exact_mut(4) {
                if on_mask || p[3] != 0 {
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
        h.begin(id, PaintTarget::Mask(0));
        let i = ((5 * size + 70) * 4) as usize;
        h.save_tiles(&layers.layers[0].mask().unwrap().pixels, size, (70, 5, 70, 5));
        layers.layers[0].mask_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        h.end();
        assert!(h.undo(&mut layers).is_some());
        let m = &layers.layers[0].mask().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[255, 255, 255, 255]);
        // La capa no se toco.
        assert_eq!(px(&layers, 70, 5), [0, 0, 0, 0]);
        assert!(h.redo(&mut layers).is_some());
        let m = &layers.layers[0].mask().unwrap().pixels;
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
        layers.layers[0].mask_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        assert!(clear_selected(&mut layers, &mut h).is_some());
        let m = &layers.layers[0].mask().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[255, 255, 255, 255]);
        // La capa en si no se toco y la mascara sigue ahi.
        assert_eq!(px(&layers, 3, 3), [0, 0, 0, 0]);
        // Ya estaba limpia: no hay segundo paso.
        assert!(clear_selected(&mut layers, &mut h).is_none());
        assert!(h.undo(&mut layers).is_some());
        let m = &layers.layers[0].mask().unwrap().pixels;
        assert_eq!(&m[i..i + 4], &[0, 0, 0, 255]);
    }

    #[test]
    fn limpiar_la_mascara_de_una_capa_de_color_la_vuelve_negra() {
        let mut layers = LayerStack::new(64);
        layers.add_solid_layer([10, 20, 30]);
        let mut h = History::default();
        let i = ((3 * 64 + 3) * 4) as usize;
        layers.layers[1].mask_mut().unwrap().pixels[i..i + 4]
            .copy_from_slice(&[255, 255, 255, 255]);
        assert!(clear_selected(&mut layers, &mut h).is_some());
        let m = &layers.layers[1].mask().unwrap().pixels;
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
        layers.layers[0].mask_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        assert!(invert_selected(&mut layers, &mut h).is_some());
        let m = &layers.layers[0].mask().unwrap().pixels;
        // El negro paso a blanco y el resto (blanco) a negro; sigue siendo opaca.
        assert_eq!(&m[i..i + 4], &[255, 255, 255, 255]);
        assert_eq!(&m[0..4], &[0, 0, 0, 255]);
        assert!(h.undo(&mut layers).is_some());
        let m = &layers.layers[0].mask().unwrap().pixels;
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
    fn deshacer_en_una_de_varias_mascaras_no_toca_las_otras() {
        let mut layers = LayerStack::new(128);
        layers.add_mask();
        layers.add_mask(); // la elegida es la segunda (indice 1)
        let mut h = History::default();
        let id = layers.layers[0].id;
        let size = layers.size;
        let i = ((5 * size + 70) * 4) as usize;
        h.begin(id, PaintTarget::Mask(1));
        h.save_tiles(&layers.layers[0].masks[1].pixels, size, (70, 5, 70, 5));
        layers.layers[0].masks[1].pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        h.end();
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(&layers.layers[0].masks[1].pixels[i..i + 4], &[255, 255, 255, 255]);
        // La otra mascara y la capa no se tocaron.
        assert_eq!(&layers.layers[0].masks[0].pixels[i..i + 4], &[255, 255, 255, 255]);
        assert_eq!(px(&layers, 70, 5), [0, 0, 0, 0]);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(&layers.layers[0].masks[1].pixels[i..i + 4], &[0, 0, 0, 255]);
        // Si esa mascara ya no existe, el paso se descarta sin fallar.
        layers.remove_mask();
        layers.remove_mask();
        assert!(h.undo(&mut layers).is_none());
    }

    /// Pasa al historial los cambios de estructura que hizo la pila (lo que en la app hace `main`).
    fn registrar(h: &mut History, layers: &mut LayerStack) {
        for e in layers.take_layout_log() {
            h.push_layout(e);
        }
    }

    fn ids(layers: &LayerStack) -> Vec<u32> {
        layers.layers.iter().map(|l| l.id).collect()
    }

    #[test]
    fn crear_una_capa_se_deshace_y_se_rehace_con_su_pintura() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        assert!(layers.add_layer());
        registrar(&mut h, &mut layers);
        let nueva = layers.layers[1].id;
        layers.layers[1].pixels[0..4].copy_from_slice(&[9, 8, 7, 255]);
        assert_eq!(layers.layers.len(), 2);
        assert_eq!(h.undo(&mut layers), Some((0, 0, 63, 63)));
        assert_eq!(layers.layers.len(), 1);
        assert_eq!(layers.selected, 0);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers.len(), 2);
        assert_eq!(layers.layers[1].id, nueva);
        assert_eq!(layers.selected, 1);
        // La pintura de la capa no se perdio al deshacer y rehacer.
        assert_eq!(&layers.layers[1].pixels[0..4], &[9, 8, 7, 255]);
    }

    #[test]
    fn borrar_una_capa_se_deshace_y_trae_su_pintura() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.add_layer();
        registrar(&mut h, &mut layers);
        let id1 = layers.layers[1].id;
        // Un trazo en la capa 1 (pasa a ser un paso anterior al borrado).
        let i = ((5 * 64 + 5) * 4) as usize;
        h.begin(id1, PaintTarget::Layer);
        h.save_tiles(&layers.layers[1].pixels, 64, (5, 5, 5, 5));
        layers.layers[1].pixels[i..i + 4].copy_from_slice(&[1, 2, 3, 255]);
        h.end();
        assert!(layers.remove_layer());
        registrar(&mut h, &mut layers);
        assert_eq!(layers.layers.len(), 1);
        // Deshacer el borrado: vuelve la capa con su pintura y vuelve a estar elegida.
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(ids(&layers).len(), 2);
        assert_eq!(layers.layers[1].id, id1);
        assert_eq!(layers.selected, 1);
        assert_eq!(&layers.layers[1].pixels[i..i + 4], &[1, 2, 3, 255]);
        // Deshacer otra vez: el trazo, que ahora si encuentra su capa.
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(&layers.layers[1].pixels[i..i + 4], &[0, 0, 0, 0]);
        // Rehacer dos veces y la capa vuelve a estar borrada.
        assert!(h.redo(&mut layers).is_some());
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers.len(), 1);
    }

    #[test]
    fn borrar_una_carpeta_trae_de_vuelta_todas_sus_capas() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.add_layer();
        assert!(layers.add_folder()); // la capa 2 queda dentro de una carpeta nueva
        registrar(&mut h, &mut layers);
        let antes = ids(&layers);
        let parents: Vec<_> = layers.layers.iter().map(|l| l.parent).collect();
        assert!(layers.layers[layers.selected].is_group());
        assert!(layers.remove_layer());
        registrar(&mut h, &mut layers);
        assert_eq!(layers.layers.len(), 1);
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(ids(&layers), antes);
        assert_eq!(
            layers.layers.iter().map(|l| l.parent).collect::<Vec<_>>(),
            parents
        );
    }

    #[test]
    fn mover_una_capa_se_deshace_y_se_rehace() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.add_layer();
        layers.add_layer();
        registrar(&mut h, &mut layers);
        let [a, b, c] = [layers.layers[0].id, layers.layers[1].id, layers.layers[2].id];
        layers.selected = 0;
        assert!(layers.move_up());
        registrar(&mut h, &mut layers);
        assert_eq!(ids(&layers), vec![b, a, c]);
        assert_eq!(layers.selected, 1);
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(ids(&layers), vec![a, b, c]);
        // La capa que se movio vuelve a ser la elegida.
        assert_eq!(layers.selected, 0);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(ids(&layers), vec![b, a, c]);
        assert_eq!(layers.selected, 1);
    }

    #[test]
    fn meter_y_sacar_de_una_carpeta_se_deshacen() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.add_layer();
        assert!(layers.add_folder()); // la capa de arriba queda dentro de una carpeta
        layers.selected = 0; // la capa 0 va suelta, debajo de la carpeta
        registrar(&mut h, &mut layers);
        let dentro = layers.layers[1].id;
        assert!(layers.layers[1].parent.is_some());
        // Sacar la de dentro de su carpeta.
        layers.selected = 1;
        assert!(layers.leave_folder());
        registrar(&mut h, &mut layers);
        let id_sacada = layers.layers[layers.selected].id;
        assert_eq!(id_sacada, dentro);
        assert!(layers.layers[layers.selected].parent.is_none());
        assert!(h.undo(&mut layers).is_some());
        let i = layers.index_of(dentro).unwrap();
        assert!(layers.layers[i].parent.is_some());
        assert_eq!(layers.selected, i);
        assert!(h.redo(&mut layers).is_some());
        assert!(layers.layers[layers.index_of(dentro).unwrap()].parent.is_none());
    }

    #[test]
    fn duplicar_una_capa_se_deshace_y_se_rehace() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.layers[0].pixels[0..4].copy_from_slice(&[4, 5, 6, 255]);
        assert!(layers.duplicate_layer());
        registrar(&mut h, &mut layers);
        let copia = layers.layers[1].id;
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(layers.layers.len(), 1);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers[1].id, copia);
        assert_eq!(&layers.layers[1].pixels[0..4], &[4, 5, 6, 255]);
    }

    #[test]
    fn los_pasos_de_estructura_y_de_trazo_se_deshacen_en_su_orden() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        stroke(&mut h, &mut layers, 3, 3, [255, 0, 0, 255]);
        layers.add_layer();
        registrar(&mut h, &mut layers);
        // Dos pasos atras: sin capa nueva y sin trazo.
        assert!(h.undo(&mut layers).is_some());
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(layers.layers.len(), 1);
        assert_eq!(px(&layers, 3, 3), [0, 0, 0, 0]);
        assert!(h.undo(&mut layers).is_none());
        // Y los dos de vuelta.
        assert!(h.redo(&mut layers).is_some());
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers.len(), 2);
        assert_eq!(px(&layers, 3, 3), [255, 0, 0, 255]);
        // Un paso de estructura nuevo vacia el rehacer.
        h.undo(&mut layers);
        layers.add_layer();
        registrar(&mut h, &mut layers);
        assert!(h.redo(&mut layers).is_none());
    }

    #[test]
    fn un_paso_que_pide_una_capa_que_ya_no_existe_se_descarta_sin_fallar() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.add_layer();
        layers.add_layer();
        registrar(&mut h, &mut layers);
        // "Combinar hacia abajo" no se deshace: la de arriba desaparece y la de abajo cambia de id.
        assert!(layers.merge_down());
        assert_eq!(layers.layers.len(), 2);
        // El paso de la segunda capa pide una que ya no existe y se salta; el de la primera si se puede.
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(layers.layers.len(), 1);
        assert_eq!(layers.layers[0].id, 0);
        assert!(h.undo(&mut layers).is_none());
    }

    /// Lo que hace `main` en cada cuadro: compara los ajustes con los del cuadro anterior (`seen`), registra el cambio y
    /// deja los de ahora como los vistos.
    fn cuadro(
        h: &mut History,
        layers: &LayerStack,
        seen: &mut Vec<crate::layers::LayerSettings>,
        drag: bool,
    ) {
        let now = layers.settings_snapshot();
        h.record_settings(crate::layers::settings_diff(seen, &now), drag);
        *seen = now;
    }

    #[test]
    fn los_ajustes_de_una_capa_se_deshacen_y_se_rehacen() {
        use crate::blend::BlendMode;
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        let mut seen = layers.settings_snapshot();
        let l = &mut layers.layers[0];
        l.opacity = 40;
        l.blend = BlendMode::Multiply;
        l.name = "piel".to_string();
        l.visible = false;
        l.locked = true;
        l.alpha_lock = true;
        l.clip_to_below = true;
        cuadro(&mut h, &layers, &mut seen, false);
        assert_eq!(h.undo(&mut layers), Some((0, 0, 63, 63)));
        let l = &layers.layers[0];
        assert_eq!((l.opacity, l.blend, l.name.as_str()), (100, BlendMode::Normal, "1"));
        assert!(l.visible && !l.locked && !l.alpha_lock && !l.clip_to_below);
        assert!(h.redo(&mut layers).is_some());
        let l = &layers.layers[0];
        assert_eq!((l.opacity, l.blend, l.name.as_str()), (40, BlendMode::Multiply, "piel"));
        assert!(!l.visible && l.locked && l.alpha_lock && l.clip_to_below);
    }

    #[test]
    fn un_arrastre_de_deslizador_es_un_solo_paso() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        let mut seen = layers.settings_snapshot();
        // Primer arrastre: 100 -> 90 -> 80 -> 70 con el boton apretado, y al soltar un cuadro sin cambios.
        for v in [90, 80, 70] {
            layers.layers[0].opacity = v;
            cuadro(&mut h, &layers, &mut seen, true);
        }
        cuadro(&mut h, &layers, &mut seen, false);
        // Segundo arrastre: 70 -> 50.
        for v in [60, 50] {
            layers.layers[0].opacity = v;
            cuadro(&mut h, &layers, &mut seen, true);
        }
        cuadro(&mut h, &layers, &mut seen, false);
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(layers.layers[0].opacity, 70);
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(layers.layers[0].opacity, 100);
        assert!(h.undo(&mut layers).is_none());
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers[0].opacity, 70);
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers[0].opacity, 50);
    }

    #[test]
    fn dos_clics_seguidos_son_dos_pasos() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        let mut seen = layers.settings_snapshot();
        layers.layers[0].visible = false;
        cuadro(&mut h, &layers, &mut seen, false);
        layers.layers[0].locked = true;
        cuadro(&mut h, &layers, &mut seen, false);
        assert!(h.undo(&mut layers).is_some());
        assert!(!layers.layers[0].locked && !layers.layers[0].visible);
        assert!(h.undo(&mut layers).is_some());
        assert!(layers.layers[0].visible);
        assert!(h.undo(&mut layers).is_none());
    }

    #[test]
    fn un_ajuste_nuevo_vacia_el_rehacer() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        let mut seen = layers.settings_snapshot();
        layers.layers[0].opacity = 30;
        cuadro(&mut h, &layers, &mut seen, false);
        assert!(h.undo(&mut layers).is_some());
        seen = layers.settings_snapshot();
        layers.layers[0].locked = true;
        cuadro(&mut h, &layers, &mut seen, false);
        assert!(h.redo(&mut layers).is_none());
        assert_eq!(layers.layers[0].opacity, 100);
    }

    #[test]
    fn el_color_de_una_capa_lisa_y_el_estado_de_su_mascara_se_deshacen() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        assert!(layers.add_solid_layer([10, 20, 30]));
        let i = layers.selected;
        let mut seen = layers.settings_snapshot();
        assert!(layers.set_solid_color([200, 100, 50]));
        assert!(layers.toggle_mask_enabled());
        assert!(layers.invert_mask());
        cuadro(&mut h, &layers, &mut seen, false);
        assert!(h.undo(&mut layers).is_some());
        let l = &layers.layers[i];
        assert_eq!(l.source, crate::layers::LayerSource::Solid([10, 20, 30]));
        let m = l.mask().unwrap();
        assert!(m.enabled && !m.invert);
        assert!(h.redo(&mut layers).is_some());
        let l = &layers.layers[i];
        assert_eq!(l.source, crate::layers::LayerSource::Solid([200, 100, 50]));
        let m = l.mask().unwrap();
        assert!(!m.enabled && m.invert);
    }

    #[test]
    fn si_cambian_las_capas_o_las_mascaras_no_se_registra_un_paso_de_ajustes() {
        let mut layers = LayerStack::new(64);
        let antes = layers.settings_snapshot();
        layers.add_layer();
        assert!(crate::layers::settings_diff(&antes, &layers.settings_snapshot()).is_none());
        let antes = layers.settings_snapshot();
        layers.add_mask();
        assert!(crate::layers::settings_diff(&antes, &layers.settings_snapshot()).is_none());
    }

    #[test]
    fn un_paso_de_ajustes_de_una_capa_borrada_se_descarta_sin_fallar() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        layers.add_layer();
        registrar(&mut h, &mut layers);
        let mut seen = layers.settings_snapshot();
        layers.layers[1].opacity = 10;
        cuadro(&mut h, &layers, &mut seen, false);
        // "Combinar hacia abajo" quita la capa 1 y no se deshace: el paso de ajustes ya no tiene capa.
        assert!(layers.merge_down());
        // El paso de ajustes pide una capa que ya no existe: se salta. Lo que importa es que no falle ni toque la capa 0.
        let antes = layers.layers[0].opacity;
        let _ = h.undo(&mut layers);
        let _ = h.undo(&mut layers);
        assert_eq!(layers.layers[0].opacity, antes);
        assert!(h.undo(&mut layers).is_none());
    }

    #[test]
    fn anadir_y_quitar_una_mascara_se_deshace_con_su_pintura() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        assert!(layers.add_mask());
        registrar(&mut h, &mut layers);
        let i = ((3 * 64 + 3) * 4) as usize;
        layers.layers[0].mask_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        // Deshacer: la mascara se va y se vuelve a pintar la capa.
        assert_eq!(h.undo(&mut layers), Some((0, 0, 63, 63)));
        assert!(layers.layers[0].masks.is_empty());
        assert!(!layers.layers[0].editing_mask);
        // Rehacer: vuelve la misma mascara, con lo que se le pinto, y se pinta en ella.
        assert!(h.redo(&mut layers).is_some());
        assert_eq!(layers.layers[0].masks.len(), 1);
        assert_eq!(&layers.layers[0].mask().unwrap().pixels[i..i + 4], &[0, 0, 0, 255]);
        assert!(layers.layers[0].editing_mask);
        // Quitarla tambien se deshace y trae su pintura.
        assert!(layers.remove_mask());
        registrar(&mut h, &mut layers);
        assert!(layers.layers[0].masks.is_empty());
        assert!(h.undo(&mut layers).is_some());
        assert_eq!(layers.layers[0].masks.len(), 1);
        assert_eq!(&layers.layers[0].mask().unwrap().pixels[i..i + 4], &[0, 0, 0, 255]);
        assert!(layers.layers[0].editing_mask);
        assert!(h.redo(&mut layers).is_some());
        assert!(layers.layers[0].masks.is_empty());
    }

    #[test]
    fn quitar_una_de_varias_mascaras_la_devuelve_a_su_lugar() {
        let mut layers = LayerStack::new(64);
        let mut h = History::default();
        for _ in 0..3 {
            layers.add_mask();
        }
        registrar(&mut h, &mut layers);
        // Marca cada mascara con un gris distinto en el texel 0 para reconocerlas.
        for (k, g) in [10u8, 20, 30].into_iter().enumerate() {
            layers.layers[0].masks[k].pixels[0..4].copy_from_slice(&[g, g, g, 255]);
        }
        layers.layers[0].mask_sel = 1;
        assert!(layers.remove_mask()); // quita la del medio (gris 20)
        registrar(&mut h, &mut layers);
        let grises: Vec<u8> = layers.layers[0].masks.iter().map(|m| m.pixels[0]).collect();
        assert_eq!(grises, vec![10, 30]);
        assert!(h.undo(&mut layers).is_some());
        let grises: Vec<u8> = layers.layers[0].masks.iter().map(|m| m.pixels[0]).collect();
        assert_eq!(grises, vec![10, 20, 30]);
        assert_eq!(layers.layers[0].mask_sel, 1);
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
