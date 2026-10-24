//! Capas y composicion (ESPECIFICACION.md, secciones 4 y 5). Lista plana de capas de abajo hacia arriba, con
//! visibilidad, opacidad, bloqueos, modo de mezcla y recorte, mascara, canales, capas de color liso y carpetas de un
//! solo nivel (las capas de una carpeta van seguidas justo debajo de ella), subir y bajar capas y un Modo Solo que solo
//! cambia lo que se ve. Arrastrar capas viene despues.

use crate::blend::{BlendMode, over};
use crate::paint::Rect;

/// Capas con pixeles por pieza (4 MB cada una).
pub const MAX_LAYERS: usize = 10;

/// Intensidad del relieve del mapa de normales que se exporta a partir de la altura (como el "Strength" del Bump de
/// Blender). Mas alto = relieve mas marcado.
pub const NORMAL_STRENGTH: f32 = 10.0;

/// Que se pinta: el color o uno de los canales del material. Cada capa guarda una hoja por canal (como en Ucupaint) y
/// la mascara, la opacidad y la visibilidad de la capa valen para todas. Los canales que no son color se ven y se
/// exportan como imagenes en grises.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChannelKind {
    Color,
    Roughness,
    Metallic,
    Height,
}

impl ChannelKind {
    pub const ALL: [ChannelKind; 4] = [
        ChannelKind::Color,
        ChannelKind::Roughness,
        ChannelKind::Metallic,
        ChannelKind::Height,
    ];
    /// Los canales que no son color.
    pub const EXTRA: [ChannelKind; 3] = [
        ChannelKind::Roughness,
        ChannelKind::Metallic,
        ChannelKind::Height,
    ];

    /// Nombre para mostrar.
    pub fn label(self) -> &'static str {
        match self {
            ChannelKind::Color => "Color",
            ChannelKind::Roughness => "Rugosidad",
            ChannelKind::Metallic => "Metálico",
            ChannelKind::Height => "Altura",
        }
    }

    /// Nombre para archivos (sin acentos ni espacios).
    pub fn file_tag(self) -> &'static str {
        match self {
            ChannelKind::Color => "color",
            ChannelKind::Roughness => "rugosidad",
            ChannelKind::Metallic => "metalico",
            ChannelKind::Height => "altura",
        }
    }

    /// Valor (0..=255) de un texel donde ninguna capa pinta este canal: rugosidad y altura a la mitad, nada de metal.
    pub fn default_value(self) -> u8 {
        match self {
            ChannelKind::Color => 255,
            ChannelKind::Roughness | ChannelKind::Height => 128,
            ChannelKind::Metallic => 0,
        }
    }

    /// Posicion en las listas de canales extra (`None` = el color, que tiene su propio sitio).
    pub fn slot(self) -> Option<usize> {
        match self {
            ChannelKind::Color => None,
            ChannelKind::Roughness => Some(0),
            ChannelKind::Metallic => Some(1),
            ChannelKind::Height => Some(2),
        }
    }
}

/// En que hoja de una capa escribe el pincel (y a cual se refiere un paso del historial).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaintTarget {
    /// Los pixeles de color de la capa.
    Layer,
    /// La mascara de la capa.
    Mask,
    /// Una hoja de canal extra (posicion en `PaintLayer::chans`).
    Channel(usize),
}

/// De donde sale el color de una capa (como el "tipo de capa" de Ucupaint).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerSource {
    /// Pintura normal: el color son los pixeles que se pintan en ella.
    Paint,
    /// Color liso: toda la capa es de ese color y no se pinta; se ve donde lo deja pasar su mascara y se puede cambiar
    /// de color despues. Solo vale para el color: los canales (rugosidad...) de la capa se pintan como siempre.
    Solid([u8; 3]),
    /// Carpeta: junta las capas que tiene dentro (las que apuntan a ella con `PaintLayer::parent`, que van justo
    /// debajo de ella en la lista). Sus `pixels` y `chans` no se pintan: son el resultado ya compuesto de esas capas,
    /// que `LayerStack::composite_rect` mantiene al dia. La carpeta se mezcla con lo de abajo como una capa mas, con su
    /// propia opacidad, modo de mezcla, mascara y visibilidad.
    Group,
}

/// Operaciones sobre toda la hoja que se pinta ahora (la capa, un canal o la mascara) que pide el panel de capas.
/// Las hace `undo::clear_selected` y `undo::invert_selected`, y entran al historial (Ctrl+Z las deshace).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LayerOp {
    /// Dejarla vacia ("Limpiar capa").
    Clear,
    /// Cambiar sus colores por los opuestos ("Invertir colores").
    Invert,
}

#[derive(Clone)]
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
    /// Mascara de la capa (una sola por ahora): decide en que parte se ve la pintura.
    pub mask: Option<LayerMask>,
    /// Se pinta en la mascara y no en los pixeles de la capa. Solo cuenta si la capa tiene mascara.
    pub editing_mask: bool,
    /// Hojas de los canales extra (rugosidad, metalico, altura; ver `ChannelKind::slot`). RGBA como los pixeles de la
    /// capa: el alfa dice donde la capa pinta ese canal y el valor es la luminosidad del color. Se crean al pintar.
    pub chans: [Option<Vec<u8>>; 3],
    /// De donde sale el color de la capa: pintura normal, un color liso o una carpeta.
    pub source: LayerSource,
    /// Carpeta a la que pertenece (su `id`), o `None` si va suelta. Las capas de una carpeta van seguidas, justo debajo
    /// de ella en la lista, y no se dibujan por su cuenta: la carpeta las junta. Una carpeta no va dentro de otra.
    pub parent: Option<u32>,
}

/// Mascara de una capa (como las de Ucupaint): una imagen en grises del mismo tamano que la textura. Donde es blanca la
/// pintura de la capa se ve entera, donde es negra se oculta y los grises la dejan a medias. Se pinta con los mismos
/// pinceles que la capa, asi que se guarda como RGBA (siempre opaca) y su valor es la luminosidad del texel.
#[derive(Clone)]
pub struct LayerMask {
    /// Apagada, la capa se ve como si no tuviera mascara (lo pintado en ella se conserva).
    pub enabled: bool,
    /// Invertida, el blanco oculta y el negro muestra.
    pub invert: bool,
    /// RGBA opaco en grises, la fila 0 es v = 0.
    pub pixels: Vec<u8>,
}

impl LayerMask {
    /// Mascara nueva: toda blanca (no oculta nada).
    pub fn new(size: u32) -> LayerMask {
        LayerMask::filled(size, 255)
    }

    /// Mascara toda de un gris `v` (0 = oculta todo, 255 = no oculta nada), siempre opaca.
    pub fn filled(size: u32, v: u8) -> LayerMask {
        let mut pixels = vec![v; (size * size * 4) as usize];
        for p in pixels.chunks_exact_mut(4) {
            p[3] = 255;
        }
        LayerMask {
            enabled: true,
            invert: false,
            pixels,
        }
    }

    /// Cuanto se ve la capa en el texel con indice `i` (en bytes, el mismo que usan los pixeles de la capa): 0..=255.
    #[inline]
    fn value(&self, i: usize) -> u32 {
        let p = &self.pixels[i..i + 3];
        // Luminosidad entera (77 + 150 + 29 = 256): blanco da 255 y negro 0 sin redondeos.
        let v = (77 * p[0] as u32 + 150 * p[1] as u32 + 29 * p[2] as u32) >> 8;
        if self.invert { 255 - v } else { v }
    }
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
            mask: None,
            editing_mask: false,
            chans: [None, None, None],
            source: LayerSource::Paint,
            parent: None,
        }
    }

    /// Se puede pintar en ella: ni oculta ni bloqueada.
    pub fn paintable(&self) -> bool {
        self.visible && !self.locked
    }

    /// Es una carpeta.
    pub fn is_group(&self) -> bool {
        self.source == LayerSource::Group
    }

    /// El color liso de la capa, si es de color liso.
    pub fn is_solid(&self) -> bool {
        matches!(self.source, LayerSource::Solid(_))
    }

    /// El pincel puede escribir en `channel` de esta capa: se puede pintar en ella y, si es de color liso, solo vale
    /// pintar en su mascara o en un canal (los pixeles de color de una capa lisa no se usan). En una carpeta solo se
    /// pinta su mascara: lo demas es el resultado de las capas de dentro.
    pub fn accepts(&self, channel: ChannelKind) -> bool {
        self.paintable()
            && match self.source {
                LayerSource::Paint => true,
                LayerSource::Solid(_) => self.painting_mask() || channel != ChannelKind::Color,
                LayerSource::Group => self.painting_mask(),
            }
    }

    /// La mascara que se aplica al componer (la que existe y esta encendida).
    pub fn active_mask(&self) -> Option<&LayerMask> {
        self.mask.as_ref().filter(|m| m.enabled)
    }

    /// El pincel pinta en la mascara de esta capa (y no en sus pixeles).
    pub fn painting_mask(&self) -> bool {
        self.editing_mask && self.mask.is_some()
    }

    /// A donde va el pincel: la mascara si se esta pintando en ella; si no, el canal elegido (color = la capa).
    pub fn target(&self, channel: ChannelKind) -> PaintTarget {
        if self.painting_mask() {
            PaintTarget::Mask
        } else {
            match channel.slot() {
                Some(s) => PaintTarget::Channel(s),
                None => PaintTarget::Layer,
            }
        }
    }

    /// Pixeles del destino. La hoja de un canal se crea (vacia, sin pintura) la primera vez que se necesita.
    pub fn target_pixels_mut(&mut self, target: PaintTarget) -> &mut Vec<u8> {
        let len = self.pixels.len();
        match target {
            PaintTarget::Mask => match self.mask.as_mut() {
                Some(m) => &mut m.pixels,
                None => &mut self.pixels,
            },
            PaintTarget::Channel(s) => self.chans[s].get_or_insert_with(|| vec![0; len]),
            PaintTarget::Layer => &mut self.pixels,
        }
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
    /// Canal que se pinta y se ve: Color, o uno de los canales en grises (rugosidad, metalico, altura).
    pub channel: ChannelKind,
    /// Vista de material: se ve el color de la capa (no el canal) y el visor usa rugosidad, metalico y altura.
    pub material_view: bool,
    /// Textura compuesta (lo que se ve), RGBA recto.
    pub composite: Vec<u8>,
    next_name: u32,
    next_id: u32,
    /// Operacion sobre toda la hoja que pidio el panel de capas. La atiende `main` (hace falta el historial de la pieza)
    /// y la apaga.
    pub op_requested: Option<LayerOp>,
    /// Modo Solo: en pantalla solo se ve la capa elegida (ver `solo_top`). No se guarda con el proyecto.
    pub solo: bool,
    /// Lo que se mostraba (`solo_top`) la ultima vez que se compuso todo: sirve para saber si hay que recomponer.
    solo_seen: Option<u32>,
}

