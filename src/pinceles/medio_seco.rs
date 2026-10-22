//! Lo que tienen en comun los pinceles de medio seco portados de photocraft: el CARBONCILLO (`carboncillo.rs`), el
//! PASTEL (`pastel.rs`) y el PUNTEADO (`punteado.rs`). Cada uno arma su punta y su toque en su propio archivo; aqui
//! solo estan el tipo `DryKind`, el toque ya preparado (`DryDab`) y la resta del papel.
//!
//! Su papel o lienzo no multiplica cada toque: RESTA cobertura al trazo ya acumulado (`dry_subtract`, modo Subtract
//! de photocraft), asi que dos toques sobre el mismo sitio no cambian el dibujo del papel.

use glam::Vec3;

use super::{
    carboncillo, cincel, crayon, esponja, follaje, pastel, pasto, punteado, salpicadura, semitono,
};

/// Los pinceles de medio seco portados de photocraft.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DryKind {
    Charcoal,
    Pastel,
    /// Punteado: gotas sueltas, sin papel ni lienzo.
    Stipple,
    /// Follaje: hojas de tamano, giro y color al azar (1 o 2 por toque), sin papel ni lienzo.
    Foliage,
    /// Esponja de tinta: punta de esponja con poros que gira en cada toque, sin papel ni lienzo pero con bordes mojados
    /// (`esponja.rs`; los bordes mojados se aplican en `paint.rs`).
    Sponge,
    /// Salpicadura: racimos de gotas que giran, se voltean y se dispersan en los dos ejes (1 o 2 por toque), sin papel
    /// ni lienzo (`salpicadura.rs`).
    Spatter,
    /// Pasto: un manojo de hojas finas que sube hacia arriba del modelo (1 por toque), con tamano, giro leve, volteo y
    /// dispersion de lado al azar, sin papel ni lienzo (`pasto.rs`).
    Grass,
    /// Puntos de semitono: un disco suave cuya cobertura acumulada se mezcla con una trama de puntos en modo HardMix de
    /// photocraft (pinta del todo o nada). No resta papel: la mezcla se aplica en `paint.rs` con `dry_hardmix`
    /// (`semitono.rs`).
    Halftone,
    /// Crayon de cera (Wax Crayon de photocraft): punta redonda aplastada que sigue la direccion del trazo, con un papel
    /// que se mezcla en modo HardMix con CADA toque (`crayon.rs`; la mezcla se aplica en `paint.rs` con
    /// `dry_mezcla_toque`).
    Crayon,
    /// Textura de esponja (Sponge Texture de photocraft): la esponja de poros con otra semilla, sin bordes mojados, y una
    /// opacidad distinta en cada toque (`esponja.rs`; la opacidad se aplica en `paint.rs` con `dry_mezcla_toque`).
    SpongeTexture,
    /// Cincel plano: punta de paralelogramo aplastada que sigue la direccion del trazo, sin papel ni azar
    /// (`cincel.rs`; deducido de una imagen, no de photocraft).
    Chisel,
}

/// Cuanta cobertura RESTA el papel (carboncillo) o el lienzo (pastel) en un texel: `profundidad * t`, con `t` el patron
/// invertido (1 - valor). Va de 0 a la profundidad. Es el modo Subtract de photocraft: se resta a la cobertura YA
/// acumulada del trazo, no a cada toque, asi que pasar dos veces por el mismo sitio no cambia el dibujo del papel.
pub fn dry_subtract(kind: DryKind, pos: Vec3, tx: u32, ty: u32) -> f32 {
    match kind {
        DryKind::Charcoal => carboncillo::subtract(pos, tx, ty),
        DryKind::Pastel => pastel::subtract(tx, ty),
        // El punteado no lleva textura: su preset de photocraft no activa ninguna. La esponja tampoco (sus bordes mojados
        // no restan: multiplican, en `paint.rs`).
        // Los puntos de semitono tampoco restan: su trama se mezcla en modo HardMix (`dry_hardmix`). El crayon tampoco:
        // su papel se mezcla con cada toque (`dry_mezcla_toque`).
        DryKind::Stipple
        | DryKind::Foliage
        | DryKind::Sponge
        | DryKind::Spatter
        | DryKind::Grass
        | DryKind::Halftone
        | DryKind::Crayon
        | DryKind::SpongeTexture
        | DryKind::Chisel => 0.0,
    }
}

