//! Capas y composicion (ESPECIFICACION.md, secciones 4 y 5). Primera version SIN carpetas: lista plana de capas
//! de abajo hacia arriba, con visibilidad, opacidad, bloqueos, modo de mezcla y recorte. Las carpetas, mover,
//! clonar, combinar y el modo Solo vienen despues.

use crate::blend::{BlendMode, over};
use crate::paint::Rect;

/// Capas con pixeles por pieza (4 MB cada una).
pub const MAX_LAYERS: usize = 10;

pub struct PaintLayer {
    /// Identificador estable (la posicion en la lista cambia al insertar capas); lo usa el historial.
    pub id: u32,
    pub name: String,
    pub visible: bool,
    /// 0..=100.
    pub opacity: u8,
    pub locked: bool,
    pub alpha_lock: bool,
    pub blend: BlendMode,
    /// Recortar a la capa de abajo (la de mas abajo del grupo ignora este ajuste).
    pub clip_to_below: bool,
    /// RGBA de 8 bits con alfa recto; la fila 0 es v = 0.
    pub pixels: Vec<u8>,
}

impl PaintLayer {
    fn new(id: u32, name: String, size: u32) -> PaintLayer {
        PaintLayer {
            id,
            name,
            visible: true,
            opacity: 100,
            locked: false,
            alpha_lock: false,
            blend: BlendMode::Normal,
            clip_to_below: false,
            pixels: vec![0; (size * size * 4) as usize],
        }
    }

    /// Se puede pintar en ella: ni oculta ni bloqueada.
    pub fn paintable(&self) -> bool {
        self.visible && !self.locked
    }
}

/// Color opaco bajo todas las capas (seccion 4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    White,
    Checker,
    CheckerDark,
    /// En pantalla se ve como `Checker`; solo al exportar sale transparente.
    None,
}

impl Background {
    /// Lo contrario de `{:?}` (asi se guarda en el proyecto). Un nombre desconocido cae en `Checker`.
    pub fn from_name(name: &str) -> Background {
        match name {
            "White" => Background::White,
            "CheckerDark" => Background::CheckerDark,
            "None" => Background::None,
            _ => Background::Checker,
        }
    }

    fn pixel(self, x: u32, y: u32) -> [u8; 4] {
        let even = (x / 32 + y / 32) % 2 == 0;
        let c = match self {
            Background::White => 0xFF,
            Background::Checker | Background::None => {
                if even {
                    0xFF
                } else {
                    0xDE
                }
            }
            Background::CheckerDark => {
                if even {
                    0x5A
                } else {
                    0x37
                }
            }
        };
        [c, c, c, 255]
    }
}

pub struct LayerStack {
    pub size: u32,
    /// De abajo hacia arriba.
    pub layers: Vec<PaintLayer>,
    pub selected: usize,
    pub background: Background,
    /// Textura compuesta (lo que se ve), RGBA recto.
    pub composite: Vec<u8>,
    next_name: u32,
    next_id: u32,
}

impl LayerStack {
    pub fn new(size: u32) -> LayerStack {
        let mut s = LayerStack {
            size,
            layers: vec![PaintLayer::new(0, "1".to_string(), size)],
            selected: 0,
            background: Background::Checker,
            composite: vec![0; (size * size * 4) as usize],
            next_name: 2,
            next_id: 1,
        };
        s.composite_all();
        s
    }

    /// Pila armada con capas ya cargadas de un proyecto guardado. Los contadores de id y de nombre siguen despues de
    /// los mayores que haya, y la capa elegida se ajusta al rango. Si no hay ninguna capa, se pone una vacia.
    pub fn from_saved(
        size: u32,
        mut layers: Vec<PaintLayer>,
        selected: usize,
        background: Background,
    ) -> LayerStack {
        if layers.is_empty() {
            layers.push(PaintLayer::new(0, "1".to_string(), size));
        }
        let next_id = layers.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        let next_name = layers
            .iter()
            .filter_map(|l| l.name.parse::<u32>().ok())
            .max()
            .map_or(layers.len() as u32, |m| m.max(layers.len() as u32))
            + 1;
        let selected = selected.min(layers.len() - 1);
        let mut s = LayerStack {
            size,
            layers,
            selected,
            background,
            composite: vec![0; (size * size * 4) as usize],
            next_name,
            next_id,
        };
        s.composite_all();
        s
    }

    pub fn selected_layer(&self) -> &PaintLayer {
        &self.layers[self.selected]
    }