impl LayerStack {
    pub fn new(size: u32) -> LayerStack {
        let mut s = LayerStack {
            size,
            layers: vec![PaintLayer::new(0, "1".to_string(), size)],
            selected: 0,
            background: Background::Checker,
            channel: ChannelKind::Color,
            material_view: false,
            composite: vec![0; (size * size * 4) as usize],
            next_name: 2,
            next_id: 1,
            op_requested: None,
            solo: false,
            solo_seen: None,
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
        fix_folders(&mut layers);
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
            channel: ChannelKind::Color,
            material_view: false,
            composite: vec![0; (size * size * 4) as usize],
            next_name,
            next_id,
            op_requested: None,
            solo: false,
            solo_seen: None,
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

    /// Anade una capa transparente justo encima de la elegida y la elige. Falla en `MAX_LAYERS`. Si la elegida esta dentro
    /// de una carpeta, la nueva tambien (queda dentro, encima de ella); si es la carpeta misma, queda suelta encima.
    pub fn add_layer(&mut self) -> bool {
        if self.layers.len() >= MAX_LAYERS {
            return false;
        }
        let mut layer = PaintLayer::new(self.next_id, self.next_name.to_string(), self.size);
        layer.parent = self.layers[self.selected].parent;
        self.next_id += 1;
        self.next_name += 1;
        self.layers.insert(self.selected + 1, layer);
        self.selected += 1;
        true
    }

    /// Posicion de la primera capa de la carpeta que esta en `folder` (la propia carpeta si no tiene nada dentro): las
    /// capas de una carpeta van seguidas justo debajo de ella.
    fn group_start(&self, folder: usize) -> usize {
        let id = self.layers[folder].id;
        let mut first = folder;
        while first > 0 && self.layers[first - 1].parent == Some(id) {
            first -= 1;
        }
        first
    }

    /// Por que no se puede meter la capa elegida en una carpeta nueva (`None` si se puede).
    pub fn add_folder_blocker(&self) -> Option<&'static str> {
        let l = &self.layers[self.selected];
        if l.is_group() || l.parent.is_some() {
            return Some("Elige una capa suelta: una carpeta no puede ir dentro de otra");
        }
        if self.layers.len() >= MAX_LAYERS {
            return Some("No caben mas capas");
        }
        None
    }

    /// Mete la capa elegida en una carpeta nueva, que queda justo encima y elegida. Para sumarle mas capas se elige una de
    /// las de dentro y se anade otra (queda dentro tambien). false si `add_folder_blocker` dice que no se puede.
    pub fn add_folder(&mut self) -> bool {
        if self.add_folder_blocker().is_some() {
            return false;
        }
        let mut folder = PaintLayer::new(
            self.next_id,
            format!("Carpeta {}", self.next_name),
            self.size,
        );
        folder.source = LayerSource::Group;
        let folder_id = folder.id;
        self.next_id += 1;
        self.next_name += 1;
        let sel = self.selected;
        self.layers[sel].parent = Some(folder_id);
        self.layers.insert(sel + 1, folder);
        self.selected = sel + 1;
        true
    }

    /// Posicion de la carpeta a la que la capa elegida puede entrar: la que tiene justo encima (si encima hay una capa
    /// de una carpeta, esa carpeta; o una carpeta vacia). `None` si la elegida ya esta en una carpeta, es una carpeta o
    /// encima no hay ninguna.
    fn folder_above(&self) -> Option<usize> {
        let l = &self.layers[self.selected];
        if l.is_group() || l.parent.is_some() {
            return None;
        }
        let above = self.layers.get(self.selected + 1)?;
        let folder_id = match above.parent {
            Some(p) => p,
            None if above.is_group() => above.id,
            None => return None,
        };
        self.index_of(folder_id)
    }

    /// La capa elegida puede entrar en la carpeta que tiene justo encima (`enter_folder_above`).
    pub fn can_enter_folder_above(&self) -> bool {
        self.folder_above().is_some()
    }

    /// Mete la capa elegida en la carpeta que tiene justo encima (queda la de mas abajo de la carpeta). La capa no se mueve
    /// de sitio. false si no se puede.
    pub fn enter_folder_above(&mut self) -> bool {
        let Some(f) = self.folder_above() else {
            return false;
        };
        self.layers[self.selected].parent = Some(self.layers[f].id);
        true
    }

    /// La capa elegida esta dentro de una carpeta (`leave_folder`).
    pub fn can_leave_folder(&self) -> bool {
        self.layers[self.selected].parent.is_some()
    }

    /// Saca la capa elegida de su carpeta: queda suelta, justo encima de ella, y sigue elegida. false si no estaba en una.
    pub fn leave_folder(&mut self) -> bool {
        let sel = self.selected;
        let Some(folder_id) = self.layers[sel].parent else {
            return false;
        };
        let Some(f) = self.index_of(folder_id) else {
            // No deberia pasar (la carpeta ya no existe): la capa queda suelta donde esta.
            self.layers[sel].parent = None;
            return true;
        };
        let mut layer = self.layers.remove(sel);
        layer.parent = None;
        // Al quitarla, la carpeta paso de `f` a `f - 1`: la capa se pone justo encima.
        self.layers.insert(f, layer);
        self.selected = f;
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

    /// Pone una mascara blanca (que no oculta nada) a la capa elegida y pasa a pintar en ella. false si ya tenia una.
    /// Quien llama debe recomponer la textura.
    pub fn add_mask(&mut self) -> bool {
        let size = self.size;
        let layer = &mut self.layers[self.selected];
        if layer.mask.is_some() {
            return false;
        }
        layer.mask = Some(LayerMask::new(size));
        layer.editing_mask = true;
        true
    }

    /// Quita la mascara de la capa elegida (la pintura de la capa no cambia) y vuelve a pintar en la capa.
    pub fn remove_mask(&mut self) -> bool {
        let layer = &mut self.layers[self.selected];
        layer.editing_mask = false;
        layer.mask.take().is_some()
    }

    /// Enciende o apaga la mascara de la capa elegida (apagada se conserva pero no oculta nada).
    pub fn toggle_mask_enabled(&mut self) -> bool {
        match self.layers[self.selected].mask.as_mut() {
            Some(m) => {
                m.enabled = !m.enabled;
                true
            }
            None => false,
        }
    }

    /// Invierte la mascara de la capa elegida (lo blanco pasa a ocultar y lo negro a mostrar).
    pub fn invert_mask(&mut self) -> bool {
        match self.layers[self.selected].mask.as_mut() {
            Some(m) => {
                m.invert = !m.invert;
                true
            }
            None => false,
        }
    }

    /// Anade una capa de color liso justo encima de la elegida y la elige. Empieza con una mascara NEGRA (todavia no se ve
    /// nada) y pasa a pintar en ella: con el pincel se "pinta" el color donde se quiere. false en `MAX_LAYERS`.
    pub fn add_solid_layer(&mut self, color: [u8; 3]) -> bool {
        if !self.add_layer() {
            return false;
        }
        let size = self.size;
        let layer = &mut self.layers[self.selected];
        layer.source = LayerSource::Solid(color);
        layer.mask = Some(LayerMask::filled(size, 0));
        layer.editing_mask = true;
        true
    }

    /// Cambia el color de la capa elegida si es de color liso. false si no lo es o ya tenia ese color.
    pub fn set_solid_color(&mut self, color: [u8; 3]) -> bool {
        match &mut self.layers[self.selected].source {
            LayerSource::Solid(c) if *c != color => {
                *c = color;
                true
            }
            _ => false,
        }
    }

    /// Posiciones (primera y ultima, incluidas) de lo que se mueve junto con la capa `i`: una capa suelta va sola; una
    /// carpeta y sus capas de dentro van juntas (da lo mismo cual de ellas se pase).
    fn unit_range(&self, i: usize) -> (usize, usize) {
        let l = &self.layers[i];
        let folder = if l.is_group() {
            Some(i)
        } else {
            l.parent.and_then(|p| self.index_of(p))
        };
        match folder {
            Some(f) => (self.group_start(f), f),
            None => (i, i),
        }
    }

    /// Por que la capa elegida no se puede subir (`None` si se puede).
    pub fn move_up_blocker(&self) -> Option<&'static str> {
        let sel = self.selected;
        let parent = self.layers[sel].parent;
        if parent.is_some() {
            // Dentro de una carpeta solo se mueve entre las de la carpeta; encima de la ultima va la carpeta misma.
            let above_is_sibling = self.layers.get(sel + 1).is_some_and(|a| a.parent == parent);
            return if above_is_sibling {
                None
            } else {
                Some("Ya es la de mas arriba de su carpeta (para subirla mas, sacala de la carpeta)")
            };
        }
        let (_, end) = self.unit_range(sel);
        if end + 1 >= self.layers.len() {
            Some("Ya es la capa de mas arriba")
        } else {
            None
        }
    }

    /// Sube la capa elegida un puesto (una carpeta sube con todas sus capas de dentro; una capa suelta salta por encima
    /// de una carpeta entera). La capa sigue elegida y no cambia de id, asi que el historial sigue valiendo. false si
    /// `move_up_blocker` dice que no se puede. Quien llama debe recomponer la textura.
    pub fn move_up(&mut self) -> bool {
        if self.move_up_blocker().is_some() {
            return false;
        }
        let sel = self.selected;
        if self.layers[sel].parent.is_some() {
            self.layers.swap(sel, sel + 1);
            self.selected = sel + 1;
            return true;
        }
        let (start, end) = self.unit_range(sel);
        let (_, next_end) = self.unit_range(end + 1);
        // La de arriba (end + 1 ..= next_end) pasa a ir debajo de la nuestra.
        self.layers[start..=next_end].rotate_left(end - start + 1);
        self.selected = sel + (next_end - end);
        true
    }

    /// Por que la capa elegida no se puede bajar (`None` si se puede).
    pub fn move_down_blocker(&self) -> Option<&'static str> {
        let sel = self.selected;
        let parent = self.layers[sel].parent;
        if parent.is_some() {
            let below_is_sibling = sel > 0 && self.layers[sel - 1].parent == parent;
            return if below_is_sibling {
                None
            } else {
                Some("Ya es la de mas abajo de su carpeta (para bajarla mas, sacala de la carpeta)")
            };
        }
        let (start, _) = self.unit_range(sel);
        if start == 0 {
            Some("Ya es la capa de mas abajo")
        } else {
            None
        }
    }

