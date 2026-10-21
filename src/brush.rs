//! Pinceles (ESPECIFICACION.md, seccion 7.5): los 7 presets de fabrica y el pincel elegido (preset + tamano).
//!
//! `falloff(t)` recibe `t = 1 - d/radio` (1 en el centro del toque, 0 en el borde) y devuelve cuanta opacidad queda
//! ahi (0..1). El espaciado es una fraccion del radio. El tamano es el RADIO en texeles (el slider va de 1 a 100).

use crate::pinceles::{DUAL_GRAIN_REACH, DryKind, FOLIAGE_REACH, PASTEL_SCATTER, STIPPLE_REACH};

/// Radio minimo y maximo del pincel en texeles.
pub const MIN_RADIUS: f32 = 1.0;
pub const MAX_RADIUS: f32 = 100.0;

/// Los presets de fabrica (seccion 7.5). El primero (`Soft`) es el pincel con el que arranca la app.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum BrushType {
    #[default]
    Soft,
    Hard,
    Airbrush,
    Pencil,
    PencilSoft,
    MarkerSoft,
    MarkerHard,
    /// Tiza (portada del Chalk de photocraft): punta de bitmap con grano, que gira en cada toque, y papel (`pinceles/tiza.rs`).
    Chalk,
    /// Carboncillo (portado de photocraft): punta elipse con vetas que sigue la direccion del trazo y papel que resta
    /// cobertura (`pinceles/carboncillo.rs`).
    Charcoal,
    /// Pastel (portado de photocraft): punta de grano que gira y se dispersa en cada toque, con trama de lienzo que
    /// resta cobertura (`pinceles/pastel.rs`).
    Pastel,
    /// Punteado (Stipple de photocraft): punta de gotas sueltas que gira y se voltea en cada toque y se dispersa en los
    /// dos ejes, sin papel (`pinceles/punteado.rs`).
    Stipple,
    /// Follaje (Foliage de photocraft): una o dos hojas por toque, cada una con su tamano, giro y color al azar, que se
    /// dispersan en los dos ejes, sin papel (`pinceles/follaje.rs`).
    Foliage,
    /// Doble grano (Dual Grain de photocraft): un disco con borde ruidoso multiplicado por una segunda punta de gotas
    /// sueltas, asi el trazo sale granulado (`pinceles/dual_grain.rs`).
    DualGrain,
    /// Esponja de tinta (Ink Wash Sponge de photocraft): punta de esponja con poros que gira en cada toque, flujo 0.6
    /// y bordes mojados (el interior del trazo queda mas claro que su borde), sin papel (`pinceles/esponja.rs`).
    InkSponge,
}

/// Categorias del panel de pinceles (la columna de la izquierda).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrushGroup {
    Basic,
    Pencil,
    Marker,
    Airbrush,
    Chalk,
    Dry,
}

impl BrushGroup {
    pub const ALL: [BrushGroup; 6] = [
        BrushGroup::Basic,
        BrushGroup::Pencil,
        BrushGroup::Marker,
        BrushGroup::Airbrush,
        BrushGroup::Chalk,
        BrushGroup::Dry,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BrushGroup::Basic => "Simple",
            BrushGroup::Pencil => "Lápiz",
            BrushGroup::Marker => "Rotulador",
            BrushGroup::Airbrush => "Aerógrafo",
            BrushGroup::Chalk => "Tiza",
            BrushGroup::Dry => "Seco",
        }
    }
}

