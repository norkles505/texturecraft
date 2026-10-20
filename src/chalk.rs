//! Pincel de tiza (portado del Chalk de photocraft). Tiene tres piezas, todas deterministas (sin archivos ni azar):
//!
//! - La PUNTA: un bitmap de 64x64 en gris. Es un disco de borde irregular (80 a 95 % del radio) con grano: el 20 % de
//!   sus puntos casi no pinta y el resto va del 70 al 100 %.
//! - El ANGULO y el TAMANO de cada toque: gira un angulo al azar (de -180 a 180 grados) y mide del 85 al 100 % del
//!   radio. Como cada toque gira distinto, el grano no se repite en fila y el trazo se ve organico.
//! - El PAPEL: ruido fractal en 3D anclado a la posicion en el modelo (no se estira con el UV) mas un grano por texel.
//!   Multiplica la cobertura: `1 - PAPEL_PROFUNDIDAD + PAPEL_PROFUNDIDAD * papel`, o sea de 0.4 (papel oscuro) a 1.
//!
//! El angulo al azar permite orientar la punta con cualquier base de la superficie: no hace falta una direccion
//! coherente entre triangulos.
//!
//! Tambien estan aqui los dos pinceles de medio seco portados de photocraft (`DryDab`): el CARBONCILLO (punta de elipse
//! con vetas que sigue la direccion del trazo) y el PASTEL (punta de grano que gira, se aplasta un poco y se dispersa de
//! lado). Su papel o lienzo no multiplica cada toque: RESTA cobertura al trazo ya acumulado (`dry_subtract`, modo Subtract
//! de photocraft), asi que dos toques sobre el mismo sitio no cambian el dibujo del papel.

use std::f32::consts::PI;
use std::sync::OnceLock;

use glam::Vec3;

/// Lado del bitmap de la punta.
pub const TIP_SIZE: usize = 64;
/// Semillas de las puntas (las de photocraft: tiza 7, pastel 31, carboncillo 4).
const CHALK_TIP_SEED: u32 = 7;
const PASTEL_TIP_SEED: u32 = 31;
const CHARCOAL_TIP_SEED: u32 = 4;
const PAPER_SALT: u64 = 0x5041_5045_5233;
const DAB_ANGLE_SALT: u64 = 0x414E_474C;
const DAB_SIZE_SALT: u64 = 0x5349_5A45;
/// Cuanto oscurece el papel donde es oscuro (0.6 = la cobertura baja hasta el 40 %).
pub const PAPER_DEPTH: f32 = 0.6;
/// Tamano de la celda base del ruido del papel, en unidades del modelo (el slider de tamano mide 0.01 por punto).
const PAPER_CELL: f32 = 0.08;
const PAPER_OCTAVES: u32 = 3;
/// Variacion del tamano de cada toque (0.15 = del 85 al 100 %).
const SIZE_JITTER: f32 = 0.15;
/// Para las vistas previas del panel (que miden en pixeles): unidades del modelo por pixel de la imagen.
pub const PREVIEW_UNITS_PER_PX: f32 = 0.011;

// Carboncillo y pastel (medio seco). Los numeros son los de los presets de photocraft.
/// Papel del carboncillo: semilla 5, profundidad 0.7, modo Subtract invertido.
const CHARCOAL_PAPER_SALT: u64 = PAPER_SALT + 5;
const CHARCOAL_PAPER_DEPTH: f32 = 0.7;
/// Lienzo del pastel: baldosa de 64, semilla 2, profundidad 0.55, modo Subtract invertido.
const PASTEL_CANVAS_DEPTH: f32 = 0.55;
const CANVAS_SALT: u64 = 2 * 0x0100_0000_01B3 + 17;
/// Un hilo del lienzo mide 8 texeles y la trama se repite cada 64.
const CANVAS_PERIOD: f32 = 8.0;
const CANVAS_TILE: u32 = 64;
/// Cuanto varia la redondez de cada toque de pastel (0.3 = del 70 al 100 %).
const ROUNDNESS_JITTER: f32 = 0.3;
const DAB_ROUND_SALT: u64 = 0x524F_554E_44;
const DAB_SCATTER_SALT: u64 = 0x5343_4154_54;
/// Dispersion del pastel: hasta esta fraccion del radio, de lado respecto al trazo (0.25 = 25 %).
pub const PASTEL_SCATTER: f32 = 0.25;

// ---------------------------------------------------------------------------
// Azar determinista
// ---------------------------------------------------------------------------