    /// Baja la capa elegida un puesto (como `move_up`, al reves). false si `move_down_blocker` dice que no se puede.
    pub fn move_down(&mut self) -> bool {
        if self.move_down_blocker().is_some() {
            return false;
        }
        let sel = self.selected;
        if self.layers[sel].parent.is_some() {
            self.layers.swap(sel, sel - 1);
            self.selected = sel - 1;
            return true;
        }
        let (start, end) = self.unit_range(sel);
        let (prev_start, _) = self.unit_range(start - 1);
        // La de abajo (prev_start ..= start - 1) pasa a ir encima de la nuestra.
        self.layers[prev_start..=end].rotate_right(end - start + 1);
        self.selected = sel - (start - prev_start);
        true
    }

    /// Por que no se puede duplicar la capa elegida (`None` si se puede).
    pub fn duplicate_blocker(&self) -> Option<&'static str> {
        if self.layers.len() >= MAX_LAYERS {
            return Some("No caben mas capas");
        }
        if self.layers[self.selected].is_group() {
            return Some("Las carpetas todavia no se pueden duplicar");
        }
        None
    }

    /// Duplica la capa elegida (pintura, mascara, canales y ajustes) justo encima de ella y elige la copia, que lleva un
    /// identificador y un nombre nuevos (y queda en la misma carpeta, si estaba en una). Falla en `MAX_LAYERS` y con las
    /// carpetas (`duplicate_blocker`). Quien llama debe recomponer la textura.
    pub fn duplicate_layer(&mut self) -> bool {
        if self.duplicate_blocker().is_some() {
            return false;
        }
        let mut copy = self.layers[self.selected].clone();
        copy.id = self.next_id;
        copy.name = self.next_name.to_string();
        self.next_id += 1;
        self.next_name += 1;
        self.layers.insert(self.selected + 1, copy);
        self.selected += 1;
        true
    }

    /// Borra la capa elegida y elige la de abajo (o la de arriba si era la primera). Una carpeta se borra con todas las
    /// capas que tiene dentro. Siempre queda al menos una capa: false si borrar dejaria la lista vacia. No se puede
    /// deshacer; los pasos del historial de lo borrado se descartan solos.
    pub fn remove_layer(&mut self) -> bool {
        let sel = self.selected;
        let first = if self.layers[sel].is_group() {
            self.group_start(sel)
        } else {
            sel
        };
        if self.layers.len() <= sel - first + 1 {
            return false;
        }
        self.layers.drain(first..=sel);
        self.selected = first.saturating_sub(1).min(self.layers.len() - 1);
        true
    }

    /// Por que la capa elegida no se puede combinar con la de abajo (`None` si se puede).
    pub fn merge_down_blocker(&self) -> Option<&'static str> {
        if self.selected == 0 {
            return Some("No hay ninguna capa debajo");
        }
        let (low, up) = (&self.layers[self.selected - 1], &self.layers[self.selected]);
        if low.is_group() || up.is_group() {
            return Some("Las carpetas no se pueden combinar");
        }
        if low.parent != up.parent {
            return Some("Las dos capas tienen que estar en la misma carpeta (o las dos sueltas)");
        }
        if !low.visible || !up.visible {
            return Some("Las dos capas tienen que estar visibles");
        }
        if low.locked || up.locked {
            return Some("Las dos capas tienen que estar desbloqueadas");
        }
        if up.clip_to_below {
            return Some("La capa de arriba esta recortada a la de abajo: quita el recorte primero");
        }
        None
    }

    /// Combina la capa elegida con la de abajo en una sola (la de abajo, que se queda con su nombre, su modo de mezcla y
    /// su recorte). La mascara y la opacidad de las dos pasan al alfa de los pixeles, la capa de color liso se vuelve
    /// pintura y los canales (rugosidad...) se juntan igual; la capa resultante no tiene mascara y queda al 100 %.
    /// Lleva un identificador nuevo: los pasos del historial de las dos capas ya no valen y se descartan solos.
    /// false si `merge_down_blocker` dice que no se puede. No se puede deshacer.
    pub fn merge_down(&mut self) -> bool {
        if self.merge_down_blocker().is_some() {
            return false;
        }
        let sel = self.selected;
        let len = (self.size * self.size * 4) as usize;
        let (low, up) = (&self.layers[sel - 1], &self.layers[sel]);
        let mut pixels = vec![0u8; len];
        for i in (0..len).step_by(4) {
            let m = merge_texel(baked_texel(low, None, i), baked_texel(up, None, i), up.blend);
            pixels[i..i + 4].copy_from_slice(&m);
        }
        let mut chans: [Option<Vec<u8>>; 3] = [None, None, None];
        for (slot, out_chan) in chans.iter_mut().enumerate() {
            if low.chans[slot].is_none() && up.chans[slot].is_none() {
                continue;
            }
            let mut out = vec![0u8; len];
            for i in (0..len).step_by(4) {
                let m = merge_texel(
                    baked_texel(low, Some(slot), i),
                    baked_texel(up, Some(slot), i),
                    BlendMode::Normal,
                );
                out[i..i + 4].copy_from_slice(&m);
            }
            *out_chan = Some(out);
        }
        let id = self.next_id;
        self.next_id += 1;
        let merged = &mut self.layers[sel - 1];
        merged.id = id;
        merged.source = LayerSource::Paint;
        merged.pixels = pixels;
        merged.chans = chans;
        merged.mask = None;
        merged.editing_mask = false;
        merged.opacity = 100;
        self.layers.remove(sel);
        self.selected = sel - 1;
        true
    }

    /// Con el Modo Solo encendido, el id de la unica capa suelta o carpeta que se ve en pantalla: la elegida o, si la
    /// elegida esta dentro de una carpeta, esa carpeta (con todas sus capas, para no rehacer lo que guarda). `None` si
    /// el Modo Solo esta apagado. Solo cambia lo que se VE: exportar y guardar usan siempre todas las capas.
    pub fn solo_top(&self) -> Option<u32> {
        if !self.solo {
            return None;
        }
        let l = &self.layers[self.selected];
        Some(l.parent.unwrap_or(l.id))
    }

    /// El Modo Solo cambio lo que hay que ver desde la ultima vez que se compuso todo (se encendio o apago, o se
    /// eligio otra capa mientras estaba encendido): hay que recomponer. Lo consulta `main` en cada cuadro.
    pub fn solo_dirty(&self) -> bool {
        self.solo_top() != self.solo_seen
    }

    /// Recompone todo el mapa.
    pub fn composite_all(&mut self) {
        self.solo_seen = self.solo_top();
        let m = self.size - 1;
        self.composite_rect((0, 0, m, m));
    }

    /// Color final de un texel (indice `i` en bytes) sobre el fondo `bg`, con las reglas de la seccion 5.
    /// `slot` = `None` compone el color; `Some(s)` compone la hoja del canal extra `s` (las capas que no la tienen
    /// cuentan como vacias). Es la lista completa de capas: las que estan dentro de una carpeta no se dibujan por su
    /// cuenta (las junta la carpeta, ver `update_group_caches`). `solo` = el id de la unica capa suelta o carpeta que se
    /// dibuja (Modo Solo, ver `solo_top`); `None` dibuja todas.
    fn blend_texel(
        layers: &[PaintLayer],
        i: usize,
        bg: [u8; 4],
        slot: Option<usize>,
        solo: Option<u32>,
    ) -> [u8; 4] {
        Self::blend_texel_in(layers, i, bg, slot, true, solo)
    }

    /// Lo mismo con `top_level = false` para componer las capas DE una carpeta entre ellas (todas se dibujan; `solo`
    /// no cuenta ahi).
    fn blend_texel_in(
        layers: &[PaintLayer],
        i: usize,
        bg: [u8; 4],
        slot: Option<usize>,
        top_level: bool,
        solo: Option<u32>,
    ) -> [u8; 4] {
        const EMPTY: [u8; 4] = [0; 4];
        let mut acc = bg;
        // Alfa del pixel de la ultima capa no recortada del grupo (0 si esa capa esta oculta).
        let mut base_a = 0u32;
        for (n, layer) in layers.iter().enumerate() {
            if top_level && (layer.parent.is_some() || solo.is_some_and(|s| layer.id != s)) {
                continue;
            }
            // Una capa de color liso aporta ese color, opaco, en todas partes (solo en el color: los canales usan sus hojas).
            let solid: [u8; 4];
            let p: &[u8] = match (slot, layer.source) {
                (None, LayerSource::Solid(c)) => {
                    solid = [c[0], c[1], c[2], 255];
                    &solid
                }
                // En una carpeta, `pixels` y `chans` son lo que dejo componer sus capas de dentro.
                (None, LayerSource::Paint | LayerSource::Group) => &layer.pixels[i..i + 4],
                (Some(s), _) => match &layer.chans[s] {
                    Some(v) => &v[i..i + 4],
                    None => &EMPTY,
                },
            };
            // En el Modo Solo la capa que se ve no se recorta contra una base que no se dibuja.
            let clipped = layer.clip_to_below && n > 0 && solo.is_none();
            // Alfa del pixel ya con la mascara: asi una capa recortada tambien queda limitada por la mascara de su base.
            let pa = match layer.active_mask() {
                Some(m) => p[3] as u32 * m.value(i) / 255,
                None => p[3] as u32,
            };
            if !clipped {
                base_a = if layer.visible { pa } else { 0 };
            }
            if !layer.visible || layer.opacity == 0 {
                continue;
            }
            let mut sa = pa * layer.opacity as u32 / 100;
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

    /// Pone al dia, en el rectangulo, lo que guarda cada carpeta: el resultado de componer sus capas de dentro entre
    /// ellas (con las reglas de siempre, sobre transparente: lo que hagan los modos de mezcla de las de dentro se queda
    /// dentro de la carpeta). Es el color y, en los canales (rugosidad...) en que alguna de dentro tiene pintura, la
    /// hoja de ese canal; los canales en que ya no hay nada se sueltan. Despues la carpeta se compone como una capa mas.
    fn update_group_caches(&mut self, (x0, y0, x1, y1): Rect) {
        let size = self.size;
        let len = (size * size * 4) as usize;
        for k in 0..self.layers.len() {
            if !self.layers[k].is_group() {
                continue;
            }
            let first = self.group_start(k);
            let (below, rest) = self.layers.split_at_mut(k);
            let kids = &below[first..k];
            let folder = &mut rest[0];
            for s in 0..3 {
                if kids.iter().any(|c| c.chans[s].is_some()) {
                    folder.chans[s].get_or_insert_with(|| vec![0; len]);
                } else {
                    folder.chans[s] = None;
                }
            }
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let i = ((y * size + x) * 4) as usize;
                    let c = Self::blend_texel_in(kids, i, [0; 4], None, false, None);
                    folder.pixels[i..i + 4].copy_from_slice(&c);
                    for s in 0..3 {
                        if let Some(buf) = folder.chans[s].as_mut() {
                            let g = Self::blend_texel_in(kids, i, [0; 4], Some(s), false, None);
                            buf[i..i + 4].copy_from_slice(&g);
                        }
                    }
                }
            }
        }
    }

    /// Recompone solo el rectangulo (extremos incluidos). Mismas reglas de la seccion 5 para pantalla y exportacion.
    /// Si la zona es grande (cambiar la opacidad, el modo de mezcla, el fondo...) se reparte por franjas de filas entre
    /// los hilos del procesador; los toques pequenos del pincel se hacen en un solo hilo, donde crear hilos no compensa.
    pub fn composite_rect(&mut self, (x0, y0, x1, y1): Rect) {
        // Primero las carpetas (cada una junta sus capas de dentro) y despues todo se compone como capas normales.
        self.update_group_caches((x0, y0, x1, y1));
        let solo = self.solo_top();
        let size = self.size;
        let stride = (size * 4) as usize;
        let rows = (y1 - y0 + 1) as usize;
        let area = (x1 - x0 + 1) as usize * rows;
        let layers: &[PaintLayer] = &self.layers;
        let bg = self.background;
        // Con la vista de material la textura compuesta es siempre la de color (el material va en otra textura).
        let channel = if self.material_view {
            ChannelKind::Color
        } else {
            self.channel
        };
        let out = &mut self.composite[y0 as usize * stride..(y1 as usize + 1) * stride];

        let threads = if area < 65_536 {
            1
        } else {
            std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .min(rows)
        };
        if threads <= 1 {
            composite_rows(layers, bg, channel, solo, size, y0, out, x0, x1);
            return;
        }
        let band = rows.div_ceil(threads);
        std::thread::scope(|s| {
            for (n, chunk) in out.chunks_mut(band * stride).enumerate() {
                let y_start = y0 + (n * band) as u32;
                s.spawn(move || {
                    composite_rows(layers, bg, channel, solo, size, y_start, chunk, x0, x1)
                });
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
                out[i..i + 4].copy_from_slice(&Self::blend_texel(&self.layers, i, bg, None, None));
            }
        }
        out
    }

    /// Textura completa de un canal extra para exportar: grises opacos, y donde nadie pinta el canal vale su valor por
    /// defecto. `None` si es el color o si ninguna capa tiene pintado ese canal (no hay nada que exportar).
    pub fn export_channel(&self, kind: ChannelKind) -> Option<Vec<u8>> {
        let slot = kind.slot()?;
        let painted = self.layers.iter().any(|l| {
            l.chans[slot]
                .as_ref()
                .is_some_and(|v| v.chunks_exact(4).any(|p| p[3] != 0))
        });
        if !painted {
            return None;
        }
        let d = kind.default_value();
        let mut out = vec![0u8; (self.size * self.size * 4) as usize];
        for i in (0..out.len()).step_by(4) {
            out[i..i + 4]
                .copy_from_slice(&Self::blend_texel(&self.layers, i, [d, d, d, 255], Some(slot), None));
        }
        Some(out)
    }

    /// Mapa de normales (formato OpenGL, el de Blender: verde = hacia +V) calculado a partir de la altura compuesta.
    /// Plano = (128, 128, 255). `strength` es la intensidad del relieve. `None` si ninguna capa tiene pintada la altura.
    pub fn export_normal(&self, strength: f32) -> Option<Vec<u8>> {
        let height = self.export_channel(ChannelKind::Height)?;
        let n = self.size as i64;
        // Altura 0..1 del texel; fuera de la textura se repite el borde.
        let h = |x: i64, y: i64| -> f32 {
            let (x, y) = (x.clamp(0, n - 1), y.clamp(0, n - 1));
            height[((y * n + x) * 4) as usize] as f32 / 255.0
        };
        let mut out = vec![255u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                // Diferencias centrales: cuanto cambia la altura por texel hacia +U y hacia +V (las filas crecen con V).
                let dx = (h(x + 1, y) - h(x - 1, y)) * 0.5;
                let dy = (h(x, y + 1) - h(x, y - 1)) * 0.5;
                let (nx, ny, nz) = (-dx * strength, -dy * strength, 1.0f32);
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                let i = ((y * n + x) * 4) as usize;
                out[i] = ((nx / len * 0.5 + 0.5) * 255.0).round() as u8;
                out[i + 1] = ((ny / len * 0.5 + 0.5) * 255.0).round() as u8;
                out[i + 2] = ((nz / len * 0.5 + 0.5) * 255.0).round() as u8;
            }
        }
        Some(out)
    }

    /// Textura de material del rectangulo (para subirla a la GPU): R = rugosidad, G = metalico, B = altura, A = 255.
    /// Cada canal se compone con las mismas reglas que el color (mascara, opacidad, mezcla, recorte).
    pub fn material_bytes(&self, (x0, y0, x1, y1): Rect) -> Vec<u8> {
        let solo = self.solo_top();
        let mut out = Vec::with_capacity(((x1 - x0 + 1) * (y1 - y0 + 1) * 4) as usize);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = ((y * self.size + x) * 4) as usize;
                let mut px = [0u8, 0, 0, 255];
                for k in ChannelKind::EXTRA {
                    let Some(s) = k.slot() else { continue };
                    let d = k.default_value();
                    px[s] = Self::blend_texel(&self.layers, i, [d, d, d, 255], Some(s), solo)[0];
                }
                out.extend_from_slice(&px);
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
    channel: ChannelKind,
    solo: Option<u32>,
    size: u32,
    y_start: u32,
    rows: &mut [u8],
    x0: u32,
    x1: u32,
) {
    let stride = (size * 4) as usize;
    let slot = channel.slot();
    // En un canal en grises el fondo es el valor de "nadie pinta este canal", liso y opaco.
    let flat = channel.default_value();
    for (k, row) in rows.chunks_mut(stride).enumerate() {
        let y = y_start + k as u32;
        for x in x0..=x1 {
            let i = ((y * size + x) * 4) as usize;
            let base = match slot {
                None => bg.pixel(x, y),
                Some(_) => [flat, flat, flat, 255],
            };
            let acc = LayerStack::blend_texel(layers, i, base, slot, solo);
            let o = (x * 4) as usize;
            row[o..o + 4].copy_from_slice(&acc);
        }
    }
}