impl BrushType {
    /// En el orden en que salen en el panel.
    pub const ALL: [BrushType; 14] = [
        BrushType::Soft,
        BrushType::Hard,
        BrushType::Airbrush,
        BrushType::Pencil,
        BrushType::PencilSoft,
        BrushType::MarkerSoft,
        BrushType::MarkerHard,
        BrushType::Chalk,
        BrushType::Charcoal,
        BrushType::Pastel,
        BrushType::Stipple,
        BrushType::Foliage,
        BrushType::DualGrain,
        BrushType::InkSponge,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BrushType::Soft => "Suave",
            BrushType::Hard => "Duro",
            BrushType::Airbrush => "Aerógrafo",
            BrushType::Pencil => "Lápiz",
            BrushType::PencilSoft => "Lápiz (Difuminado)",
            BrushType::MarkerSoft => "Rotulador (Suave)",
            BrushType::MarkerHard => "Rotulador (Fuerte)",
            BrushType::Chalk => "Tiza",
            BrushType::Charcoal => "Carboncillo",
            BrushType::Pastel => "Pastel",
            BrushType::Stipple => "Punteado",
            BrushType::Foliage => "Follaje",
            BrushType::DualGrain => "Doble grano",
            BrushType::InkSponge => "Esponja de tinta",
        }
    }

    pub fn group(self) -> BrushGroup {
        match self {
            BrushType::Soft | BrushType::Hard => BrushGroup::Basic,
            BrushType::Pencil | BrushType::PencilSoft => BrushGroup::Pencil,
            BrushType::MarkerSoft | BrushType::MarkerHard => BrushGroup::Marker,
            BrushType::Airbrush => BrushGroup::Airbrush,
            BrushType::Chalk => BrushGroup::Chalk,
            BrushType::Charcoal
            | BrushType::Pastel
            | BrushType::Stipple
            | BrushType::Foliage
            | BrushType::DualGrain
            | BrushType::InkSponge => BrushGroup::Dry,
        }
    }

    /// Radio inicial (texeles).
    pub fn default_radius(self) -> f32 {
        match self {
            BrushType::Soft | BrushType::Hard | BrushType::Airbrush => 18.0,
            BrushType::Pencil => 3.0,
            BrushType::PencilSoft => 48.0,
            BrushType::MarkerSoft => 6.0,
            BrushType::MarkerHard => 14.0,
            BrushType::Chalk => 20.0,
            // Los presets de photocraft miden 45 y 50 de diametro: aqui el radio es la mitad.
            BrushType::Charcoal => 22.0,
            BrushType::Pastel => 25.0,
            // Stipple de photocraft mide 40 de diametro.
            BrushType::Stipple => 20.0,
            // Foliage de photocraft mide 50 de diametro.
            BrushType::Foliage => 25.0,
            // Dual Grain de photocraft mide 50 de diametro.
            BrushType::DualGrain => 25.0,
            // Ink Wash Sponge de photocraft mide 70 de diametro.
            BrushType::InkSponge => 35.0,
        }
    }

    /// Distancia entre toques como fraccion del radio.
    pub fn spacing(self) -> f32 {
        match self {
            BrushType::Soft | BrushType::Hard | BrushType::Airbrush => 0.25,
            BrushType::Pencil => 0.15,
            BrushType::PencilSoft => 0.20,
            BrushType::MarkerSoft => 0.15,
            BrushType::MarkerHard => 0.12,
            BrushType::Chalk => 0.20,
            // photocraft mide el espaciado en fraccion del diametro (0.12 y 0.15): aqui es del radio, el doble.
            BrushType::Charcoal => 0.24,
            BrushType::Pastel => 0.30,
            // Stipple: 0.5 del diametro en photocraft = 1.0 del radio (un toque cada radio, para que las gotas no se
            // amontonen).
            BrushType::Stipple => 1.0,
            // Foliage: 0.35 del diametro en photocraft = 0.7 del radio.
            BrushType::Foliage => 0.7,
            // Dual Grain: 0.25 del diametro en photocraft = 0.5 del radio.
            BrushType::DualGrain => 0.5,
            // Ink Wash Sponge: 0.3 del diametro en photocraft = 0.6 del radio.
            BrushType::InkSponge => 0.6,
        }
    }

    /// El pincel de medio seco que es (carboncillo, pastel o punteado), o `None` si no lo es. Estos pinceles no usan
    /// el `falloff`: su forma sale de la punta de bitmap y su papel o lienzo (si lo tienen) resta cobertura al trazo
    /// (`pinceles/medio_seco.rs`).
    pub fn dry_kind(self) -> Option<DryKind> {
        match self {
            BrushType::Charcoal => Some(DryKind::Charcoal),
            BrushType::Pastel => Some(DryKind::Pastel),
            BrushType::Stipple => Some(DryKind::Stipple),
            BrushType::Foliage => Some(DryKind::Foliage),
            BrushType::InkSponge => Some(DryKind::Sponge),
            _ => None,
        }
    }

    /// Cuanto se puede alejar un toque de su centro, como fraccion del radio (la dispersion del pastel y del
    /// punteado). Sirve para buscar los triangulos cercanos con un poco de margen.
    pub fn scatter(self) -> f32 {
        match self {
            BrushType::Pastel => PASTEL_SCATTER,
            BrushType::Stipple => STIPPLE_REACH,
            BrushType::Foliage => FOLIAGE_REACH,
            // Lo que se pasa del radio la segunda punta (sus gotas caen hasta 1.45 radios del centro).
            BrushType::DualGrain => DUAL_GRAIN_REACH - 1.0,
            _ => 0.0,
        }
    }

    /// Opacidad segun la distancia al centro: `t = 1 - d/radio`.
    pub fn falloff(self, t: f32) -> f32 {
        match self {
            BrushType::Soft => (2.0 * t).min(1.0),
            BrushType::Hard => (8.0 * t).min(1.0),
            BrushType::Airbrush => t * t,
            BrushType::Pencil => (10.0 * t).min(1.0),
            BrushType::PencilSoft => t * t * (3.0 - 2.0 * t),
            BrushType::MarkerSoft => (4.0 * t).min(1.0),
            BrushType::MarkerHard => (20.0 * t).min(1.0),
            // La tiza no usa el falloff radial: su forma sale de la punta de bitmap (`pinceles/tiza.rs`). Este valor solo
            // existe para que la funcion este definida para todos los pinceles.
            BrushType::Chalk => (6.0 * t).min(1.0),
            // Carboncillo, pastel y punteado tampoco usan el falloff radial (su forma sale de la punta de bitmap): igual
            // que la tiza, el valor solo existe para que la funcion este definida para todos los pinceles.
            BrushType::Charcoal | BrushType::Pastel | BrushType::Stipple | BrushType::Foliage | BrushType::InkSponge => {
                (6.0 * t).min(1.0)
            }
            // Doble grano: tampoco usa el falloff radial (su disco y sus gotas salen de `pinceles/dual_grain.rs`).
            BrushType::DualGrain => (6.0 * t).min(1.0),
        }
    }
}

