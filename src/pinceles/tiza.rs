//! Pincel de TIZA (portado del Chalk de photocraft). Tiene tres piezas, todas deterministas (sin archivos ni azar):
//!
//! - La PUNTA: un bitmap de 64x64 en gris. Es un disco de borde irregular (80 a 95 % del radio) con grano: el 20 % de
//!   sus puntos casi no pinta y el resto va del 70 al 100 %.
//! - El ANGULO y el TAMANO de cada toque: gira un angulo al azar (de -180 a 180 grados) y mide del 85 al 100 % del
//!   radio. Como cada toque gira distinto, el grano no se repite en fila y el trazo se ve organico.
//! - El PAPEL (`papel.rs`): multiplica la cobertura: `1 - PAPER_DEPTH + PAPER_DEPTH * papel`, o sea de 0.4 (papel
//!   oscuro) a 1.
//!
//! El angulo al azar permite orientar la punta con cualquier base de la superficie: no hace falta una direccion
//! coherente entre triangulos.
//!
//! La punta de grano (`construir_grano`) tambien la usa el pastel, con otra semilla.

use std::f32::consts::PI;
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{dab_angle, hash2, unit, value_noise2};
use super::papel::paper_mask;
use super::punta::{Punta, TIP_SIZE};

/// Semilla de la punta de la tiza (la de photocraft: 7).
const CHALK_TIP_SEED: u32 = 7;
const DAB_SIZE_SALT: u64 = 0x5349_5A45;
/// Variacion del tamano de cada toque (0.15 = del 85 al 100 %).
const SIZE_JITTER: f32 = 0.15;
/// Para las vistas previas del panel (que miden en pixeles): unidades del modelo por pixel de la imagen.
pub const PREVIEW_UNITS_PER_PX: f32 = 0.011;

/// La punta de grano (`chalk_tip(64, seed)` de photocraft): la semilla cambia el borde y el grano. La usan la tiza y
/// el pastel.
pub(super) fn construir_grano(seed: u32) -> Punta {
    let salt = u64::from(seed) + 101;
    let n = TIP_SIZE;
    let c = n as f32 / 2.0;
    let mut data = Vec::with_capacity(n * n);
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = (dx * dx + dy * dy).sqrt() / c;
            let ang = dy.atan2(dx);
            // Borde irregular: entre el 80 y el 95 % del radio segun el angulo.
            let edge = 0.8 + 0.15 * value_noise2((ang + PI) * 3.0, 0.5, 19, salt);
            let inside = ((edge - r) * 8.0).clamp(0.0, 1.0);
            let grain = hash2(x as i32, y as i32, salt);
            data.push(inside * if grain < 0.2 { 0.2 } else { 0.7 + 0.3 * grain });
        }
    }
    Punta::new(data)
}

/// La punta de la tiza (se genera una sola vez).
fn chalk_tip() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| construir_grano(CHALK_TIP_SEED))
}

/// Angulo (radianes, de -pi a pi) y factor de tamano (0.85 a 1) del toque numero `dab`.
fn dab_params(dab: u32) -> (f32, f32) {
    let size = 1.0 - SIZE_JITTER * unit(DAB_SIZE_SALT.wrapping_add(u64::from(dab)));
    (dab_angle(dab), size)
}

/// Un toque de tiza ya preparado: la base (en 3D) sobre la que se mide cada texel y el tamano de la punta.
pub struct ChalkDab {
    /// Ejes de la punta sobre la superficie, ya girados el angulo del toque.
    t: Vec3,
    b: Vec3,
    inv_radius: f32,
}

impl ChalkDab {
    /// `dab` es el numero del toque (de el salen el angulo y el tamano), `normal` la normal de la superficie en el
    /// punto tocado (si es cero se usa Z) y `radius` el radio del pincel en las mismas unidades que los puntos.
    pub fn new(dab: u32, normal: Vec3, radius: f32) -> ChalkDab {
        let (angle, size) = dab_params(dab);
        let n = normal.try_normalize().unwrap_or(Vec3::Z);
        let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let t = n.cross(helper).normalize();
        let b = n.cross(t);
        let (sn, cs) = angle.sin_cos();
        ChalkDab { t: t * cs + b * sn, b: b * cs - t * sn, inv_radius: 1.0 / (radius * size).max(1e-6) }
    }

    /// Cobertura (0..1) de un texel: `offset` es su posicion respecto al centro del toque, `paper_pos` el punto donde
    /// se mide el papel y `(tx, ty)` el texel. Es 0 fuera de la punta.
    pub fn alpha(&self, offset: Vec3, paper_pos: Vec3, tx: u32, ty: u32) -> f32 {
        let tip = self.tip(offset);
        if tip <= 0.0 {
            return 0.0;
        }
        tip * paper_mask(paper_pos, tx, ty)
    }