/// Deja bien las carpetas de unas capas cargadas: las capas de una carpeta tienen que ir seguidas justo debajo de ella y
/// una carpeta no va dentro de otra. Si algo no lo cumple (un proyecto raro), esas capas quedan sueltas en vez de
/// desaparecer de la vista (las capas de dentro no se dibujan por su cuenta).
fn fix_folders(layers: &mut [PaintLayer]) {
    for k in 0..layers.len() {
        let Some(p) = layers[k].parent else {
            continue;
        };
        let mut j = k + 1;
        while j < layers.len() && layers[j].parent == Some(p) {
            j += 1;
        }
        let ok = !layers[k].is_group()
            && j < layers.len()
            && layers[j].id == p
            && layers[j].is_group()
            && layers[j].parent.is_none();
        if !ok {
            layers[k].parent = None;
        }
    }
}

/// Texel (RGBA) de la capa en el indice `i` (en bytes) con todo "horneado": la capa de color liso aporta su color, y la
/// mascara y la opacidad de la capa pasan al alfa. `slot` = canal extra (`None` = el color). Lo usa `merge_down`.
fn baked_texel(layer: &PaintLayer, slot: Option<usize>, i: usize) -> [u8; 4] {
    let mut p = match (slot, layer.source) {
        (None, LayerSource::Solid(c)) => [c[0], c[1], c[2], 255],
        (None, LayerSource::Paint | LayerSource::Group) => [
            layer.pixels[i],
            layer.pixels[i + 1],
            layer.pixels[i + 2],
            layer.pixels[i + 3],
        ],
        (Some(s), _) => match &layer.chans[s] {
            Some(v) => [v[i], v[i + 1], v[i + 2], v[i + 3]],
            None => [0, 0, 0, 0],
        },
    };
    let mut a = p[3] as u32;
    if let Some(m) = layer.active_mask() {
        a = a * m.value(i) / 255;
    }
    p[3] = (a * layer.opacity as u32 / 100) as u8;
    p
}

