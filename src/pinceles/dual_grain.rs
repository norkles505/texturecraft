//! Pincel DOBLE GRANO (Dual Grain de photocraft). Lleva dos puntas:
//!
//! - La principal: un disco de dureza 0.6 (nucleo solido hasta el 70 % del radio y un borde suave que se apaga hasta
//!   1.32 radios) con RUIDO en el borde (`noise`): cada texel decide al azar, pero siempre igual, si queda del todo
//!   pintado o casi vacio, y el valor se acerca un 70 % a esa decision.
//! - La segunda (Dual Brush): gotas sueltas (`spatter_tip(48, 23, 20)`) de 30 de tamano contra 50 de la principal, o
//!   sea el 60 % de su radio, que caen a lo largo del trazo con dispersion 1.0 en los dos ejes, 2 por paso, con volteo
//!   horizontal al azar.
//!
//! El trazo acumula las dos por separado: la principal sube hacia la opacidad (`paint.rs`) y la segunda va sumando
//! `c + v * (1 - c)` toque a toque. Lo que se ve es el producto de las dos (modo Multiply de photocraft): la principal
//! solo pinta donde la segunda ya dejo gotas, asi que el trazo sale granulado.
//!
//! Diferencia con photocraft: la segunda punta lleva su propio espaciado (0.3 de su diametro) y aqui no hay un
//! recorrido aparte, asi que cada toque principal deposita las gotas que le tocan en promedio
//! (`DUAL_PER_DAB`, unas 2.78) alrededor de su centro. La dispersion es mucho mayor que esa diferencia de posicion.

use std::f32::consts::SQRT_2;
use std::sync::OnceLock;

use glam::Vec3;

use super::azar::{hash2, unit};
use super::punta::Punta;
use super::punteado;

/// Radio del nucleo solido y ancho del borde de la punta principal (como fraccion del radio): los de la dureza 0.6 de
/// photocraft (`TIP_SHAPE`).
const CORE: f32 = 0.70;
const SHOULDER: f32 = 0.24;
/// Hasta donde pinta la principal (el borde baja de 0.1 %), como fraccion del radio.
const PRIMARY_REACH: f32 = CORE + 2.6 * SHOULDER;
/// Cuanto se acerca el valor del borde a su decision al azar (0.7 en photocraft).
const NOISE_MIX: f32 = 0.7;
const NOISE_SALT: u64 = 0x006E_6F69_7365;

/// Tamano de la segunda punta respecto a la principal (30 de 50).
const DUAL_RADIUS: f32 = 30.0 / 50.0;
/// Dispersion de la segunda punta, como fraccion de SU radio, en cada eje.
const DUAL_SCATTER: f32 = 1.0;
/// Espaciado de la segunda punta (fraccion de su diametro) y de la principal (fraccion de su radio, el de `brush.rs`).
const DUAL_SPACING: f32 = 0.3;
const PRIMARY_SPACING: f32 = 0.5;
/// Cuantas gotas (de la segunda punta) caen en promedio por cada toque principal: 2 por paso de la segunda, que avanza
/// `0.3 * 2 * 0.6` radios, mientras la principal avanza `PRIMARY_SPACING`.
const DUAL_PER_DAB: f32 = 2.0 * PRIMARY_SPACING / (DUAL_SPACING * 2.0 * DUAL_RADIUS);
/// Semilla y gotas de la segunda punta (`spatter_tip(48, 23, 20)`).
const DUAL_TIP_SEED: u32 = 23;
const DUAL_TIP_DROPS: u32 = 20;

/// Lo mas que se aleja de un toque algo que cuente (la esquina de la dispersion mas el radio de la segunda punta), como
/// fraccion del radio del pincel.
pub const DUAL_GRAIN_REACH: f32 = DUAL_RADIUS * (1.0 + SQRT_2);

const SCATTER_X_SALT: u64 = 0x4455_5358;
const SCATTER_Y_SALT: u64 = 0x4455_5359;
const FLIP_SALT: u64 = 0x4455_464C;

/// La segunda punta: gotas sueltas, la del Punteado con otra semilla y menos gotas (se genera una sola vez).
fn punta_dual() -> &'static Punta {
    static TIP: OnceLock<Punta> = OnceLock::new();
    TIP.get_or_init(|| punteado::construir(DUAL_TIP_SEED, DUAL_TIP_DROPS))
}

/// `exp(-u)` como `(1 - u/64)^64` (la que usa photocraft para el borde suave).
fn soft_exp(u: f32) -> f32 {
    let mut g = (1.0 - u / 64.0).clamp(0.0, 1.0);
    for _ in 0..6 {
        g *= g;
    }
    g
}