/// Flujo minimo del pincel (el deslizador no baja de aqui: con 0 no pintaria nada).
pub const MIN_FLOW: f32 = 0.01;

/// Cada cuanto deposita un toque el aerografo con el boton apretado y el puntero quieto (20 por segundo, como en
/// photocraft).
pub const BUILD_UP_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);

impl BrushType {
    /// true si el pincel sigue depositando pintura con el puntero quieto (un toque cada `BUILD_UP_INTERVAL`).
    pub fn build_up(self) -> bool {
        matches!(self, BrushType::Airbrush)
    }

    /// Flujo con el que arranca el pincel al elegirlo (el deslizador "Flujo" lo puede cambiar). Casi todos arrancan en
    /// 1 (sin efecto); la esponja de tinta trae el 0.6 de photocraft.
    pub fn default_flow(self) -> f32 {
        match self {
            BrushType::InkSponge => 0.6,
            _ => 1.0,
        }
    }
}

/// El pincel elegido: preset, radio actual (texeles) y flujo.
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    pub kind: BrushType,
    pub radius: f32,
    /// Flujo (`MIN_FLOW..=1`): multiplica la cobertura de cada toque (como `flow` en photocraft). Con 1 el pincel se
    /// comporta como siempre; con un flujo bajo cada toque suma poco y hacen falta varios para llegar a la opacidad.
    pub flow: f32,
}

impl Brush {
    pub fn new(kind: BrushType) -> Brush {
        Brush { kind, radius: kind.default_radius(), flow: kind.default_flow() }
    }
}