/// Mezcla de 64 bits (splitmix64): pocos cambios en la entrada dan una salida muy distinta.
fn mix(z: u64) -> u64 {
    let z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Numero entre 0 y 1 (sin incluir el 1) a partir de una semilla.
fn unit(z: u64) -> f32 {
    (mix(z) >> 40) as f32 / 16_777_216.0
}

fn hash2(x: i32, y: i32, salt: u64) -> f32 {
    unit(mix(salt ^ u64::from(x as u32)) ^ (u64::from(y as u32) << 32))
}

fn hash3(x: i32, y: i32, z: i32, salt: u64) -> f32 {
    unit(mix(mix(salt ^ u64::from(x as u32)) ^ (u64::from(y as u32) << 32)) ^ u64::from(z as u32))
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Ruido de valores 2D que se repite cada `period` celdas.
fn value_noise2(x: f32, y: f32, period: i32, salt: u64) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (tx, ty) = (smooth(x - x0), smooth(y - y0));
    let (xi, yi) = (x0 as i32, y0 as i32);
    let h = |i: i32, j: i32| hash2(i.rem_euclid(period), j.rem_euclid(period), salt);
    let a = lerp(h(xi, yi), h(xi + 1, yi), tx);
    let b = lerp(h(xi, yi + 1), h(xi + 1, yi + 1), tx);
    lerp(a, b, ty)
}

/// Ruido de valores 3D.
fn value_noise3(p: Vec3, salt: u64) -> f32 {
    let f = p.floor();
    let t = p - f;
    let (tx, ty, tz) = (smooth(t.x), smooth(t.y), smooth(t.z));
    let (xi, yi, zi) = (f.x as i32, f.y as i32, f.z as i32);
    let h = |dx: i32, dy: i32, dz: i32| hash3(xi + dx, yi + dy, zi + dz, salt);
    let x00 = lerp(h(0, 0, 0), h(1, 0, 0), tx);
    let x10 = lerp(h(0, 1, 0), h(1, 1, 0), tx);
    let x01 = lerp(h(0, 0, 1), h(1, 0, 1), tx);
    let x11 = lerp(h(0, 1, 1), h(1, 1, 1), tx);
    lerp(lerp(x00, x10, ty), lerp(x01, x11, ty), tz)
}

/// Ruido fractal 3D entre 0 y 1: varias octavas, cada una al doble de frecuencia y la mitad de fuerza.
fn fbm3(p: Vec3, octaves: u32, salt: u64) -> f32 {
    let (mut sum, mut amp, mut norm, mut freq) = (0.0f32, 1.0f32, 0.0f32, 1.0f32);
    for o in 0..octaves {
        sum += value_noise3(p * freq, salt.wrapping_add(u64::from(o) * 7919)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

// ---------------------------------------------------------------------------
// Papel
// ---------------------------------------------------------------------------

/// Valor del papel (0..1) en la posicion `pos` del modelo. `tx`, `ty` son el texel: el grano fino va por texel, asi
/// que no cambia de un toque a otro. Mismo punto, mismo valor: dos trazos sobre el mismo sitio ven el mismo papel.
pub fn paper(pos: Vec3, tx: u32, ty: u32) -> f32 {
    paper_value(pos, tx, ty, PAPER_SALT)
}

/// El papel con su propia semilla (cada pincel de papel usa una distinta).
fn paper_value(pos: Vec3, tx: u32, ty: u32, salt: u64) -> f32 {
    let base = fbm3(pos / PAPER_CELL, PAPER_OCTAVES, salt);
    let grain = hash2(tx as i32, ty as i32, salt ^ 0x55);
    ((base - 0.5) * 2.2 + 0.5 + (grain - 0.5) * 0.35).clamp(0.0, 1.0)
}

/// Cuanto deja pasar el papel de la cobertura del trazo (de `1 - PAPER_DEPTH` a 1).
pub fn paper_mask(pos: Vec3, tx: u32, ty: u32) -> f32 {
    1.0 - PAPER_DEPTH + PAPER_DEPTH * paper(pos, tx, ty)
}

// ---------------------------------------------------------------------------
// Punta
// ---------------------------------------------------------------------------

/// La punta de la tiza: `TIP_SIZE` x `TIP_SIZE` valores entre 0 y 1 (1 = pinta del todo).
pub struct ChalkTip {
    data: Vec<f32>,
}

impl ChalkTip {
    /// La punta de tiza/pastel (`chalk_tip(64, seed)` de photocraft): la semilla cambia el borde y el grano.
    fn build(seed: u32) -> ChalkTip {
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
        ChalkTip { data }
    }

    /// La punta del carboncillo (`charcoal_tip(64, seed)` de photocraft): una elipse achatada (el eje vertical cuenta
    /// 1.6 veces, asi que es mas ancha que alta) de vetas horizontales cortadas, con el borde granuloso y un 15 % de
    /// granos casi vacios.
    fn build_charcoal(seed: u32) -> ChalkTip {
        let salt = u64::from(seed) + 404;
        let n = TIP_SIZE;
        let c = n as f32 / 2.0;
        let mut data = Vec::with_capacity(n * n);
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = ((x as f32 + 0.5 - c) / c, (y as f32 + 0.5 - c) / c);
                let r = (dx * dx + (dy * 1.6).powi(2)).sqrt();
                let edge = 0.85 + 0.15 * value_noise2(dx * 6.0 + 9.0, dy * 6.0 + 9.0, 64, salt);
                if r > edge {
                    data.push(0.0);
                    continue;
                }
                // Vetas: la densidad de cada fila se modula a lo largo de x para que se corten.
                let row = hash2(0, y as i32, salt);
                let along = value_noise2(x as f32 / 6.0, y as f32 / 1.5, 1 << 12, salt ^ 0x77);
                let grit = hash2(x as i32, y as i32, salt ^ 0x13);
                let v = (0.35 + 0.65 * row) * (0.4 + 0.6 * along) * if grit < 0.15 { 0.2 } else { 1.0 };
                data.push(v * ((edge - r) * 6.0).clamp(0.0, 1.0));
            }
        }
        ChalkTip { data }
    }

    /// Valor de la punta en `(u, v)` (0..1 sobre el bitmap, interpolado). Fuera del bitmap es 0.
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return 0.0;
        }
        let n = TIP_SIZE as i32;
        let (fx, fy) = (u * n as f32 - 0.5, v * n as f32 - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i32, y0 as i32);
        let at = |x: i32, y: i32| if x < 0 || y < 0 || x >= n || y >= n { 0.0 } else { self.data[(y * n + x) as usize] };
        let a = lerp(at(x0, y0), at(x0 + 1, y0), tx);
        let b = lerp(at(x0, y0 + 1), at(x0 + 1, y0 + 1), tx);
        lerp(a, b, ty)
    }
}