/// Un toque de Doble grano: la punta principal y las gotas de la segunda que caen en este toque.
pub struct DualDab {
    radius: f32,
    dual_radius: f32,
    /// Ejes de la superficie.
    t: Vec3,
    b: Vec3,
    /// Cada gota de la segunda punta: su desplazamiento respecto al centro del toque y el volteo horizontal (+1 o -1).
    gotas: Vec<(Vec3, f32)>,
}

impl DualDab {
    /// `dab` es el numero del toque (desde 1), `normal` la normal de la superficie (cero = se usa Z) y `radius` el radio
    /// del pincel en las mismas unidades que los puntos.
    pub fn new(dab: u32, normal: Vec3, radius: f32) -> DualDab {
        let n = normal.try_normalize().unwrap_or(Vec3::Z);
        let helper = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
        let t = n.cross(helper).normalize();
        let b = n.cross(t);
        let dual_radius = (DUAL_RADIUS * radius).max(1e-6);
        // Las gotas llevan numero propio y consecutivo a lo largo del trazo: este toque pone las que van de
        // `(dab - 1) * DUAL_PER_DAB` a `dab * DUAL_PER_DAB` (asi el promedio es `DUAL_PER_DAB` y el primer toque pone 2).
        let first = (dab.saturating_sub(1) as f32 * DUAL_PER_DAB).floor() as u64;
        let last = (dab as f32 * DUAL_PER_DAB).floor() as u64;
        let k = DUAL_SCATTER * dual_radius;
        let gotas = (first..last)
            .map(|j| {
                let r = |salt: u64| unit(salt.wrapping_add(j));
                let shift = t * ((r(SCATTER_X_SALT) * 2.0 - 1.0) * k) + b * ((r(SCATTER_Y_SALT) * 2.0 - 1.0) * k);
                (shift, if r(FLIP_SALT) < 0.5 { -1.0 } else { 1.0 })
            })
            .collect();
        DualDab { radius, dual_radius, t, b, gotas }
    }

    /// Cuantas gotas de la segunda punta pone este toque.
    pub fn drops(&self) -> usize {
        self.gotas.len()
    }

    /// Hasta donde hay que revisar texeles alrededor del centro: lo mas lejos que llega la segunda punta.
    pub fn limit(&self) -> f32 {
        DUAL_GRAIN_REACH * self.radius
    }

    /// Cobertura (0..1) de la punta principal en un texel a distancia `d` del centro; `(tx, ty)` es el texel (el ruido
    /// del borde depende de el, no del toque, asi que no cambia de un toque al siguiente).
    pub fn primary(&self, d: f32, tx: u32, ty: u32) -> f32 {
        let dn = d / self.radius;
        if dn >= PRIMARY_REACH {
            return 0.0;
        }
        let mut v = if dn <= CORE {
            1.0
        } else {
            let x = (dn - CORE) / SHOULDER;
            soft_exp(x * x)
        };
        if v <= 0.0 {
            return 0.0;
        }
        if v < 1.0 {
            let hard = if hash2(tx as i32, ty as i32, NOISE_SALT) < v { 1.0 } else { 0.0 };
            v += (hard - v) * NOISE_MIX;
        }
        v
    }