/// El valor de la trama (0..1) con que se mezcla en modo HardMix la cobertura acumulada del trazo en el texel `(tx, ty)`,
/// o `None` si el pincel no lleva esa mezcla (todos menos los puntos de semitono). Es el modo HardMix de photocraft con
/// profundidad 1: donde `cobertura + trama > 1` se pinta del todo y donde no, nada (`paint.rs`, `apply_texel`). Como
/// el papel, se aplica a la cobertura ya acumulada y no a cada toque.
pub fn dry_hardmix(kind: DryKind, tx: u32, ty: u32) -> Option<f32> {
    match kind {
        DryKind::Halftone => Some(semitono::patron(tx, ty)),
        _ => None,
    }
}

/// La cobertura `v` de UN toque (el numero `dab`) en el texel `(tx, ty)` (en la posicion `pos` del modelo) ya mezclada
/// con el papel de ese toque, o `v` tal cual si el pincel no mezcla papel con cada toque (todos menos el crayon y la textura de esponja, que solo la escala por la opacidad del toque). A
/// diferencia de `dry_subtract` y `dry_hardmix`, que actuan sobre la cobertura ya acumulada del trazo, esta se aplica
/// a cada toque ANTES de acumularlo (`each_tip` de photocraft; `paint.rs`, `dab_surface`).
pub fn dry_mezcla_toque(kind: DryKind, v: f32, dab: u32, pos: Vec3, tx: u32, ty: u32) -> f32 {
    match kind {
        DryKind::Crayon => crayon::mezcla(v, dab, pos, tx, ty),
        DryKind::SpongeTexture => v * esponja::opacidad_toque(dab),
        _ => v,
    }
}

/// Un toque de medio seco ya preparado. Cada pincel lo arma a su manera:
///
/// - Carboncillo: la punta (elipse con vetas) apunta en la direccion del trazo, sobre el plano de la superficie.
/// - Pastel: la punta gira, se aplasta un poco y el centro se dispersa de lado respecto al trazo.
/// - Punteado: la punta de gotas gira, se voltea y el centro se dispersa en los dos ejes del plano.
/// - Follaje: una o dos hojas, cada una con su tamano, giro, redondez, dispersion y color (`follaje.rs`).
/// - Salpicadura: uno o dos racimos de gotas, cada uno con su tamano, giro, volteo y dispersion (`salpicadura.rs`).
/// - Pasto: un manojo de hojas que sube hacia arriba del modelo, con tamano, giro leve, volteo y dispersion de lado
///   (`pasto.rs`).
/// - Puntos de semitono: un disco redondo, sin giro, tamano ni dispersion; su punta llega mas alla del radio
///   (`semitono.rs`).
/// - Crayon de cera: una elipse de borde suave que sigue la direccion del trazo, sin tamano ni dispersion al azar; su
///   punta llega mas alla del radio (`crayon.rs`).
/// - Cincel plano: un paralelogramo aplastado que sigue la direccion del trazo, sin azar (`cincel.rs`).
/// - Esponja de tinta y Textura de esponja: la punta de poros gira libre; la de tinta cambia de tamano y la textura
///   cambia de opacidad en cada toque (`esponja.rs`).
pub struct DryDab {
    pub(super) kind: DryKind,
    /// Ejes de la punta sobre la superficie.
    pub(super) t: Vec3,
    pub(super) b: Vec3,
    pub(super) inv_radius: f32,
    /// Factor de redondez de la punta (1 = redonda); aplasta el eje `b`.
    pub(super) roundness: f32,
    /// Desplazamiento del centro del toque (dispersion); cero para el carboncillo.
    pub(super) shift: Vec3,
    /// Solo el follaje: el color propio de la hoja y la segunda hoja del toque. La salpicadura lo usa solo para su
    /// segundo racimo (sin cambio de color). `None` en los demas pinceles.
    pub(super) follaje: Option<Box<follaje::Extras>>,
}