/// La punta compartida (se genera una sola vez).
pub fn chalk_tip() -> &'static ChalkTip {
    static TIP: OnceLock<ChalkTip> = OnceLock::new();
    TIP.get_or_init(|| ChalkTip::build(CHALK_TIP_SEED))
}

/// La punta del pastel (la de la tiza con otra semilla).
pub fn pastel_tip() -> &'static ChalkTip {
    static TIP: OnceLock<ChalkTip> = OnceLock::new();
    TIP.get_or_init(|| ChalkTip::build(PASTEL_TIP_SEED))
}

/// La punta del carboncillo.
pub fn charcoal_tip() -> &'static ChalkTip {
    static TIP: OnceLock<ChalkTip> = OnceLock::new();
    TIP.get_or_init(|| ChalkTip::build_charcoal(CHARCOAL_TIP_SEED))
}

/// Angulo (radianes, de -pi a pi) y factor de tamano (0.85 a 1) del toque numero `dab`.
pub fn dab_params(dab: u32) -> (f32, f32) {
    let angle = (unit(DAB_ANGLE_SALT.wrapping_add(u64::from(dab))) * 2.0 - 1.0) * PI;
    let size = 1.0 - SIZE_JITTER * unit(DAB_SIZE_SALT.wrapping_add(u64::from(dab)));
    (angle, size)
}

// ---------------------------------------------------------------------------
// Un toque
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Medio seco: carboncillo y pastel
// ---------------------------------------------------------------------------

/// Los dos pinceles de medio seco portados de photocraft.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DryKind {
    Charcoal,
    Pastel,
}

/// Valor del lienzo (0..1) en el texel `(tx, ty)`: una trama de hilos de `CANVAS_PERIOD` texeles que pasan por encima y
/// por debajo, con un poco de grano. Se repite cada `CANVAS_TILE` texeles: esta anclado a la textura (el UV), no a la
/// pantalla.
fn canvas_value(tx: u32, ty: u32) -> f32 {
    let (x, y) = (tx % CANVAS_TILE, ty % CANVAS_TILE);
    let cell = |v: u32| (v as f32 / CANVAS_PERIOD).floor();
    let within = |v: u32| (v as f32 % CANVAS_PERIOD) / CANVAS_PERIOD;
    let over = (cell(x) + cell(y)) as u32 % 2 == 0;
    // Hilo horizontal (varia en y) o vertical (varia en x), segun quien este por encima en esta celda.
    let thread = if over { (PI * within(y)).sin() } else { (PI * within(x)).sin() };
    let grain = hash2(x as i32, y as i32, CANVAS_SALT);
    ((0.2 + 0.8 * thread) * (0.85 + 0.15 * grain)).clamp(0.0, 1.0)
}