    /// Cobertura de la punta sola (sin papel) en un texel; `offset` es su posicion respecto al centro del toque. El
    /// trazo de la app usa esta y aplica el papel despues, sobre la cobertura acumulada (`apply_texel`), como la
    /// textura de photocraft (con `each_tip` apagado): si el papel multiplicara cada toque, al acumular los toques
    /// rellenarian los huecos y el papel desapareceria.
    pub fn tip(&self, offset: Vec3) -> f32 {
        let (u, v) = (offset.dot(self.t) * self.inv_radius, offset.dot(self.b) * self.inv_radius);
        chalk_tip().sample(u * 0.5 + 0.5, v * 0.5 + 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_punta_es_determinista_y_tiene_pintura() {
        let a = construir_grano(CHALK_TIP_SEED);
        let b = construir_grano(CHALK_TIP_SEED);
        assert_eq!(a.data, b.data);
        assert!(a.data.iter().all(|v| (0.0..=1.0).contains(v)));
        let suma: f32 = a.data.iter().sum();
        assert!(suma > TIP_SIZE as f32, "suma {suma}");
        // La esquina queda vacia y el centro tiene pintura.
        assert_eq!(a.data[0], 0.0);
        assert!(a.sample(0.5, 0.5) > 0.0);
    }

    #[test]
    fn fuera_de_la_punta_no_hay_pintura() {
        let t = chalk_tip();
        assert_eq!(t.sample(-0.1, 0.5), 0.0);
        assert_eq!(t.sample(0.5, 1.2), 0.0);
        assert_eq!(t.sample(f32::NAN, 0.5), 0.0);
    }

    #[test]
    fn el_grano_deja_huecos_en_la_punta() {
        // El 20 % de los puntos del interior casi no pintan: hay valores bajos y altos dentro del disco.
        let t = chalk_tip();
        let dentro: Vec<f32> = t.data.iter().copied().filter(|v| *v > 0.0).collect();
        assert!(dentro.iter().any(|v| *v < 0.3));
        assert!(dentro.iter().any(|v| *v > 0.9));
    }

    #[test]
    fn cada_toque_tiene_su_angulo_y_su_tamano() {
        let mut angulos = std::collections::HashSet::new();
        for d in 1..200 {
            let (a, s) = dab_params(d);
            assert!((-PI..=PI).contains(&a), "{a}");
            assert!((1.0 - SIZE_JITTER..=1.0).contains(&s), "{s}");
            angulos.insert((a * 1000.0) as i32);
            // Determinista.
            assert_eq!(dab_params(d), (a, s));
        }
        assert!(angulos.len() > 150, "los angulos se repiten: {}", angulos.len());
    }

    #[test]
    fn la_base_de_la_punta_es_ortonormal_sea_cual_sea_la_normal() {
        for n in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO] {
            let d = ChalkDab::new(7, n, 1.0);
            assert!((d.t.length() - 1.0).abs() < 1e-4 && (d.b.length() - 1.0).abs() < 1e-4);
            assert!(d.t.dot(d.b).abs() < 1e-4);
        }
    }

    #[test]
    fn el_toque_pinta_cerca_del_centro_y_no_mas_alla_del_radio() {
        let radio = 2.0;
        let d = ChalkDab::new(3, Vec3::Z, radio);
        // Algun texel cerca del centro pinta.
        let cerca = (0..40).any(|i| {
            let o = Vec3::new((i % 8) as f32 * 0.1 - 0.4, (i / 8) as f32 * 0.1 - 0.2, 0.0);
            d.alpha(o, o, i, 2) > 0.0
        });
        assert!(cerca);
        // A mas de un radio (con el tamano al maximo) no hay nada, en ninguna direccion.
        for k in 0..16 {
            let a = k as f32 * PI / 8.0;
            let o = Vec3::new(a.cos(), a.sin(), 0.0) * radio * 1.01;
            assert_eq!(d.alpha(o, o, 1, 1), 0.0);
        }
    }

    #[test]
    fn la_cobertura_nunca_pasa_de_1() {
        let d = ChalkDab::new(11, Vec3::Y, 1.0);
        for i in 0..400 {
            let o = Vec3::new((i % 20) as f32 * 0.1 - 1.0, 0.0, (i / 20) as f32 * 0.1 - 1.0);
            let a = d.alpha(o, o, i, i);
            assert!((0.0..=1.0).contains(&a), "{a}");
        }
    }
}