impl DryDab {
    /// `dab` es el numero del toque, `normal` la normal de la superficie (cero = se usa Z), `dir` la direccion del
    /// trazo en 3D (cero = no se sabe) y `radius` el radio del pincel en las mismas unidades que los puntos.
    pub fn new(kind: DryKind, dab: u32, normal: Vec3, dir: Vec3, radius: f32) -> DryDab {
        let n = normal.try_normalize().unwrap_or(Vec3::Z);
        let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let t0 = n.cross(helper).normalize();
        let b0 = n.cross(t0);
        // La direccion del trazo proyectada en el plano de la superficie.
        let tangent = dir - n * dir.dot(n);
        let along = (tangent.length_squared() > 1e-12).then(|| tangent.normalize());
        let inv_radius = 1.0 / radius.max(1e-6);
        match kind {
            DryKind::Charcoal => carboncillo::nuevo(inv_radius, t0, n, along),
            DryKind::Pastel => pastel::nuevo(dab, inv_radius, radius, t0, b0, n, along),
            DryKind::Stipple => punteado::nuevo(dab, inv_radius, radius, t0, b0),
            DryKind::Foliage => follaje::nuevo(dab, inv_radius, radius, t0, b0),
            DryKind::Sponge => esponja::nuevo(dab, inv_radius, t0, b0),
            DryKind::SpongeTexture => esponja::nuevo_textura(dab, inv_radius, t0, b0),
            DryKind::Chisel => cincel::nuevo(inv_radius, t0, n, along),
            DryKind::Spatter => salpicadura::nuevo(dab, inv_radius, radius, t0, b0),
            DryKind::Grass => pasto::nuevo(dab, inv_radius, radius, t0, b0, n, along),
            DryKind::Halftone => semitono::nuevo(inv_radius, t0, b0),
            DryKind::Crayon => crayon::nuevo(inv_radius, t0, n, along),
        }
    }

    /// La segunda hoja del toque (solo el follaje la tiene).
    fn otra(&self) -> Option<&DryDab> {
        self.follaje.as_ref().and_then(|f| f.otra.as_ref())
    }

    /// Cuanto se aleja el centro de la punta del punto tocado (para ampliar la zona que se revisa). Con dos hojas (o dos
    /// racimos), la que mas se aleja. El pasto suma ademas lo que sus hojas se salen del circulo de su radio (llegan
    /// hasta la esquina del cuadrado de la punta).
    pub fn reach(&self) -> f32 {
        let exceso = match self.kind {
            DryKind::Grass => pasto::GRASS_OVERSHOOT / self.inv_radius,
            // Los puntos de semitono: su punta llega mas alla del radio (el hombro suave del disco).
            DryKind::Halftone => semitono::exceso(self.inv_radius),
            // El crayon: su borde suave llega mas alla del radio.
            DryKind::Crayon => crayon::exceso(self.inv_radius),
            _ => 0.0,
        };
        let own = self.shift.length() + exceso;
        self.otra().map_or(own, |o| own.max(o.reach()))
    }