    /// Cobertura (0..1) de las gotas de este toque en un texel; `offset` es su posicion respecto al centro del toque.
    /// Varias gotas se suman como en el trazo: `c + v * (1 - c)`.
    pub fn dual(&self, offset: Vec3) -> f32 {
        let inv = 1.0 / self.dual_radius;
        let mut c = 0.0f32;
        for (shift, flip) in &self.gotas {
            let o = offset - *shift;
            let (u, v) = (o.dot(self.t) * inv * flip, o.dot(self.b) * inv);
            let s = punta_dual().sample(u * 0.5 + 0.5, v * 0.5 + 0.5);
            c += s * (1.0 - c);
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};

    #[test]
    fn el_pincel_principal_es_un_disco_con_borde_suave_y_ruidoso() {
        let d = DualDab::new(1, Vec3::Z, 1.0);
        // El nucleo es solido y fuera del alcance no hay nada.
        for tx in 0..50 {
            assert_eq!(d.primary(0.5, tx, 3), 1.0);
            assert_eq!(d.primary(1.4, tx, 3), 0.0);
        }
        // En el borde (a un radio) hay ruido: valores distintos, todos entre 0 y 1, y la media sigue la del borde suave
        // (exp(-1.25^2) = 0.21).
        let vals: Vec<f32> = (0..400).map(|i| d.primary(1.0, i % 20, i / 20)).collect();
        assert!(vals.iter().all(|v| (0.0..=1.0).contains(v)));
        let distintos: HashSet<u32> = vals.iter().map(|v| (v * 1000.0) as u32).collect();
        assert!(distintos.len() >= 2, "sin ruido en el borde");
        let media = vals.iter().sum::<f32>() / vals.len() as f32;
        assert!((0.1..0.35).contains(&media), "media {media}");
        // El ruido depende del texel, no del toque.
        assert_eq!(d.primary(1.0, 7, 9), DualDab::new(55, Vec3::Z, 1.0).primary(1.0, 7, 9));
    }

    #[test]
    fn caen_unas_278_gotas_por_cada_100_toques_y_siempre_dos_o_tres() {
        let mut total = 0;
        for dab in 1..=100 {
            let n = DualDab::new(dab, Vec3::Z, 1.0).drops();
            assert!((2..=3).contains(&n), "toque {dab}: {n} gotas");
            total += n;
        }
        assert!((270..=285).contains(&total), "total {total}");
    }

    #[test]
    fn la_segunda_punta_cae_dentro_de_su_alcance_y_en_los_dos_ejes() {
        let radio = 2.0;
        let (mut en_x, mut en_y, mut cubierto) = (false, false, false);
        for dab in 1..60 {
            let d = DualDab::new(dab, Vec3::Z, radio);
            assert!((d.limit() - DUAL_GRAIN_REACH * radio).abs() < 1e-5);
            for k in 0..72 {
                let a = k as f32 * std::f32::consts::TAU / 72.0;
                // Un poco mas lejos de lo que llega: nada.
                let p = Vec3::new(a.cos(), a.sin(), 0.0) * (d.limit() * 1.01);
                assert_eq!(d.dual(p), 0.0, "toque {dab}, angulo {k}");
            }
            for ix in -20..=20 {
                for iy in -20..=20 {
                    let p = Vec3::new(ix as f32 * 0.1, iy as f32 * 0.1, 0.0);
                    let v = d.dual(p);
                    assert!((0.0..=1.0).contains(&v));
                    if v > 0.5 {
                        cubierto = true;
                        en_x |= p.x.abs() > 0.8;
                        en_y |= p.y.abs() > 0.8;
                    }
                }
            }
        }
        assert!(cubierto, "las gotas no pintan nada");
        assert!(en_x && en_y, "no se dispersan en los dos ejes");
    }

    #[test]
    fn el_mismo_toque_da_lo_mismo() {
        let (a, b) = (DualDab::new(9, Vec3::Z, 1.5), DualDab::new(9, Vec3::Z, 1.5));
        let p = Vec3::new(0.3, -0.4, 0.0);
        assert_eq!(a.dual(p), b.dual(p));
        assert_eq!(a.primary(1.1, 4, 5), b.primary(1.1, 4, 5));
    }

    #[test]
    fn el_trazo_sale_granulado_y_no_pasa_de_su_alcance() {
        let p = [Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 0.0), Vec3::new(0.0, 2.0, 0.0)];
        let uv = [[0.25, 0.25], [0.5, 0.25], [0.5, 0.75], [0.25, 0.75]];
        let tris = [
            SurfTri { p: [p[0], p[1], p[2]], uv: [uv[0], uv[1], uv[2]] },
            SurfTri { p: [p[0], p[2], p[3]], uv: [uv[0], uv[2], uv[3]] },
        ];
        let opts = DabOptions { brush: BrushType::DualGrain, normal: Vec3::Z, ..Default::default() };
        let pintar = || {
            let mut s = StrokeState::new(64);
            let mut pixels = vec![0u8; 64 * 64 * 4];
            s.begin_stroke();
            let radio = 0.5;
            for i in 0..16 {
                let c = Vec3::new(0.5 + i as f32 * 0.5 * radio, 1.0, 0.0);
                s.dab_surface(&mut pixels, &tris, c, radio, [200, 40, 40], 1.0, opts);
            }
            pixels
        };
        let pixels = pintar();
        // Es determinista.
        assert_eq!(pixels, pintar());
        let (mut pintados, mut alfas, mut lejos) = (0, HashSet::new(), 0);
        for ty in 0..64u32 {
            for tx in 0..64u32 {
                let i = ((ty * 64 + tx) * 4) as usize;
                let a = pixels[i + 3];
                // Posicion 3D del texel dentro del cuadrado (el UV es de 0.25 x 0.5 de la textura).
                let x = (tx as f32 / 63.0 - 0.25) / 0.25 * 2.0;
                let y = (ty as f32 / 63.0 - 0.25) / 0.5 * 2.0;
                if a > 0 {
                    pintados += 1;
                    alfas.insert(a);
                    // Mas arriba o abajo de la franja que cubre el pincel (con su alcance) no hay pintura.
                    if (y - 1.0).abs() > 0.5 * DUAL_GRAIN_REACH + 0.15 {
                        lejos += 1;
                    }
                    assert_eq!(&pixels[i..i + 3], &[200, 40, 40], "el color no es el del pincel");
                }
                let _ = x;
            }
        }
        assert_eq!(lejos, 0, "pinto mas alla de su alcance");
        assert!(pintados > 50, "pinto pocos texeles: {pintados}");
        assert!(alfas.len() > 3, "la opacidad es casi pareja: {} valores", alfas.len());
    }
}