    /// Posicion actual de la capa con ese identificador (`None` si ya no existe).
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.layers.iter().position(|l| l.id == id)
    }

    /// Anade una capa transparente justo encima de la elegida y la elige. Falla en `MAX_LAYERS`.
    pub fn add_layer(&mut self) -> bool {
        if self.layers.len() >= MAX_LAYERS {
            return false;
        }
        let layer = PaintLayer::new(self.next_id, self.next_name.to_string(), self.size);
        self.next_id += 1;
        self.next_name += 1;
        self.layers.insert(self.selected + 1, layer);
        self.selected += 1;
        true
    }

    /// Cambia la capa elegida (`delta` > 0 sube). Devuelve false si ya estaba en el extremo.
    pub fn select_step(&mut self, delta: i32) -> bool {
        let n = self.selected as i32 + delta;
        if n < 0 || n >= self.layers.len() as i32 {
            return false;
        }
        self.selected = n as usize;
        true
    }

    /// Recompone todo el mapa.
    pub fn composite_all(&mut self) {
        let m = self.size - 1;
        self.composite_rect((0, 0, m, m));
    }

    /// Color final de un texel (indice `i` en bytes) sobre el fondo `bg`, con las reglas de la seccion 5.
    fn blend_texel(layers: &[PaintLayer], i: usize, bg: [u8; 4]) -> [u8; 4] {
        let mut acc = bg;
        // Alfa del pixel de la ultima capa no recortada del grupo (0 si esa capa esta oculta).
        let mut base_a = 0u32;
        for (n, layer) in layers.iter().enumerate() {
            let p = &layer.pixels[i..i + 4];
            let clipped = layer.clip_to_below && n > 0;
            if !clipped {
                base_a = if layer.visible { p[3] as u32 } else { 0 };
            }
            if !layer.visible || layer.opacity == 0 {
                continue;
            }
            let mut sa = p[3] as u32 * layer.opacity as u32 / 100;
            if clipped {
                sa = sa * base_a / 255;
            }
            if sa == 0 {
                continue;
            }
            acc = over(acc, [p[0], p[1], p[2]], sa, layer.blend);
        }
        acc
    }

    /// Recompone solo el rectangulo (extremos incluidos). Mismas reglas de la seccion 5 para pantalla y exportacion.
    /// Si la zona es grande (cambiar la opacidad, el modo de mezcla, el fondo...) se reparte por franjas de filas entre
    /// los hilos del procesador; los toques pequenos del pincel se hacen en un solo hilo, donde crear hilos no compensa.
    pub fn composite_rect(&mut self, (x0, y0, x1, y1): Rect) {
        let size = self.size;
        let stride = (size * 4) as usize;
        let rows = (y1 - y0 + 1) as usize;
        let area = (x1 - x0 + 1) as usize * rows;
        let layers: &[PaintLayer] = &self.layers;
        let bg = self.background;
        let out = &mut self.composite[y0 as usize * stride..(y1 as usize + 1) * stride];

        let threads = if area < 65_536 {
            1
        } else {
            std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .min(rows)
        };
        if threads <= 1 {
            composite_rows(layers, bg, size, y0, out, x0, x1);
            return;
        }
        let band = rows.div_ceil(threads);
        std::thread::scope(|s| {
            for (n, chunk) in out.chunks_mut(band * stride).enumerate() {
                let y_start = y0 + (n * band) as u32;
                s.spawn(move || composite_rows(layers, bg, size, y_start, chunk, x0, x1));
            }
        });
    }

    /// Textura completa para exportar (seccion 13): con el fondo elegido, o transparente. Con el fondo `None`
    /// sale transparente aunque se pida con fondo.
    pub fn export_composite(&self, with_background: bool) -> Vec<u8> {
        let size = self.size;
        let mut out = vec![0u8; (size * size * 4) as usize];
        let opaque_bg = with_background && self.background != Background::None;
        for y in 0..size {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                let bg = if opaque_bg {
                    self.background.pixel(x, y)
                } else {
                    [0, 0, 0, 0]
                };
                out[i..i + 4].copy_from_slice(&Self::blend_texel(&self.layers, i, bg));
            }
        }
        out
    }

    /// Copia contigua del rectangulo de la textura compuesta (para subirlo a la GPU).
    pub fn rect_bytes(&self, (x0, y0, x1, y1): Rect) -> Vec<u8> {
        let mut out = Vec::with_capacity(((x1 - x0 + 1) * (y1 - y0 + 1) * 4) as usize);
        for y in y0..=y1 {
            let a = ((y * self.size + x0) * 4) as usize;
            let b = ((y * self.size + x1 + 1) * 4) as usize;
            out.extend_from_slice(&self.composite[a..b]);
        }
        out
    }
}