    /// Cobertura (0..1) de UNA punta (la hoja principal, sin la segunda) en un texel.
    fn punta_alpha(&self, offset: Vec3) -> f32 {
        let o = offset - self.shift;
        let (u, v) = (
            o.dot(self.t) * self.inv_radius,
            o.dot(self.b) * self.inv_radius / self.roundness,
        );
        let tip = match self.kind {
            DryKind::Charcoal => carboncillo::punta(),
            DryKind::Pastel => pastel::punta(),
            DryKind::Stipple => punteado::punta(),
            DryKind::Foliage => follaje::punta(),
            DryKind::Sponge => esponja::punta(),
            DryKind::SpongeTexture => esponja::punta_textura(),
            DryKind::Chisel => cincel::punta(),
            DryKind::Spatter => salpicadura::punta(),
            DryKind::Grass => pasto::punta(),
            DryKind::Halftone => semitono::punta(),
            DryKind::Crayon => crayon::punta(),
        };
        tip.sample(u * 0.5 + 0.5, v * 0.5 + 0.5)
    }

    /// Cobertura (0..1) de la punta en un texel; `offset` es su posicion respecto al punto tocado. Es 0 fuera de la
    /// punta. Con dos hojas (follaje) es la de la que mas cubre el texel. El papel o el lienzo no entran aqui: restan
    /// despues (`dry_subtract`).
    pub fn alpha(&self, offset: Vec3) -> f32 {
        let own = self.punta_alpha(offset);
        self.otra().map_or(own, |o| own.max(o.alpha(offset)))
    }

    /// Calcula el color propio de cada hoja a partir del color del pincel (`base`). Solo el follaje cambia algo: cada
    /// hoja corre un poco el tono y el brillo. Se llama una vez por toque, no por texel.
    pub fn with_color(mut self, base: [u8; 3]) -> DryDab {
        if let Some(f) = self.follaje.as_mut() {
            f.paint = Some(follaje::matizar(base, f.hue, f.bright));
            if let Some(otra) = f.otra.take() {
                f.otra = Some(otra.with_color(base));
            }
        }
        self
    }

    /// Como `alpha`, pero devuelve tambien el color con que se pinta ese texel: el de la hoja que mas lo cubre (el del
    /// pincel, `base`, si el toque no tiene color propio, como todos menos el follaje).
    pub fn alpha_color(&self, offset: Vec3, base: [u8; 3]) -> (f32, [u8; 3]) {
        let a = self.punta_alpha(offset);
        let color = self.follaje.as_ref().and_then(|f| f.paint).unwrap_or(base);
        match self.otra() {
            Some(o) => {
                let (b, cb) = o.alpha_color(offset, base);
                if b > a { (b, cb) } else { (a, color) }
            }
            None => (a, color),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pinceles::carboncillo::CHARCOAL_PAPER_DEPTH;
    use crate::pinceles::pastel::PASTEL_CANVAS_DEPTH;

    #[test]
    fn la_resta_del_papel_y_del_lienzo_esta_en_rango_y_varia() {
        for (kind, depth) in [
            (DryKind::Charcoal, CHARCOAL_PAPER_DEPTH),
            (DryKind::Pastel, PASTEL_CANVAS_DEPTH),
        ] {
            let vals: Vec<f32> = (0..300u32)
                .map(|i| {
                    dry_subtract(
                        kind,
                        Vec3::new(i as f32 * 0.011, 0.2, i as f32 * 0.005),
                        i,
                        i * 3 % 1024,
                    )
                })
                .collect();
            assert!(
                vals.iter().all(|v| (0.0..=depth + 1e-6).contains(v)),
                "{kind:?}"
            );
            let (lo, hi) = (
                vals.iter().cloned().fold(f32::MAX, f32::min),
                vals.iter().cloned().fold(0.0, f32::max),
            );
            assert!(
                hi - lo > 0.2 * depth,
                "{kind:?} es casi plano: de {lo} a {hi}"
            );
            // Mismo punto, mismo valor.
            assert_eq!(
                dry_subtract(kind, Vec3::ONE, 5, 9),
                dry_subtract(kind, Vec3::ONE, 5, 9)
            );
        }
    }
}
