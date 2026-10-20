//! Pinceles (ESPECIFICACION.md, seccion 7.5): los 7 presets de fabrica y el pincel elegido (preset + tamano).
//!
//! `falloff(t)` recibe `t = 1 - d/radio` (1 en el centro del toque, 0 en el borde) y devuelve cuanta opacidad queda
//! ahi (0..1). El espaciado es una fraccion del radio. El tamano es el RADIO en texeles (el slider va de 1 a 100).

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
}

/// Categorias del panel de pinceles (la columna de la izquierda).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BrushGroup {
    Basic,
    Pencil,
    Marker,
    Airbrush,
}

impl BrushGroup {
    pub const ALL: [BrushGroup; 4] = [BrushGroup::Basic, BrushGroup::Pencil, BrushGroup::Marker, BrushGroup::Airbrush];

    pub fn name(self) -> &'static str {
        match self {
            BrushGroup::Basic => "Simple",
            BrushGroup::Pencil => "Lápiz",
            BrushGroup::Marker => "Rotulador",
            BrushGroup::Airbrush => "Aerógrafo",
        }
    }
}

impl BrushType {
    /// En el orden en que salen en el panel.
    pub const ALL: [BrushType; 7] = [
        BrushType::Soft,
        BrushType::Hard,
        BrushType::Airbrush,
        BrushType::Pencil,
        BrushType::PencilSoft,
        BrushType::MarkerSoft,
        BrushType::MarkerHard,
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
        }
    }

    pub fn group(self) -> BrushGroup {
        match self {
            BrushType::Soft | BrushType::Hard => BrushGroup::Basic,
            BrushType::Pencil | BrushType::PencilSoft => BrushGroup::Pencil,
            BrushType::MarkerSoft | BrushType::MarkerHard => BrushGroup::Marker,
            BrushType::Airbrush => BrushGroup::Airbrush,
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
        }
    }
}

/// El pincel elegido: preset y radio actual (texeles).
#[derive(Clone, Copy, Debug)]
pub struct Brush {
    pub kind: BrushType,
    pub radius: f32,
}

impl Brush {
    pub fn new(kind: BrushType) -> Brush {
        Brush { kind, radius: kind.default_radius() }
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
}