/// Compone las columnas `x0..=x1` de un grupo de filas de la textura. `rows` son las filas completas (RGBA) de la
/// textura compuesta a partir de la fila `y_start`; cada hilo recibe su propia franja, asi que no se pisan.
fn composite_rows(
    layers: &[PaintLayer],
    bg: Background,
    size: u32,
    y_start: u32,
    rows: &mut [u8],
    x0: u32,
    x1: u32,
) {
    let stride = (size * 4) as usize;
    for (k, row) in rows.chunks_mut(stride).enumerate() {
        let y = y_start + k as u32;
        for x in x0..=x1 {
            let i = ((y * size + x) * 4) as usize;
            let acc = LayerStack::blend_texel(layers, i, bg.pixel(x, y));
            let o = (x * 4) as usize;
            row[o..o + 4].copy_from_slice(&acc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_px(layer: &mut PaintLayer, size: u32, x: u32, y: u32, rgba: [u8; 4]) {
        let i = ((y * size + x) * 4) as usize;
        layer.pixels[i..i + 4].copy_from_slice(&rgba);
    }

    fn px(stack: &LayerStack, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * stack.size + x) * 4) as usize;
        [
            stack.composite[i],
            stack.composite[i + 1],
            stack.composite[i + 2],
            stack.composite[i + 3],
        ]
    }

    #[test]
    fn sin_pintura_se_ve_el_fondo() {
        let s = LayerStack::new(64);
        assert_eq!(px(&s, 0, 0), [255, 255, 255, 255]);
        assert_eq!(px(&s, 32, 0), [0xDE, 0xDE, 0xDE, 255]);
    }

    #[test]
    fn capa_normal_tapa_el_fondo_y_opacidad_la_atenua() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
        s.layers[0].opacity = 50;
        s.composite_all();
        // 50 % de rojo sobre blanco: G y B suben a ~127.
        let p = px(&s, 5, 5);
        assert_eq!(p[0], 255);
        assert!((p[1] as i32 - 127).abs() <= 1 && (p[2] as i32 - 127).abs() <= 1);
        assert_eq!(p[3], 255);
    }

    #[test]
    fn capa_oculta_no_se_ve() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 1, 1, [255, 0, 0, 255]);
        s.layers[0].visible = false;
        s.composite_all();
        assert_eq!(px(&s, 1, 1), [255, 255, 255, 255]);
    }

    #[test]
    fn multiplicar_oscurece_el_fondo() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        set_px(&mut s.layers[1], 64, 2, 2, [128, 128, 128, 255]);
        s.layers[1].blend = BlendMode::Multiply;
        s.composite_all();
        // Blanco x 128 = 128.
        assert_eq!(px(&s, 2, 2), [128, 128, 128, 255]);
    }

    #[test]
    fn recorte_solo_se_ve_donde_la_base_tiene_pintura() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        // Base: un pixel rojo en (3,3). Capa recortada: azul en (3,3) y en (4,4).
        set_px(&mut s.layers[0], 64, 3, 3, [255, 0, 0, 255]);
        set_px(&mut s.layers[1], 64, 3, 3, [0, 0, 255, 255]);
        set_px(&mut s.layers[1], 64, 4, 4, [0, 0, 255, 255]);
        s.layers[1].clip_to_below = true;
        s.composite_all();
        assert_eq!(px(&s, 3, 3), [0, 0, 255, 255]);
        // Donde la base esta vacia, la recortada no se ve: queda el fondo.
        assert_eq!(px(&s, 4, 4), s.background.pixel(4, 4));
    }

    #[test]
    fn exportar_con_y_sin_fondo() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 1, 1, [255, 0, 0, 255]);
        let sin = s.export_composite(false);
        assert_eq!(&sin[0..4], &[0, 0, 0, 0]);
        let i = ((64 + 1) * 4) as usize;
        assert_eq!(&sin[i..i + 4], &[255, 0, 0, 255]);
        let con = s.export_composite(true);
        assert_eq!(&con[0..4], &[255, 255, 255, 255]);
        // Con el fondo `None` sale transparente aunque se pida con fondo.
        s.background = Background::None;
        assert_eq!(&s.export_composite(true)[0..4], &[0, 0, 0, 0]);
    }

    #[test]
    fn maximo_de_capas() {
        let mut s = LayerStack::new(16);
        for _ in 0..MAX_LAYERS - 1 {
            assert!(s.add_layer());
        }
        assert!(!s.add_layer());
        assert_eq!(s.layers.len(), MAX_LAYERS);
    }
}