/// `up` sobre `low` (los dos ya horneados) con el modo de mezcla dado. Donde `up` no tiene nada queda `low` tal cual.
fn merge_texel(low: [u8; 4], up: [u8; 4], mode: BlendMode) -> [u8; 4] {
    if up[3] == 0 {
        return low;
    }
    over(low, [up[0], up[1], up[2]], up[3] as u32, mode)
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

    /// Pone un texel de la mascara de la capa 0 a un gris (opaco).
    fn set_mask(stack: &mut LayerStack, x: u32, y: u32, v: u8) {
        let i = ((y * stack.size + x) * 4) as usize;
        let m = stack.layers[0].mask.as_mut().unwrap();
        m.pixels[i..i + 4].copy_from_slice(&[v, v, v, 255]);
    }

    #[test]
    fn mascara_nueva_es_blanca_y_no_oculta_nada() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        assert!(s.add_mask());
        // Una capa solo tiene una mascara.
        assert!(!s.add_mask());
        assert!(s.layers[0].painting_mask());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn mascara_negra_oculta_y_gris_deja_a_medias() {
        let mut s = LayerStack::new(64);
        for x in 5..=7 {
            set_px(&mut s.layers[0], 64, x, 5, [255, 0, 0, 255]);
        }
        s.add_mask();
        set_mask(&mut s, 5, 5, 0);
        set_mask(&mut s, 6, 5, 128);
        s.composite_all();
        // Negro: se ve el fondo. Blanco: rojo entero. Gris: la mitad de rojo sobre blanco.
        assert_eq!(px(&s, 5, 5), [255, 255, 255, 255]);
        assert_eq!(px(&s, 7, 5), [255, 0, 0, 255]);
        let g = px(&s, 6, 5);
        assert!((g[1] as i32 - 127).abs() <= 2, "gris = {g:?}");
    }

    #[test]
    fn mascara_invertida_o_apagada() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.add_mask();
        set_mask(&mut s, 5, 5, 0);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 255, 255, 255]);
        // Invertida: el negro muestra.
        assert!(s.invert_mask());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
        // Apagada: no oculta nada aunque este invertida o negra.
        s.invert_mask();
        assert!(s.toggle_mask_enabled());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
    }

    #[test]
    fn quitar_la_mascara_devuelve_la_pintura_y_el_pincel_a_la_capa() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.add_mask();
        set_mask(&mut s, 5, 5, 0);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 255, 255, 255]);
        assert!(s.remove_mask());
        assert!(!s.layers[0].painting_mask());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
        // Sin mascara no hay nada que invertir ni apagar.
        assert!(!s.invert_mask() && !s.toggle_mask_enabled() && !s.remove_mask());
    }

    #[test]
    fn el_pincel_apunta_a_la_mascara_solo_mientras_se_edita() {
        let mut s = LayerStack::new(16);
        let capa_len = s.layers[0].pixels.len();
        s.add_mask();
        // Mascara y capa tienen el mismo tamano; la mascara empieza blanca y la capa vacia.
        let t = s.layers[0].target(ChannelKind::Color);
        assert_eq!(t, PaintTarget::Mask);
        assert_eq!(s.layers[0].target_pixels_mut(t)[0], 255);
        s.layers[0].editing_mask = false;
        let t = s.layers[0].target(ChannelKind::Color);
        assert_eq!(t, PaintTarget::Layer);
        assert_eq!(s.layers[0].target_pixels_mut(t).len(), capa_len);
        assert_eq!(s.layers[0].target_pixels_mut(t)[0], 0);
    }

    #[test]
    fn el_material_junta_los_tres_canales_y_la_vista_de_material_deja_el_color() {
        let mut s = LayerStack::new(64);
        // Sin pintar: rugosidad y altura a la mitad, nada de metal, opaco.
        assert_eq!(s.material_bytes((0, 0, 0, 0)), vec![128, 0, 128, 255]);
        let t = s.layers[0].target(ChannelKind::Metallic);
        let i = ((1 * 64 + 2) * 4) as usize;
        s.layers[0].target_pixels_mut(t)[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        // Texel (2,1): metal al maximo, lo demas igual.
        assert_eq!(s.material_bytes((2, 1, 2, 1)), vec![128, 255, 128, 255]);
        // Con la vista de material, aunque se pinte un canal, la textura compuesta sigue siendo el color.
        s.channel = ChannelKind::Metallic;
        s.material_view = true;
        s.composite_all();
        assert_eq!(px(&s, 2, 1), [255, 255, 255, 255]);
        assert_eq!(px(&s, 32, 0), [0xDE, 0xDE, 0xDE, 255]);
    }

    #[test]
    fn el_mapa_de_normales_sale_de_la_altura() {
        let mut s = LayerStack::new(64);
        // Sin altura pintada no hay nada que exportar.
        assert!(s.export_normal(NORMAL_STRENGTH).is_none());
        // Una rampa que sube hacia +U en la fila 5: alturas 100, 150, 200 en los texeles 10, 11 y 12.
        let t = s.layers[0].target(ChannelKind::Height);
        assert_eq!(t, PaintTarget::Channel(2));
        for (x, v) in [(10u32, 100u8), (11, 150), (12, 200)] {
            let i = ((5 * 64 + x) * 4) as usize;
            s.layers[0].target_pixels_mut(t)[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
        let img = s.export_normal(NORMAL_STRENGTH).unwrap();
        // En la rampa la normal se inclina hacia -U: rojo bajo, azul todavia alto.
        let i = ((5 * 64 + 11) * 4) as usize;
        assert!(img[i] < 64, "rojo = {}", img[i]);
        assert!(img[i + 2] > 128, "azul = {}", img[i + 2]);
        // Lejos de lo pintado el relieve es plano: (128, 128, 255), siempre opaco.
        let j = ((40 * 64 + 40) * 4) as usize;
        assert_eq!(&img[j..j + 4], &[128, 128, 255, 255]);
    }

    #[test]
    fn exportar_un_canal_solo_si_tiene_pintura() {
        let mut s = LayerStack::new(64);
        assert!(s.export_channel(ChannelKind::Color).is_none());
        assert!(s.export_channel(ChannelKind::Metallic).is_none());
        let t = s.layers[0].target(ChannelKind::Metallic);
        let i = ((2 * 64 + 3) * 4) as usize;
        s.layers[0].target_pixels_mut(t)[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        let img = s.export_channel(ChannelKind::Metallic).unwrap();
        // Donde se pinto: blanco. Donde no: el metalico por defecto (negro), siempre opaco.
        assert_eq!(&img[i..i + 4], &[255, 255, 255, 255]);
        assert_eq!(&img[0..4], &[0, 0, 0, 255]);
        // Otro canal sigue sin nada que exportar.
        assert!(s.export_channel(ChannelKind::Height).is_none());
    }

    #[test]
    fn un_canal_se_crea_al_pintar_y_se_ve_en_grises() {
        let mut s = LayerStack::new(64);
        // Sin pintar, el canal no gasta memoria y se ve plano (rugosidad = 128).
        assert!(s.layers[0].chans[0].is_none());
        s.channel = ChannelKind::Roughness;
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [128, 128, 128, 255]);
        // El destino del pincel es la hoja del canal; se crea vacia y se pinta un texel gris oscuro.
        let t = s.layers[0].target(ChannelKind::Roughness);
        assert_eq!(t, PaintTarget::Channel(0));
        let i = ((5 * 64 + 5) * 4) as usize;
        s.layers[0].target_pixels_mut(t)[i..i + 4].copy_from_slice(&[20, 20, 20, 255]);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [20, 20, 20, 255]);
        // El color de la capa no se toco: al volver a Color se ve el fondo.
        s.channel = ChannelKind::Color;
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 255, 255, 255]);
        // La mascara tiene prioridad sobre el canal.
        s.add_mask();
        assert_eq!(s.layers[0].target(ChannelKind::Roughness), PaintTarget::Mask);
    }

    #[test]
    fn la_mascara_de_la_base_limita_a_la_capa_recortada() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        // Base roja en (3,3) y (4,4); capa recortada azul en los mismos texeles.
        for (x, y) in [(3, 3), (4, 4)] {
            set_px(&mut s.layers[0], 64, x, y, [255, 0, 0, 255]);
            set_px(&mut s.layers[1], 64, x, y, [0, 0, 255, 255]);
        }
        s.layers[1].clip_to_below = true;
        // La base se oculta en (3,3): ahi la recortada tampoco debe verse.
        s.selected = 0;
        s.add_mask();
        set_mask(&mut s, 3, 3, 0);
        s.composite_all();
        assert_eq!(px(&s, 3, 3), s.background.pixel(3, 3));
        assert_eq!(px(&s, 4, 4), [0, 0, 255, 255]);
    }

    #[test]
    fn una_capa_de_color_liso_empieza_oculta_y_se_ve_donde_la_mascara_es_blanca() {
        let mut s = LayerStack::new(64);
        assert!(s.add_solid_layer([200, 30, 40]));
        // Recien creada: mascara negra, se pinta en la mascara y todavia no se ve nada.
        assert_eq!(s.layers[1].source, LayerSource::Solid([200, 30, 40]));
        assert!(s.layers[1].painting_mask());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), s.background.pixel(5, 5));
        // Se "pinta" blanco en la mascara en (5,5): ahi aparece el color, entero.
        let i = ((5 * 64 + 5) * 4) as usize;
        s.layers[1].mask.as_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [200, 30, 40, 255]);
        assert_eq!(px(&s, 6, 5), s.background.pixel(6, 5));
    }

    #[test]
    fn se_cambia_el_color_de_la_capa_lisa_sin_tocar_la_mascara() {
        let mut s = LayerStack::new(64);
        s.add_solid_layer([200, 30, 40]);
        let i = ((5 * 64 + 5) * 4) as usize;
        s.layers[1].mask.as_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        assert!(s.set_solid_color([10, 20, 250]));
        // Mismo color otra vez: nada que cambiar.
        assert!(!s.set_solid_color([10, 20, 250]));
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [10, 20, 250, 255]);
        // En una capa de pintura normal no hay color que cambiar.
        s.selected = 0;
        assert!(!s.set_solid_color([1, 2, 3]));
    }

    #[test]
    fn en_una_capa_lisa_solo_se_pinta_la_mascara_o_un_canal() {
        let mut s = LayerStack::new(64);
        s.add_solid_layer([1, 2, 3]);
        let l = &mut s.layers[1];
        // Pintando la mascara: vale. Los pixeles de color de la capa: no.
        assert!(l.accepts(ChannelKind::Color));
        l.editing_mask = false;
        assert!(!l.accepts(ChannelKind::Color));
        // Un canal (rugosidad) si se pinta, como en cualquier capa.
        assert!(l.accepts(ChannelKind::Roughness));
        // Una capa de pintura normal acepta todo; oculta o bloqueada, nada.
        assert!(s.layers[0].accepts(ChannelKind::Color));
        s.layers[0].locked = true;
        assert!(!s.layers[0].accepts(ChannelKind::Color));
    }

    #[test]
    fn la_capa_lisa_no_cambia_los_canales_ni_el_resto_de_capas() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.add_solid_layer([0, 0, 255]);
        // Sin mascara la capa lisa tapa todo el color de abajo (la capa elegida es la lisa).
        assert!(s.remove_mask());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
        // Rugosidad: la capa lisa no aporta nada a los canales (sin pintar vale su valor por defecto).
        assert!(s.export_channel(ChannelKind::Roughness).is_none());
    }

    #[test]
    fn duplicar_copia_todo_con_otro_id_y_la_copia_es_independiente() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.layers[0].opacity = 40;
        s.layers[0].chans[0] = Some(vec![7; 64 * 64 * 4]);
        s.add_mask();
        set_mask(&mut s, 6, 6, 0);
        assert!(s.duplicate_layer());
        assert_eq!(s.layers.len(), 2);
        // La copia queda encima y elegida, con id y nombre nuevos.
        assert_eq!(s.selected, 1);
        assert_ne!(s.layers[0].id, s.layers[1].id);
        assert_ne!(s.layers[0].name, s.layers[1].name);
        // Pintura, ajustes, canal y mascara iguales.
        assert_eq!(s.layers[1].pixels, s.layers[0].pixels);
        assert_eq!(s.layers[1].opacity, 40);
        assert_eq!(s.layers[1].chans[0], s.layers[0].chans[0]);
        assert_eq!(
            s.layers[1].mask.as_ref().unwrap().pixels,
            s.layers[0].mask.as_ref().unwrap().pixels
        );
        // Cambiar la copia no toca la original.
        s.layers[1].pixels[0] = 9;
        s.layers[1].mask.as_mut().unwrap().pixels[0] = 9;
        assert_ne!(s.layers[0].pixels[0], 9);
        assert_ne!(s.layers[0].mask.as_ref().unwrap().pixels[0], 9);
    }

    #[test]
    fn duplicar_respeta_el_maximo_de_capas() {
        let mut s = LayerStack::new(16);
        for _ in 0..MAX_LAYERS - 1 {
            s.add_layer();
        }
        assert!(!s.duplicate_layer());
        assert_eq!(s.layers.len(), MAX_LAYERS);
    }

    #[test]
    fn borrar_capa_ajusta_la_eleccion_y_siempre_queda_una() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        s.add_layer();
        let ids: Vec<u32> = s.layers.iter().map(|l| l.id).collect();
        // Borrar la del medio: queda elegida la de abajo.
        s.selected = 1;
        assert!(s.remove_layer());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [ids[0], ids[2]]);
        assert_eq!(s.selected, 0);
        // Borrar la primera: queda elegida la que era la de arriba.
        assert!(s.remove_layer());
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.layers[0].id, ids[2]);
        assert_eq!(s.selected, 0);
        // La ultima no se borra.
        assert!(!s.remove_layer());
        assert_eq!(s.layers.len(), 1);
    }

    /// Dos buffers iguales salvo por redondeos de hasta `tol` por byte.
    fn parecido(a: &[u8], b: &[u8], tol: i32) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (*x as i32 - *y as i32).abs() <= tol)
    }

    #[test]
    fn combinar_hacia_abajo_no_cambia_lo_que_se_ve() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        set_px(&mut s.layers[0], 64, 8, 8, [0, 255, 0, 255]);
        set_px(&mut s.layers[1], 64, 5, 5, [0, 0, 255, 255]);
        set_px(&mut s.layers[1], 64, 6, 6, [0, 0, 255, 255]);
        s.layers[1].opacity = 50;
        s.composite_all();
        let antes = s.composite.clone();
        let id_viejo = s.layers[0].id;
        assert!(s.merge_down());
        assert_eq!(s.layers.len(), 1);
        assert_eq!(s.selected, 0);
        // Id nuevo (el historial viejo ya no vale) y la opacidad quedo horneada.
        assert_ne!(s.layers[0].id, id_viejo);
        assert_eq!(s.layers[0].opacity, 100);
        s.composite_all();
        assert!(parecido(&s.composite, &antes, 1));
    }

    #[test]
    fn combinar_hornea_las_mascaras_y_vuelve_pintura_a_la_capa_lisa() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.add_solid_layer([0, 0, 255]);
        // La mascara negra de la capa lisa solo deja ver el color en (5,5) y (6,6).
        for (x, y) in [(5, 5), (6, 6)] {
            let i = ((y * 64 + x) * 4) as usize;
            s.layers[1].mask.as_mut().unwrap().pixels[i..i + 4]
                .copy_from_slice(&[255, 255, 255, 255]);
        }
        s.composite_all();
        let antes = s.composite.clone();
        assert!(s.merge_down());
        assert!(s.layers[0].mask.is_none());
        assert_eq!(s.layers[0].source, LayerSource::Paint);
        assert!(!s.layers[0].painting_mask());
        s.composite_all();
        assert!(parecido(&s.composite, &antes, 1));
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
        assert_eq!(px(&s, 6, 6), [0, 0, 255, 255]);
        assert_eq!(px(&s, 7, 7), s.background.pixel(7, 7));
    }

    #[test]
    fn combinar_junta_tambien_los_canales() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        let n = 64 * 64 * 4;
        let i = ((5 * 64 + 5) * 4) as usize;
        let (mut abajo, mut arriba) = (vec![0u8; n], vec![0u8; n]);
        abajo[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        arriba[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        s.layers[0].chans[0] = Some(abajo);
        s.layers[1].chans[0] = Some(arriba);
        s.layers[1].opacity = 50;
        s.composite_all();
        let antes = s.export_channel(ChannelKind::Roughness).unwrap();
        assert!(s.merge_down());
        s.composite_all();
        let despues = s.export_channel(ChannelKind::Roughness).unwrap();
        assert!(parecido(&antes, &despues, 1));
        // Los canales que ninguna de las dos pintaba siguen sin pintar.
        assert!(s.layers[0].chans[1].is_none() && s.layers[0].chans[2].is_none());
    }

    #[test]
    fn combinar_avisa_por_que_no_se_puede() {
        let mut s = LayerStack::new(64);
        // Una sola capa: no hay nada debajo.
        assert!(s.merge_down_blocker().is_some());
        assert!(!s.merge_down());
        s.add_layer();
        assert!(s.merge_down_blocker().is_none());
        s.layers[1].clip_to_below = true;
        assert!(s.merge_down_blocker().is_some());
        s.layers[1].clip_to_below = false;
        s.layers[0].visible = false;
        assert!(s.merge_down_blocker().is_some());
        s.layers[0].visible = true;
        s.layers[0].locked = true;
        assert!(s.merge_down_blocker().is_some());
        // Con un aviso, combinar no hace nada.
        assert!(!s.merge_down());
        assert_eq!(s.layers.len(), 2);
    }

    #[test]
    fn una_carpeta_con_una_capa_se_ve_igual_que_la_capa_suelta() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.composite_all();
        let antes = s.composite.clone();
        assert!(s.add_folder());
        // [capa (dentro de F), F]: la carpeta queda justo encima y elegida.
        assert_eq!(s.layers.len(), 2);
        assert!(s.layers[1].is_group());
        assert_eq!(s.layers[0].parent, Some(s.layers[1].id));
        assert_eq!(s.selected, 1);
        s.composite_all();
        assert_eq!(s.composite, antes);
    }

    #[test]
    fn la_opacidad_y_el_ojo_de_la_carpeta_valen_para_todo_lo_de_dentro() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.add_folder();
        s.layers[1].opacity = 50;
        s.composite_all();
        let p = px(&s, 5, 5);
        assert!((p[1] as i32 - 127).abs() <= 1, "p = {p:?}");
        // Carpeta oculta: no se ve nada de lo de dentro.
        s.layers[1].opacity = 100;
        s.layers[1].visible = false;
        s.composite_all();
        assert_eq!(px(&s, 5, 5), s.background.pixel(5, 5));
        // El ojo de la capa de dentro apaga solo esa capa (aunque la carpeta se vea).
        s.layers[1].visible = true;
        s.layers[0].visible = false;
        s.composite_all();
        assert_eq!(px(&s, 5, 5), s.background.pixel(5, 5));
    }

    #[test]
    fn lo_que_mezclan_las_capas_de_dentro_se_queda_dentro_de_la_carpeta() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        s.add_layer();
        set_px(&mut s.layers[1], 64, 5, 5, [128, 128, 128, 255]);
        s.layers[1].blend = BlendMode::Multiply;
        // Suelta, el gris multiplica contra el rojo de abajo.
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [128, 0, 0, 255]);
        // Dentro de una carpeta solo ve a sus hermanas (aqui ninguna): se dibuja gris y la carpeta lo pone sobre el rojo.
        assert!(s.add_folder());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [128, 128, 128, 255]);
    }

    #[test]
    fn la_mascara_de_la_carpeta_oculta_las_capas_de_dentro() {
        let mut s = LayerStack::new(64);
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        set_px(&mut s.layers[0], 64, 6, 6, [255, 0, 0, 255]);
        s.add_folder();
        assert!(s.add_mask());
        let i = ((5 * 64 + 5) * 4) as usize;
        s.layers[1].mask.as_mut().unwrap().pixels[i..i + 4].copy_from_slice(&[0, 0, 0, 255]);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), s.background.pixel(5, 5));
        assert_eq!(px(&s, 6, 6), [255, 0, 0, 255]);
    }

    #[test]
    fn en_una_carpeta_solo_se_pinta_la_mascara() {
        let mut s = LayerStack::new(64);
        s.add_folder();
        assert!(!s.layers[1].accepts(ChannelKind::Color));
        assert!(!s.layers[1].accepts(ChannelKind::Roughness));
        // Con mascara, se puede pintar la mascara; la capa de dentro se pinta como siempre.
        s.add_mask();
        assert!(s.layers[1].accepts(ChannelKind::Color));
        assert!(s.layers[0].accepts(ChannelKind::Color));
    }

    #[test]
    fn las_capas_nuevas_dentro_de_una_carpeta_se_quedan_dentro_y_se_puede_salir_y_entrar() {
        let mut s = LayerStack::new(64);
        assert!(s.add_folder()); // [L0 (en F), F]
        let (id0, f_id) = (s.layers[0].id, s.layers[1].id);
        s.selected = 0;
        assert!(s.add_layer()); // [L0, L1, F]: la nueva queda dentro, entre su hermana y la carpeta.
        assert_eq!(s.layers[1].parent, Some(f_id));
        assert!(s.layers[2].is_group());
        // Salir: la capa queda suelta justo encima de la carpeta y sigue elegida.
        let id1 = s.layers[1].id;
        assert!(s.can_leave_folder());
        assert!(s.leave_folder());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [id0, f_id, id1]);
        assert_eq!(s.selected, 2);
        assert_eq!(s.layers[2].parent, None);
        assert!(!s.can_leave_folder() && !s.leave_folder());
        // Entrar: una capa suelta justo debajo de las de una carpeta puede entrar en ella.
        let mut t = LayerStack::new(64);
        t.add_layer(); // [A, B]
        assert!(t.add_folder()); // B va a la carpeta: [A, B (en F), F]
        let f = t.layers[2].id;
        t.selected = 0;
        assert!(t.can_enter_folder_above());
        assert!(t.enter_folder_above());
        assert_eq!(t.layers[0].parent, Some(f));
        assert!(!t.can_enter_folder_above());
    }

    #[test]
    fn borrar_una_carpeta_se_lleva_las_capas_de_dentro() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        s.add_layer(); // [A, B, C]
        let (a, c) = (s.layers[0].id, s.layers[2].id);
        s.selected = 1;
        assert!(s.add_folder()); // [A, B (en F), F, C]
        assert_eq!(s.layers.len(), 4);
        s.selected = 2;
        assert!(s.layers[2].is_group());
        assert!(s.remove_layer());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, c]);
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn no_se_borra_una_carpeta_si_la_lista_quedaria_vacia() {
        let mut s = LayerStack::new(64);
        s.add_folder(); // [L0 (en F), F]
        assert!(!s.remove_layer());
        assert_eq!(s.layers.len(), 2);
    }

    #[test]
    fn las_carpetas_tienen_sus_limites() {
        let mut s = LayerStack::new(64);
        assert!(s.add_folder_blocker().is_none());
        s.add_folder(); // elegida: la carpeta
        // Una carpeta no va dentro de otra, ni se duplica.
        assert!(s.add_folder_blocker().is_some());
        assert!(s.duplicate_blocker().is_some() && !s.duplicate_layer());
        // La capa de dentro ya esta en una carpeta, pero si se puede duplicar.
        s.selected = 0;
        assert!(s.add_folder_blocker().is_some());
        assert!(s.duplicate_blocker().is_none());
        // Combinar: dos capas de la misma carpeta si; con la carpeta, no.
        s.add_layer(); // [L0, L1, F] con L1 elegida
        assert!(s.merge_down_blocker().is_none());
        s.selected = 2;
        assert!(s.merge_down_blocker().is_some());
        // Y no caben mas capas que `MAX_LAYERS` (la carpeta cuenta como una).
        let mut t = LayerStack::new(16);
        for _ in 0..MAX_LAYERS - 1 {
            t.add_layer();
        }
        assert!(t.add_folder_blocker().is_some() && !t.add_folder());
    }

    #[test]
    fn los_canales_de_las_capas_de_dentro_pasan_por_la_carpeta() {
        let mut s = LayerStack::new(64);
        let i = ((5 * 64 + 5) * 4) as usize;
        let mut hoja = vec![0u8; 64 * 64 * 4];
        hoja[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
        s.layers[0].chans[0] = Some(hoja);
        s.add_folder();
        s.channel = ChannelKind::Roughness;
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 255, 255, 255]);
        assert_eq!(px(&s, 6, 6), [128, 128, 128, 255]);
        // La opacidad de la carpeta tambien vale para los canales: 255 al 50 % sobre 128 son ~191.
        s.layers[1].opacity = 50;
        s.composite_all();
        assert!((px(&s, 5, 5)[0] as i32 - 191).abs() <= 1);
        assert!(s.export_channel(ChannelKind::Roughness).is_some());
        // Si ninguna capa de dentro tiene ya ese canal, la carpeta lo suelta.
        s.layers[0].chans[0] = None;
        s.composite_all();
        assert!(s.layers[1].chans[0].is_none());
        assert!(s.export_channel(ChannelKind::Roughness).is_none());
    }

    #[test]
    fn al_cargar_las_capas_de_una_carpeta_mal_puesta_quedan_sueltas() {
        let size = 16;
        let capa = |id: u32, parent: Option<u32>| {
            let mut l = PaintLayer::new(id, id.to_string(), size);
            l.parent = parent;
            l
        };
        let mut carpeta = capa(10, None);
        carpeta.source = LayerSource::Group;
        // [0 (en 10), 1 (en 10), carpeta 10, 2 (en 10: ya no esta pegada a su carpeta), 3 (en una que no existe)]
        let s = LayerStack::from_saved(
            size,
            vec![
                capa(0, Some(10)),
                capa(1, Some(10)),
                carpeta,
                capa(2, Some(10)),
                capa(3, Some(99)),
            ],
            0,
            Background::White,
        );
        let padres: Vec<Option<u32>> = s.layers.iter().map(|l| l.parent).collect();
        assert_eq!(padres, [Some(10), Some(10), None, None, None]);
    }

    #[test]
    fn subir_y_bajar_cambian_el_orden_y_la_capa_sigue_elegida() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        s.add_layer(); // [A, B, C], elegida C
        let ids: Vec<u32> = s.layers.iter().map(|l| l.id).collect();
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        set_px(&mut s.layers[1], 64, 5, 5, [0, 0, 255, 255]);
        s.composite_all();
        // B (azul) esta encima de A (rojo).
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
        s.selected = 1;
        assert!(s.move_down());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [ids[1], ids[0], ids[2]]);
        assert_eq!(s.selected, 0);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
        // Y de vuelta arriba.
        assert!(s.move_up());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), ids);
        assert_eq!(s.selected, 1);
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
    }

    #[test]
    fn subir_y_bajar_tienen_sus_extremos() {
        let mut s = LayerStack::new(64);
        s.add_layer(); // [A, B], elegida B (la de arriba)
        assert!(s.move_up_blocker().is_some() && !s.move_up());
        assert!(s.move_down_blocker().is_none());
        s.selected = 0;
        assert!(s.move_down_blocker().is_some() && !s.move_down());
        assert!(s.move_up_blocker().is_none());
        assert_eq!(s.layers.len(), 2);
    }

    #[test]
    fn una_carpeta_se_mueve_con_sus_capas_y_las_sueltas_saltan_la_carpeta_entera() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        s.add_layer(); // [A, B, C]
        let (a, b, c) = (s.layers[0].id, s.layers[1].id, s.layers[2].id);
        s.selected = 1;
        assert!(s.add_folder()); // [A, B (en F), F, C]
        let f = s.layers[2].id;
        // Subir la carpeta: pasa por encima de C, con B pegado debajo de ella.
        s.selected = 2;
        assert!(s.move_up());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, c, b, f]);
        assert_eq!(s.selected, 3);
        // Bajarla otra vez.
        assert!(s.move_down());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [a, b, f, c]);
        assert_eq!(s.selected, 2);
        // A (suelta, justo debajo de las de la carpeta) sube saltando la carpeta entera.
        s.selected = 0;
        assert!(s.move_up());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [b, f, a, c]);
        assert_eq!(s.selected, 2);
        // C baja un puesto (pasa por debajo de A) y despues salta la carpeta entera hacia abajo.
        s.selected = 3;
        assert!(s.move_down());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [b, f, c, a]);
        assert!(s.move_down());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [c, b, f, a]);
        assert_eq!(s.selected, 0);
        // La carpeta sigue bien armada: B dentro, pegada debajo de ella.
        assert_eq!(s.layers[1].parent, Some(f));
        assert!(s.layers[2].is_group());
    }

    #[test]
    fn dentro_de_una_carpeta_las_capas_se_mueven_solo_entre_ellas() {
        let mut s = LayerStack::new(64);
        s.add_folder(); // [L0 (en F), F]
        s.selected = 0;
        s.add_layer(); // [L0, L1, F], elegida L1
        let (l0, l1, f) = (s.layers[0].id, s.layers[1].id, s.layers[2].id);
        // L1 es la de arriba de la carpeta: no sube mas (encima esta la carpeta), pero si baja.
        assert!(s.move_up_blocker().is_some() && !s.move_up());
        assert!(s.move_down());
        assert_eq!(s.layers.iter().map(|l| l.id).collect::<Vec<_>>(), [l1, l0, f]);
        assert_eq!(s.selected, 0);
        assert!(s.move_down_blocker().is_some() && !s.move_down());
        assert!(s.move_up());
        assert_eq!(s.selected, 1);
        assert_eq!(s.layers[0].parent, Some(f));
        assert_eq!(s.layers[1].parent, Some(f));
    }

    #[test]
    fn el_modo_solo_muestra_solo_la_capa_elegida_y_no_cambia_lo_exportado() {
        let mut s = LayerStack::new(64);
        s.add_layer(); // [A, B], elegida B
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        set_px(&mut s.layers[0], 64, 6, 6, [255, 0, 0, 255]);
        set_px(&mut s.layers[1], 64, 5, 5, [0, 0, 255, 255]);
        s.composite_all();
        assert!(!s.solo_dirty());
        // Solo B: se ve su azul y la roja de A desaparece.
        s.solo = true;
        assert!(s.solo_dirty());
        s.composite_all();
        assert!(!s.solo_dirty());
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
        assert_eq!(px(&s, 6, 6), s.background.pixel(6, 6));
        // Elegir otra capa con el modo encendido pide recomponer: ahora se ve A sola.
        s.selected = 0;
        assert!(s.solo_dirty());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [255, 0, 0, 255]);
        assert_eq!(px(&s, 6, 6), [255, 0, 0, 255]);
        // Exportar sigue usando todas las capas (B tapa a A en (5,5)).
        let todo = s.export_composite(true);
        let i = ((5 * 64 + 5) * 4) as usize;
        assert_eq!(&todo[i..i + 4], &[0, 0, 255, 255]);
        // Apagarlo vuelve a la vista de siempre.
        s.solo = false;
        assert!(s.solo_dirty());
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
        assert_eq!(px(&s, 6, 6), [255, 0, 0, 255]);
    }

    #[test]
    fn el_modo_solo_en_una_capa_de_una_carpeta_muestra_la_carpeta_entera() {
        let mut s = LayerStack::new(64);
        s.add_layer();
        s.add_layer(); // [A, B, C]
        set_px(&mut s.layers[0], 64, 5, 5, [255, 0, 0, 255]);
        set_px(&mut s.layers[1], 64, 6, 6, [0, 255, 0, 255]);
        set_px(&mut s.layers[2], 64, 7, 7, [0, 0, 255, 255]);
        s.selected = 1;
        assert!(s.add_folder()); // [A, B (en F), F, C]
        s.selected = 1;
        s.solo = true;
        s.composite_all();
        // Se ve la carpeta con lo que tiene dentro; A y C (sueltas) no.
        assert_eq!(px(&s, 6, 6), [0, 255, 0, 255]);
        assert_eq!(px(&s, 5, 5), s.background.pixel(5, 5));
        assert_eq!(px(&s, 7, 7), s.background.pixel(7, 7));
    }

    #[test]
    fn en_el_modo_solo_una_capa_recortada_se_ve_aunque_su_base_no_se_dibuje() {
        let mut s = LayerStack::new(64);
        s.add_layer(); // [A, B]: A vacia, B recortada a A
        set_px(&mut s.layers[1], 64, 5, 5, [0, 0, 255, 255]);
        s.layers[1].clip_to_below = true;
        s.composite_all();
        // Recortada a una base vacia: no se ve nada.
        assert_eq!(px(&s, 5, 5), s.background.pixel(5, 5));
        s.solo = true;
        s.composite_all();
        assert_eq!(px(&s, 5, 5), [0, 0, 255, 255]);
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