/// Cuanta cobertura RESTA el papel (carboncillo) o el lienzo (pastel) en un texel: `profundidad * t`, con `t` el patron
/// invertido (1 - valor). Va de 0 a la profundidad. Es el modo Subtract de photocraft: se resta a la cobertura YA
/// acumulada del trazo, no a cada toque, asi que pasar dos veces por el mismo sitio no cambia el dibujo del papel.
pub fn dry_subtract(kind: DryKind, pos: Vec3, tx: u32, ty: u32) -> f32 {
    match kind {
        DryKind::Charcoal => CHARCOAL_PAPER_DEPTH * (1.0 - paper_value(pos, tx, ty, CHARCOAL_PAPER_SALT)),
        DryKind::Pastel => PASTEL_CANVAS_DEPTH * (1.0 - canvas_value(tx, ty)),
    }
}

/// Un toque de carboncillo o pastel ya preparado.
///
/// - Carboncillo: la punta (elipse con vetas) apunta en la direccion del trazo, sobre el plano de la superficie.
///   Si no se sabe hacia donde va (primer toque) se usa una base fija. Sin variacion de tamano.
/// - Pastel: la punta gira un angulo al azar en cada toque, se aplasta hasta `ROUNDNESS_JITTER` y el centro se
///   desplaza de lado respecto al trazo hasta `PASTEL_SCATTER` del radio.
pub struct DryDab {
    kind: DryKind,
    /// Ejes de la punta sobre la superficie.
    t: Vec3,
    b: Vec3,
    inv_radius: f32,
    /// Factor de redondez de la punta (1 = redonda); aplasta el eje `b`.
    roundness: f32,
    /// Desplazamiento del centro del toque (dispersion del pastel); cero para el carboncillo.
    shift: Vec3,
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
            DryKind::Charcoal => {
                let t = along.unwrap_or(t0);
                DryDab { kind, t, b: n.cross(t), inv_radius, roundness: 1.0, shift: Vec3::ZERO }
            }
            DryKind::Pastel => {
                let (angle, _) = dab_params(dab);
                let (sn, cs) = angle.sin_cos();
                let roundness = 1.0 - ROUNDNESS_JITTER * unit(DAB_ROUND_SALT.wrapping_add(u64::from(dab)));
                // De lado respecto al trazo (si no se sabe hacia donde va, a lo largo de un eje cualquiera).
                let side = along.map_or(b0, |a| n.cross(a));
                let k = (unit(DAB_SCATTER_SALT.wrapping_add(u64::from(dab))) * 2.0 - 1.0) * PASTEL_SCATTER * radius;
                DryDab { kind, t: t0 * cs + b0 * sn, b: b0 * cs - t0 * sn, inv_radius, roundness, shift: side * k }
            }
        }
    }

    /// Cuanto se aleja el centro de la punta del punto tocado (para ampliar la zona que se revisa).
    pub fn reach(&self) -> f32 {
        self.shift.length()
    }

    /// Cobertura (0..1) de la punta en un texel; `offset` es su posicion respecto al punto tocado. Es 0 fuera de la
    /// punta. El papel o el lienzo no entran aqui: restan despues (`dry_subtract`).
    pub fn alpha(&self, offset: Vec3) -> f32 {
        let o = offset - self.shift;
        let (u, v) = (o.dot(self.t) * self.inv_radius, o.dot(self.b) * self.inv_radius / self.roundness);
        let tip = match self.kind {
            DryKind::Charcoal => charcoal_tip(),
            DryKind::Pastel => pastel_tip(),
        };
        tip.sample(u * 0.5 + 0.5, v * 0.5 + 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_punta_del_carboncillo_es_mas_ancha_que_alta() {
        let t = charcoal_tip();
        let n = TIP_SIZE;
        let fila = (0..n).filter(|&x| t.data[(n / 2) * n + x] > 0.0).count();
        let columna = (0..n).filter(|&y| t.data[y * n + n / 2] > 0.0).count();
        assert!(fila > columna + 10, "fila {fila}, columna {columna}");
    }

    #[test]
    fn el_carboncillo_sigue_la_direccion_del_trazo() {
        // El trazo va por Y: la punta es larga a lo largo de Y y corta de lado.
        let d = DryDab::new(DryKind::Charcoal, 1, Vec3::Z, Vec3::Y, 1.0);
        let suma = |dir: Vec3| (-9..=9).map(|k| d.alpha(dir * (k as f32 / 10.0))).sum::<f32>();
        assert!(suma(Vec3::Y) > suma(Vec3::X) * 1.2, "{} no pasa de {}", suma(Vec3::Y), suma(Vec3::X));
        // Sin direccion (primer toque) usa una base fija y no falla.
        let sin = DryDab::new(DryKind::Charcoal, 1, Vec3::Z, Vec3::ZERO, 1.0);
        assert!((0.0..=1.0).contains(&sin.alpha(Vec3::new(0.1, 0.1, 0.0))));
    }

    #[test]
    fn el_pastel_se_dispersa_solo_de_lado() {
        let mut algun_desvio = false;
        for dab in 1..100 {
            let d = DryDab::new(DryKind::Pastel, dab, Vec3::Z, Vec3::X, 2.0);
            assert!(d.shift.dot(Vec3::X).abs() < 1e-4, "se desplaza a lo largo del trazo");
            assert!(d.reach() <= PASTEL_SCATTER * 2.0 + 1e-5);
            assert!((1.0 - ROUNDNESS_JITTER - 1e-6..=1.0).contains(&d.roundness));
            algun_desvio |= d.reach() > 0.05;
        }
        assert!(algun_desvio);
        // El carboncillo no se dispersa.
        assert_eq!(DryDab::new(DryKind::Charcoal, 5, Vec3::Z, Vec3::X, 2.0).reach(), 0.0);
    }

    #[test]
    fn la_resta_del_papel_y_del_lienzo_esta_en_rango_y_varia() {
        for (kind, depth) in [(DryKind::Charcoal, CHARCOAL_PAPER_DEPTH), (DryKind::Pastel, PASTEL_CANVAS_DEPTH)] {
            let vals: Vec<f32> = (0..300u32)
                .map(|i| dry_subtract(kind, Vec3::new(i as f32 * 0.011, 0.2, i as f32 * 0.005), i, i * 3 % 1024))
                .collect();
            assert!(vals.iter().all(|v| (0.0..=depth + 1e-6).contains(v)), "{kind:?}");
            let (lo, hi) = (vals.iter().cloned().fold(f32::MAX, f32::min), vals.iter().cloned().fold(0.0, f32::max));
            assert!(hi - lo > 0.2 * depth, "{kind:?} es casi plano: de {lo} a {hi}");
            // Mismo punto, mismo valor.
            assert_eq!(dry_subtract(kind, Vec3::ONE, 5, 9), dry_subtract(kind, Vec3::ONE, 5, 9));
        }
    }

    #[test]
    fn el_lienzo_se_repite_cada_baldosa() {
        for (x, y) in [(0, 0), (3, 10), (17, 40), (63, 63)] {
            assert_eq!(canvas_value(x, y), canvas_value(x + CANVAS_TILE, y));
            assert_eq!(canvas_value(x, y), canvas_value(x, y + 3 * CANVAS_TILE));
        }
    }

    #[test]
    fn la_punta_es_determinista_y_tiene_pintura() {
        let a = ChalkTip::build(CHALK_TIP_SEED);
        let b = ChalkTip::build(CHALK_TIP_SEED);
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
    fn el_papel_esta_en_rango_es_igual_en_el_mismo_punto_y_varia() {
        let mut vals = Vec::new();
        for i in 0..200 {
            let p = Vec3::new(i as f32 * 0.013, 0.3, i as f32 * 0.007);
            let v = paper(p, i, 5);
            assert!((0.0..=1.0).contains(&v));
            assert_eq!(v, paper(p, i, 5));
            vals.push(v);
        }
        let media = vals.iter().sum::<f32>() / vals.len() as f32;
        let var = vals.iter().map(|v| (v - media).abs()).sum::<f32>() / vals.len() as f32;
        assert!(var > 0.03, "el papel es casi plano: {var}");
    }

    #[test]
    fn la_mascara_del_papel_va_de_040_a_1() {
        for i in 0..100 {
            let m = paper_mask(Vec3::new(i as f32 * 0.02, 0.1, 0.4), i, i);
            assert!((1.0 - PAPER_DEPTH - 1e-6..=1.0 + 1e-6).contains(&m), "{m}");
        }
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