impl Default for Brush {
    fn default() -> Brush {
        Brush::new(BrushType::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_falloff_va_de_0_en_el_borde_a_1_en_el_centro() {
        for b in BrushType::ALL {
            assert_eq!(b.falloff(0.0), 0.0, "{}", b.name());
            // Airbrush (t^2) y los difuminados llegan a 1 justo en el centro.
            assert!((b.falloff(1.0) - 1.0).abs() < 1e-6, "{}", b.name());
        }
    }

    #[test]
    fn el_pincel_suave_es_min_1_2t() {
        assert!((BrushType::Soft.falloff(0.25) - 0.5).abs() < 1e-6);
        assert_eq!(BrushType::Soft.falloff(0.8), 1.0);
    }

    #[test]
    fn los_valores_iniciales_son_los_de_la_especificacion() {
        assert_eq!(BrushType::Soft.default_radius(), 18.0);
        assert_eq!(BrushType::Pencil.default_radius(), 3.0);
        assert_eq!(BrushType::PencilSoft.default_radius(), 48.0);
        assert_eq!(BrushType::MarkerHard.spacing(), 0.12);
        assert_eq!(Brush::default().radius, 18.0);
    }

    #[test]
    fn todos_los_pinceles_estan_en_un_grupo_y_con_radio_valido() {
        for b in BrushType::ALL {
            assert!(BrushGroup::ALL.contains(&b.group()));
            assert!((MIN_RADIUS..=MAX_RADIUS).contains(&b.default_radius()));
            assert!(b.spacing() > 0.0);
        }
    }

    #[test]
    fn el_flujo_inicial_es_1_y_solo_el_aerografo_deposita_con_el_puntero_quieto() {
        assert_eq!(Brush::default().flow, 1.0);
        for b in BrushType::ALL {
            assert_eq!(b.build_up(), b == BrushType::Airbrush, "{}", b.name());
        }
    }

    #[test]
    fn el_punteado_es_de_medio_seco_con_los_valores_de_photocraft() {
        let b = BrushType::Stipple;
        assert_eq!(b.name(), "Punteado");
        assert_eq!(b.group(), BrushGroup::Dry);
        assert_eq!(b.dry_kind(), Some(DryKind::Stipple));
        // 40 de diametro en photocraft = radio 20; espaciado 0.5 del diametro = 1.0 del radio.
        assert_eq!(b.default_radius(), 20.0);
        assert_eq!(b.spacing(), 1.0);
        // Su dispersion cubre el 60 % del radio en cada eje: la esquina esta a 0.6 * raiz de 2.
        assert!((b.scatter() - 0.6 * 2f32.sqrt()).abs() < 1e-6);
        assert!(b.scatter() > BrushType::Pastel.scatter());
        // Solo los cinco de medio seco (carboncillo, pastel, punteado, follaje y esponja) tienen punta propia.
        assert_eq!(BrushType::ALL.iter().filter(|k| k.dry_kind().is_some()).count(), 5);
    }

    #[test]
    fn el_follaje_es_de_medio_seco_con_los_valores_de_photocraft() {
        let b = BrushType::Foliage;
        assert_eq!(b.name(), "Follaje");
        assert_eq!(b.group(), BrushGroup::Dry);
        assert_eq!(b.dry_kind(), Some(DryKind::Foliage));
        // 50 de diametro en photocraft = radio 25; espaciado 0.35 del diametro = 0.7 del radio.
        assert_eq!(b.default_radius(), 25.0);
        assert!((b.spacing() - 0.7).abs() < 1e-6);
        // Dispersion 1.2 del radio en cada eje: la esquina esta a 1.2 * raiz de 2.
        assert!((b.scatter() - 1.2 * 2f32.sqrt()).abs() < 1e-6);
        assert!(b.scatter() > BrushType::Stipple.scatter());
    }

    #[test]
    fn el_doble_grano_es_de_medio_seco_con_los_valores_de_photocraft() {
        let b = BrushType::DualGrain;
        assert_eq!(b.name(), "Doble grano");
        assert_eq!(b.group(), BrushGroup::Dry);
        // No es un `DryKind`: lleva dos puntas y su propio toque (`DualDab`).
        assert_eq!(b.dry_kind(), None);
        // 50 de diametro en photocraft = radio 25; espaciado 0.25 del diametro = 0.5 del radio.
        assert_eq!(b.default_radius(), 25.0);
        assert!((b.spacing() - 0.5).abs() < 1e-6);
        // Las gotas llegan a 0.6 * (1 + raiz de 2) radios del centro: el margen es lo que pasa de 1.
        assert!((b.scatter() - (0.6 * (1.0 + 2f32.sqrt()) - 1.0)).abs() < 1e-5);
    }

    #[test]
    fn la_esponja_de_tinta_es_de_medio_seco_con_los_valores_de_photocraft() {
        let b = BrushType::InkSponge;
        assert_eq!(b.name(), "Esponja de tinta");
        assert_eq!(b.group(), BrushGroup::Dry);
        assert_eq!(b.dry_kind(), Some(DryKind::Sponge));
        // 70 de diametro en photocraft = radio 35; espaciado 0.3 del diametro = 0.6 del radio.
        assert_eq!(b.default_radius(), 35.0);
        assert!((b.spacing() - 0.6).abs() < 1e-6);
        // Flujo 0.6; no dispersa sus toques.
        assert!((b.default_flow() - 0.6).abs() < 1e-6);
        assert_eq!(Brush::new(b).flow, b.default_flow());
        assert_eq!(b.scatter(), 0.0);
        // El resto de pinceles arranca con flujo 1.
        for k in BrushType::ALL {
            if k != BrushType::InkSponge {
                assert_eq!(k.default_flow(), 1.0, "{}", k.name());
            }
        }
    }

    #[test]
    fn la_tiza_esta_en_su_categoria_con_sus_valores() {
        assert_eq!(BrushType::Chalk.group(), BrushGroup::Chalk);
        assert_eq!(BrushType::Chalk.name(), "Tiza");
        assert_eq!(BrushType::Chalk.default_radius(), 20.0);
        assert_eq!(BrushType::Chalk.spacing(), 0.20);
    }
}
