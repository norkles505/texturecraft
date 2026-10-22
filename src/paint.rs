//! Pincel en CPU (ESPECIFICACION.md, secciones 7.1 y 7.2): estado del trazo + un toque con el `falloff` del pincel
//! elegido (`brush.rs`), modo Normal.
//! Los pixeles de la capa los pone quien llama; el estado del trazo se comparte entre capas (una pieza = un trazo
//! a la vez), como `strokeAlpha`/`strokeBase`/`strokeLastDab` de la app Android.
//!
//! Dentro de una misma pasada la opacidad es el MAXIMO de los toques (no se acumula); si el trazo se va y vuelve
//! (hueco de mas de 3 toques) la segunda pasada si suma.
//! Con `DabOptions::ceiling` (el interruptor "Acumular pasada" de Opciones, apagado) el trazo funciona como en
//! photocraft: la cobertura de cada texel sube poco a poco hacia la opacidad (`c + v * (techo - c)`), volver a
//! pasar por el mismo sitio nunca pasa del techo, y solo un trazo nuevo suma encima.
//!
//! El toque (`dab_surface`) mide en el MODELO, no en la imagen: de cada texel de los triangulos cercanos calcula su
//! posicion 3D y usa la distancia 3D al punto tocado. Asi el pincel mide lo mismo en todas partes (no se achica
//! donde el UV esta estirado) y un trazo cruza las costuras entre islas. Cada isla se rellena ademas
//! `PAD_TEXELS` texeles mas alla de su borde, para que el filtrado de la textura no muestre la costura.

use glam::Vec3;

use crate::brush::BrushType;
use crate::pinceles::{
    ChalkDab, DryDab, DryKind, DualDab, dry_hardmix, dry_mezcla_toque, dry_subtract, hard_mix,
    paper_mask, wet_factor,
};

/// Lado de la textura de cada pieza (fijo en la app Android).
pub const TEX_SIZE: u32 = 1024;

/// Un hueco de mas de tantos toques seguidos sin tocar un texel empieza una pasada nueva en ese texel.
const PASS_GAP: u32 = 3;

/// Texeles que se pintan mas alla del borde de cada triangulo (relleno de costuras).
const PAD_TEXELS: f32 = 2.0;
/// Tolerancia (coordenadas baricentricas) para dar por dentro a un texel justo sobre el borde de un triangulo.
const INSIDE_EPS: f32 = 1e-4;

pub type Rect = (u32, u32, u32, u32);

/// Opciones de la capa donde se pinta (seccion 4.2): con bloqueo alfa solo cambia el color de lo que ya tiene algo.
/// `erase` convierte el toque en borrador (seccion 7.2): baja el alfa y no toca el color.
#[derive(Clone, Copy)]
pub struct DabOptions {
    pub alpha_lock: bool,
    pub erase: bool,
    /// Pincel elegido: de el sale el `falloff` del toque (seccion 7.5). El borrador usa el mismo.
    pub brush: BrushType,
    /// Normal de la superficie en el punto tocado (solo la usa la tiza para orientar su punta; cero = no se sabe).
    pub normal: Vec3,
    /// Modo "techo" (como photocraft): la cobertura sube hacia la opacidad y no pasa de ella dentro del trazo. Apagado
    /// (por defecto) = la regla de siempre: el maximo de los toques, y una segunda pasada tras un hueco suma.
    pub ceiling: bool,
    /// Flujo del pincel (0..1): multiplica la cobertura de cada toque antes de mezclarlo (`out = val * flow` en
    /// photocraft). 1 = sin efecto. Se aplica en las dos reglas de mezcla.
    pub flow: f32,
    /// Direccion del trazo en 3D (unitaria; cero = no se sabe, por ejemplo en el primer toque). Solo la usa el
    /// carboncillo, que orienta su punta a lo largo del trazo.
    pub dir: Vec3,
}

impl Default for DabOptions {
    fn default() -> DabOptions {
        DabOptions {
            alpha_lock: false,
            erase: false,
            brush: BrushType::default(),
            normal: Vec3::ZERO,
            ceiling: false,
            flow: 1.0,
            dir: Vec3::ZERO,
        }
    }
}

/// Un triangulo del modelo que el toque puede pintar: sus 3 vertices en 3D y sus UV.
#[derive(Clone, Copy, Debug)]
pub struct SurfTri {
    pub p: [Vec3; 3],
    pub uv: [[f32; 2]; 3],
}

pub struct StrokeState {
    pub size: u32,
    /// Mayor opacidad aplicada a cada texel en la pasada actual (0 = sin tocar).
    stroke_alpha: Vec<f32>,
    /// Color de la capa ANTES del trazo.
    stroke_base: Vec<u8>,
    /// Ultimo toque que paso por cada texel.
    stroke_last: Vec<u32>,
    /// Ultimo toque que ya reviso cada texel dentro de un triangulo (para no pisar con el relleno de otra isla).
    dab_mark: Vec<u32>,
    /// Solo el Doble grano: cobertura acumulada de la segunda punta (las gotas) en cada texel durante el trazo. Se
    /// limpia con `stroke_alpha`.
    stroke_dual: Vec<f32>,
    /// Solo la esponja de tinta (bordes mojados): lo mas hondo que llego una punta a cada texel durante el trazo, o sea
    /// el valor mas alto de la punta que paso por ahi. Se limpia con `stroke_alpha`.
    stroke_depth: Vec<f32>,
    stroke_dab: u32,
    /// Rectangulo tocado por el trazo (para limpiar `stroke_alpha` sin recorrer todo).
    touched: Option<Rect>,
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    match a {
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        None => b,
    }
}

/// Caja de texeles que tocara un toque en espacio UV en `(u, v)` (extremos incluidos).
#[allow(dead_code)]
pub fn dab_box(size: u32, u: f32, v: f32, radius: f32) -> Rect {
    let max = (size - 1) as f32;
    let (u, v) = (u - u.floor(), v - v.floor());
    let (cx, cy) = (u * max, v * max);
    (
        (cx - radius).floor().max(0.0) as u32,
        (cy - radius).floor().max(0.0) as u32,
        (cx + radius).ceil().min(max) as u32,
        (cy + radius).ceil().min(max) as u32,
    )
}

/// UV del triangulo desplazada a la repeticion 0..1 donde cae su centro (la textura se repite fuera de 0..1).
fn wrapped_uv(uv: &[[f32; 2]; 3]) -> [[f32; 2]; 3] {
    let ou = ((uv[0][0] + uv[1][0] + uv[2][0]) / 3.0).floor();
    let ov = ((uv[0][1] + uv[1][1] + uv[2][1]) / 3.0).floor();
    std::array::from_fn(|i| [uv[i][0] - ou, uv[i][1] - ov])
}

/// Caja de texeles de un triangulo (UV ya envuelta) ampliada `PAD_TEXELS`, o `None` si cae fuera de la textura.
fn tri_box(size: u32, uv: &[[f32; 2]; 3]) -> Option<Rect> {
    let max = (size - 1) as f32;
    let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in uv {
        lo_x = lo_x.min(p[0] * max);
        hi_x = hi_x.max(p[0] * max);
        lo_y = lo_y.min(p[1] * max);
        hi_y = hi_y.max(p[1] * max);
    }
    let (x0, x1) = (
        (lo_x - PAD_TEXELS).floor().max(0.0),
        (hi_x + PAD_TEXELS).ceil().min(max),
    );
    let (y0, y1) = (
        (lo_y - PAD_TEXELS).floor().max(0.0),
        (hi_y + PAD_TEXELS).ceil().min(max),
    );
    if x0 > x1 || y0 > y1 {
        return None;
    }
    Some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
}

/// Rectangulo de texeles que tocara `StrokeState::dab_surface` con estos triangulos (el mismo que devuelve). Sirve
/// para guardar las baldosas de deshacer ANTES de modificar nada.
pub fn surface_rect(size: u32, tris: &[SurfTri]) -> Option<Rect> {
    let mut rect: Option<Rect> = None;
    for t in tris {
        if let Some(b) = tri_box(size, &wrapped_uv(&t.uv)) {
            rect = Some(union(rect, b));
        }
    }
    rect
}

/// Un triangulo ya preparado para recorrer sus texeles: vertices en texeles, determinante y alturas.
struct Prep {
    p: [Vec3; 3],
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    det: f32,
    /// Altura del triangulo (en texeles) desde cada vertice a su lado opuesto: distancia al borde = peso * altura.
    h: [f32; 3],
    bx: Rect,
    /// La posicion 3D de un texel `(px, py)` es una funcion afin: `p[2] + (px - c.0) * gx + (py - c.1) * gy`. Con ella
    /// se calcula, fila por fila, el tramo de columnas que cae dentro del radio del pincel (`row_span`).
    gx: Vec3,
    gy: Vec3,
}

/// Pesos baricentricos `(w0, w1, w2)` del texel `(px, py)` en el triangulo `pr`.
fn bary(pr: &Prep, px: f32, py: f32) -> (f32, f32, f32) {
    let w0 = ((pr.b.1 - pr.c.1) * (px - pr.c.0) + (pr.c.0 - pr.b.0) * (py - pr.c.1)) / pr.det;
    let w1 = ((pr.c.1 - pr.a.1) * (px - pr.c.0) + (pr.a.0 - pr.c.0) * (py - pr.c.1)) / pr.det;
    (w0, w1, 1.0 - w0 - w1)
}

/// El texel esta dentro del triangulo (con la misma tolerancia que usa la pasada 0).
fn inside(pr: &Prep, px: f32, py: f32) -> bool {
    let (w0, w1, w2) = bary(pr, px, py);
    w0 >= -INSIDE_EPS && w1 >= -INSIDE_EPS && w2 >= -INSIDE_EPS
}

/// Tramo de columnas `(desde, hasta)` de la fila `y` del triangulo `pr` donde la posicion 3D puede quedar a menos de
/// `limit` del centro del toque, o `None` si en esa fila no hay ninguna. Es una cota con un poco de margen (resuelve
/// una ecuacion de segundo grado: la distancia al centro es una cuadratica de la columna): fuera del tramo ningun texel
/// pasa la prueba de distancia, y dentro se sigue comprobando texel a texel. Asi un toque cuesta lo que mide el pincel
/// y no lo que mide la caja del triangulo.
fn row_span(pr: &Prep, y: u32, center: Vec3, limit: f32) -> Option<(u32, u32)> {
    let (x0, _, x1, _) = pr.bx;
    let q = pr.p[2] + pr.gy * (y as f32 - pr.c.1) - center;
    let a = pr.gx.length_squared();
    let l2 = limit * limit;
    if a < 1e-12 {
        // La posicion casi no cambia a lo largo de la fila: o toda la fila queda dentro o ningun texel.
        return (q.length() <= limit + 1100.0 * a.sqrt()).then_some((x0, x1));
    }
    let b = q.dot(pr.gx);
    let disc = b * b - a * (q.length_squared() - l2);
    if disc < -1e-5 * a * l2 {
        return None;
    }
    let s = disc.max(0.0).sqrt();
    let lo = (pr.c.0 + (-b - s) / a - 2.0).floor().max(x0 as f32);
    let hi = (pr.c.0 + (-b + s) / a + 2.0).ceil().min(x1 as f32);
    (lo <= hi).then_some((lo as u32, hi as u32))
}

impl StrokeState {
    pub fn new(size: u32) -> StrokeState {
        let n = (size * size) as usize;
        StrokeState {
            size,
            stroke_alpha: vec![0.0; n],
            stroke_base: vec![0; n * 4],
            stroke_last: vec![0; n],
            dab_mark: vec![0; n],
            stroke_dual: vec![0.0; n],
            stroke_depth: vec![0.0; n],
            stroke_dab: 0,
            touched: None,
        }
    }

    /// Al apoyar el boton: limpia el estado del trazo anterior (solo donde se toco).
    pub fn begin_stroke(&mut self) {
        if let Some((x0, y0, x1, y1)) = self.touched.take() {
            for y in y0..=y1 {
                let a = (y * self.size + x0) as usize;
                let b = (y * self.size + x1) as usize;
                self.stroke_alpha[a..=b].fill(0.0);
                self.stroke_dual[a..=b].fill(0.0);
                self.stroke_depth[a..=b].fill(0.0);
            }
        }
    }

    /// Mezcla un texel del toque actual (seccion 7.1): `cov` es la cobertura del toque en ese texel (0..1, la forma del
    /// pincel) y `opacity` la opacidad del pincel. Por defecto la opacidad del texel es `cov * opacity` con la regla del
    /// maximo por pasada; con `opts.ceiling` sube hacia `opacity` (`prev + cov * (opacity - prev)`) y nunca la pasa.
    /// Aplica tambien el borrador y el bloqueo alfa. Devuelve true si el pixel cambio.
    ///
    /// `sub` es lo que resta el papel o el lienzo de los pinceles de medio seco (modo Subtract de photocraft; 0 para el
    /// resto). Se resta de la cobertura YA acumulada del trazo, no de cada toque: `stroke_alpha` guarda la acumulacion
    /// sin restar y solo lo que se mezcla con el color lleva la resta (`acumulado - sub * opacidad`, que es
    /// `max(0, cobertura - profundidad * t) * opacidad` de photocraft). Asi dos toques sobre el mismo sitio no cambian
    /// el dibujo del papel.
    ///
    /// `hard` es la trama de los puntos de semitono (`None` para el resto): modo HardMix de photocraft con profundidad 1.
    /// Tambien se mezcla con la cobertura YA acumulada (`acumulado / opacidad`, que es la cobertura de photocraft antes
    /// de la opacidad del trazo) y da todo o nada: `opacidad` donde `cobertura + trama > 1` y 0 donde no. Como la
    /// cobertura acumulada solo sube, un texel que no se pinto se pinta en cuanto la cobertura pasa el umbral de su
    /// trama, y uno ya pintado nunca se despinta.
    #[allow(clippy::too_many_arguments)]
    fn apply_texel(
        &mut self,
        pixels: &mut [u8],
        idx: usize,
        cov: f32,
        opacity: f32,
        color: [u8; 3],
        sub: f32,
        mul: f32,
        hard: Option<f32>,
        opts: DabOptions,
    ) -> bool {
        let mut prev = self.stroke_alpha[idx];
        let dab_alpha = if opts.ceiling {
            // Techo: volver a pasar por el mismo sitio no oscurece mas alla de la opacidad (no hay pasada nueva).
            self.stroke_last[idx] = self.stroke_dab;
            if opacity <= prev {
                return false;
            }
            prev + cov * (opacity - prev)
        } else {
            // El trazo se fue y volvio: pasada nueva.
            if prev > 0.0 && self.stroke_dab - self.stroke_last[idx] > PASS_GAP {
                prev = 0.0;
            }
            self.stroke_last[idx] = self.stroke_dab;
            cov * opacity
        };
        if dab_alpha <= prev {
            return false;
        }
        let i = idx * 4;
        if prev == 0.0 {
            self.stroke_base[i..i + 4].copy_from_slice(&pixels[i..i + 4]);
        }
        self.stroke_alpha[idx] = dab_alpha;
        // Papel o lienzo (medio seco): se resta a lo acumulado, solo para mezclar. Donde el papel se come toda la
        // cobertura no se pinta nada.
        // `mul` es el papel de la tiza: multiplica la cobertura acumulada (1 para el resto de pinceles).
        let dab_alpha = match hard {
            // `min(1.0)`: el redondeo no debe dejar la cobertura en 1.0000001 y pintar el centro de un punto.
            Some(t) => hard_mix((dab_alpha / opacity).min(1.0), t) * opacity,
            None => (dab_alpha * mul - sub * opacity).max(0.0),
        };
        if dab_alpha <= 0.0 {
            return false;
        }
        // Siempre se mezcla contra el color de ANTES del trazo.
        let base_a = self.stroke_base[i + 3] as f32 / 255.0;
        if opts.erase {
            // Borrar: alfa = alfaBase * (1 - dabAlpha); el color no cambia. Con bloqueo alfa no se borra
            // (borrar cambia el alfa).
            if opts.alpha_lock {
                return false;
            }
            pixels[i + 3] = (base_a * (1.0 - dab_alpha) * 255.0 + 0.5) as u8;
            return true;
        }
        if opts.alpha_lock {
            // Bloqueo alfa: si no habia nada, no se hace nada; si no, solo cambia el color.
            if self.stroke_base[i + 3] == 0 {
                return false;
            }
            for k in 0..3 {
                let old = self.stroke_base[i + k] as f32;
                let c = old + (color[k] as f32 - old) * dab_alpha;
                pixels[i + k] = c.round().clamp(0.0, 255.0) as u8;
            }
            return true;
        }
        let keep = base_a * (1.0 - dab_alpha);
        let out_a = dab_alpha + keep;
        for k in 0..3 {
            let old = self.stroke_base[i + k] as f32;
            let c = (color[k] as f32 * dab_alpha + old * keep) / out_a;
            pixels[i + k] = c.round().clamp(0.0, 255.0) as u8;
        }
        pixels[i + 3] = (out_a * 255.0 + 0.5).min(255.0) as u8;
        true
    }

    /// Un toque de pincel SOBRE EL MODELO: `center` es el punto tocado (3D), `radius` el radio en unidades del
    /// modelo y `tris` los triangulos cercanos que se pueden pintar. Cada texel se mide por su distancia 3D a `center`
    /// con el `falloff` de `opts.brush` (`t = 1 - d/radio`). Devuelve el rectangulo de los texeles que cambiaron (queda
    /// dentro del de `surface_rect`, que es el que cubre todo lo que PODRIA cambiar), extremos incluidos.
    pub fn dab_surface(
        &mut self,
        pixels: &mut [u8],
        tris: &[SurfTri],
        center: Vec3,
        radius: f32,
        color: [u8; 3],
        opacity: f32,
        opts: DabOptions,
    ) -> Option<Rect> {
        if radius <= 0.0 {
            return None;
        }
        self.stroke_dab += 1;
        // La tiza gira y cambia de tamano en cada toque; se prepara una vez por toque.
        let chalk = (opts.brush == BrushType::Chalk)
            .then(|| ChalkDab::new(self.stroke_dab, opts.normal, radius));
        // Carboncillo y pastel: punta de bitmap (el carboncillo sigue la direccion del trazo, el pastel gira y se
        // dispersa de lado) y papel o lienzo que resta cobertura al trazo acumulado.
        let dry: Option<(DryKind, DryDab)> = opts.brush.dry_kind().map(|k| {
            (
                k,
                DryDab::new(k, self.stroke_dab, opts.normal, opts.dir, radius).with_color(color),
            )
        });
        // Doble grano: el disco ruidoso y las gotas de la segunda punta (que se acumulan en `stroke_dual`).
        let dual = (opts.brush == BrushType::DualGrain)
            .then(|| DualDab::new(self.stroke_dab, opts.normal, radius));
        // La dispersion del pastel desplaza el centro de la punta: los texeles se revisan un poco mas lejos. Las gotas
        // del Doble grano llegan todavia mas lejos (`DualDab::limit`).
        let limit = match &dual {
            Some(d) => d.limit(),
            None => radius + dry.as_ref().map_or(0.0, |(_, d)| d.reach()),
        };
        let max = (self.size - 1) as f32;
        // Preparacion: cada triangulo en texeles, con su caja.
        let mut preps: Vec<Prep> = Vec::with_capacity(tris.len());
        let mut rect: Option<Rect> = None;
        for t in tris {
            let uv = wrapped_uv(&t.uv);
            let Some(bx) = tri_box(self.size, &uv) else {
                continue;
            };
            let a = (uv[0][0] * max, uv[0][1] * max);
            let b = (uv[1][0] * max, uv[1][1] * max);
            let c = (uv[2][0] * max, uv[2][1] * max);
            let det = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
            // Triangulos sin area en la textura (por ejemplo, un OBJ sin UVs) no se pueden pintar.
            if det.abs() < 1e-6 {
                continue;
            }
            let len = |p: (f32, f32), q: (f32, f32)| {
                ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt().max(1e-6)
            };
            let area2 = det.abs();
            let h = [area2 / len(b, c), area2 / len(c, a), area2 / len(a, b)];
            rect = Some(union(rect, bx));
            // Posicion 3D afin en los texeles: p2 + (p0 - p2) * w0 + (p1 - p2) * w1, con w0 y w1 afines en (px, py).
            let (e0, e1) = (t.p[0] - t.p[2], t.p[1] - t.p[2]);
            let (k0x, k0y) = ((b.1 - c.1) / det, (c.0 - b.0) / det);
            let (k1x, k1y) = ((c.1 - a.1) / det, (a.0 - c.0) / det);
            let (gx, gy) = (e0 * k0x + e1 * k1x, e0 * k0y + e1 * k1y);
            preps.push(Prep {
                p: t.p,
                a,
                b,
                c,
                det,
                h,
                bx,
                gx,
                gy,
            });
        }
        let rect = rect?;
        // Rectangulo de los texeles que de verdad cambiaron (el que hay que recomponer y subir a la GPU).
        let mut changed: Option<Rect> = None;
        // Pasada 0: los texeles de dentro de cada triangulo. Pasada 1: el relleno alrededor, solo en los texeles que
        // ningun triangulo cubrio (asi el relleno de una isla no pisa a la vecina).
        // Solo se recorren las columnas de cada fila que caen dentro del radio (`row_span`); fuera de ellas ningun
        // texel pasaria la prueba de distancia, asi que el resultado es el mismo que recorriendo toda la caja.
        for pass in 0..2 {
            for pr in &preps {
                let (_, y0, _, y1) = pr.bx;
                for y in y0..=y1 {
                    let Some((xa, xb)) = row_span(pr, y, center, limit) else {
                        continue;
                    };
                    for x in xa..=xb {
                        let idx = (y * self.size + x) as usize;
                        if pass == 1 && self.dab_mark[idx] == self.stroke_dab {
                            continue;
                        }
                        let (px, py) = (x as f32, y as f32);
                        let (w0, w1, w2) = bary(pr, px, py);
                        let ok = if pass == 0 {
                            w0 >= -INSIDE_EPS && w1 >= -INSIDE_EPS && w2 >= -INSIDE_EPS
                        } else {
                            w0 * pr.h[0] >= -PAD_TEXELS
                                && w1 * pr.h[1] >= -PAD_TEXELS
                                && w2 * pr.h[2] >= -PAD_TEXELS
                        };
                        if !ok {
                            continue;
                        }
                        // En la pasada 1, un texel de dentro de OTRO triangulo ya esta cubierto aunque la pasada 0
                        // no lo marcara (por quedar fuera del radio de ese triangulo): no se rellena.
                        if pass == 1 && preps.iter().any(|q| inside(q, px, py)) {
                            continue;
                        }
                        self.dab_mark[idx] = self.stroke_dab;
                        // En el relleno la posicion se extrapola por el plano del triangulo.
                        let pos = pr.p[0] * w0 + pr.p[1] * w1 + pr.p[2] * w2;
                        let d = pos.distance(center);
                        if d >= limit {
                            continue;
                        }
                        // Color con que se pinta este texel: el del pincel, salvo el follaje (cada hoja tiene el suyo).
                        let mut texel_color = color;
                        let (cov, sub, mul, hard) = match (&chalk, &dry, &dual) {
                            // Tiza: punta de bitmap girada en el plano de la superficie; el papel (anclado al modelo)
                            // multiplica despues, sobre la cobertura acumulada.
                            (Some(c), _, _) => {
                                (c.tip(pos - center), 0.0, paper_mask(pos, x, y), None)
                            }
                            // Medio seco: la punta da la cobertura; el papel o lienzo resta despues, al acumular.
                            (_, Some((kind, dd)), _) => {
                                let (a, c) = dd.alpha_color(pos - center, color);
                                texel_color = c;
                                // Esponja de tinta, bordes mojados (`wet_edges` de photocraft): cada texel recuerda lo
                                // mas hondo que lo alcanzo una punta (su valor en la punta) y la cobertura acumulada se
                                // multiplica por `wet_factor`: la mitad donde la punta pinto del todo, la entera en el
                                // borde. Se aplica al trazo, no a cada toque, asi que pasar dos veces no oscurece mas.
                                let mul = if *kind == DryKind::Sponge && a > 0.0 {
                                    // Una pasada nueva (sin techo, el trazo se fue y volvio) empieza sin profundidad,
                                    // igual que su cobertura (`apply_texel`).
                                    if !opts.ceiling
                                        && self.stroke_alpha[idx] > 0.0
                                        && self.stroke_dab - self.stroke_last[idx] > PASS_GAP
                                    {
                                        self.stroke_depth[idx] = 0.0;
                                    }
                                    let depth = &mut self.stroke_depth[idx];
                                    *depth = depth.max(a);
                                    wet_factor(1.0 - *depth)
                                } else {
                                    1.0
                                };
                                // Puntos de semitono: la trama (modo HardMix) en vez de la resta del papel.
                                // Crayon de cera: su papel (modo HardMix, `each_tip`) se mezcla con la cobertura de ESTE
                                // toque, antes de acumularla (`dry_mezcla_toque`); para los demas pinceles no cambia nada.
                                let a = dry_mezcla_toque(*kind, a, self.stroke_dab, pos, x, y);
                                (
                                    a,
                                    dry_subtract(*kind, pos, x, y),
                                    mul,
                                    dry_hardmix(*kind, x, y),
                                )
                            }
                            // Doble grano: las gotas se acumulan en el trazo (`c + v * (1 - c)`, aunque el disco no
                            // llegue a este texel en este toque) y el disco ruidoso solo pinta donde ya hay gotas
                            // (modo Multiply de photocraft).
                            (_, _, Some(dd)) => {
                                let acc = &mut self.stroke_dual[idx];
                                *acc += dd.dual(pos - center) * (1.0 - *acc);
                                (dd.primary(d, x, y) * *acc, 0.0, 1.0, None)
                            }
                            _ => (opts.brush.falloff(1.0 - d / radius), 0.0, 1.0, None),
                        };
                        let cov = cov * opts.flow;
                        if cov * opacity <= 0.0 {
                            continue;
                        }
                        if self.apply_texel(
                            pixels,
                            idx,
                            cov,
                            opacity,
                            texel_color,
                            sub,
                            mul,
                            hard,
                            opts,
                        ) {
                            changed = Some(union(changed, (x, y, x, y)));
                        }
                    }
                }
            }
        }
        // Para limpiar `stroke_alpha` al acabar el trazo se guarda toda la caja (pudo cambiar aunque no cambie el pixel).
        self.touched = Some(union(self.touched, rect));
        changed
    }

    /// Un toque de pincel en ESPACIO UV (un circulo de `radius` texeles en la imagen, sin mirar el modelo). Ya no lo
    /// usa la app (pinta con `dab_surface`); se conserva con sus pruebas como referencia de la regla de mezcla.
    #[allow(dead_code)]
    pub fn dab(
        &mut self,
        pixels: &mut [u8],
        u: f32,
        v: f32,
        radius: f32,
        color: [u8; 3],
        opacity: f32,
        opts: DabOptions,
    ) -> Option<Rect> {
        let max = (self.size - 1) as f32;
        // La textura se repite fuera de 0..1.
        let (u, v) = (u - u.floor(), v - v.floor());
        let (cx, cy) = (u * max, v * max);
        let (x0, y0, x1, y1) = dab_box(self.size, u, v, radius);
        self.stroke_dab += 1;
        let mut changed = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                if d >= radius {
                    continue;
                }
                let t = 1.0 - d / radius;
                let cov = opts.brush.falloff(t) * opts.flow;
                if cov * opacity <= 0.0 {
                    continue;
                }
                let idx = (y * self.size + x) as usize;
                changed |= self.apply_texel(pixels, idx, cov, opacity, color, 0.0, 1.0, None, opts);
            }
        }
        // El rectangulo del trazo incluye toda la caja (stroke_alpha pudo cambiar aunque no cambie el pixel).
        self.touched = Some(union(self.touched, (x0, y0, x1, y1)));
        changed.then_some((x0, y0, x1, y1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(size: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.iter()
            .copied()
            .cycle()
            .take((size * size * 4) as usize)
            .collect()
    }

    fn px(p: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [p[i], p[i + 1], p[i + 2], p[i + 3]]
    }

    #[test]
    fn el_borrador_baja_el_alfa_en_el_centro_y_no_toca_fuera() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [200, 10, 10, 255]);
        s.begin_stroke();
        // Centro en el texel (32, 32): u = v = 32/63.
        let c = 32.0 / 63.0;
        let r = s.dab(
            &mut pixels,
            c,
            c,
            10.0,
            [0, 0, 0],
            1.0,
            DabOptions {
                erase: true,
                ..Default::default()
            },
        );
        assert!(r.is_some());
        assert_eq!(px(&pixels, 64, 32, 32), [200, 10, 10, 0]);
        // Fuera del radio no cambia nada.
        assert_eq!(px(&pixels, 64, 5, 5), [200, 10, 10, 255]);
    }

    #[test]
    fn el_borrador_no_borra_con_bloqueo_alfa() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [200, 10, 10, 255]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        s.dab(
            &mut pixels,
            c,
            c,
            10.0,
            [0, 0, 0],
            1.0,
            DabOptions {
                alpha_lock: true,
                erase: true,
                ..Default::default()
            },
        );
        assert_eq!(px(&pixels, 64, 32, 32), [200, 10, 10, 255]);
    }

    #[test]
    fn la_opacidad_no_se_acumula_dentro_de_un_trazo() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        let opts = DabOptions::default();
        // Varios toques seguidos en el mismo punto con opacidad 0.5: el alfa se queda en ~0.5, no sube.
        for _ in 0..5 {
            s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        }
        let a = px(&pixels, 64, 32, 32)[3];
        assert!((a as i32 - 128).abs() <= 1, "alfa = {a}");
    }

    #[test]
    fn un_trazo_nuevo_si_suma_sobre_el_anterior() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        let c = 32.0 / 63.0;
        let opts = DabOptions::default();
        s.begin_stroke();
        s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        s.begin_stroke();
        s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        // 0.5 + 0.5 * (1 - 0.5) = 0.75.
        let a = px(&pixels, 64, 32, 32)[3];
        assert!((a as i32 - 191).abs() <= 1, "alfa = {a}");
    }

    /// Cuadrado de 2x2 unidades en el plano z = 0, cortado en dos triangulos. En la textura de 64 ocupa 0.25 de ancho
    /// y 0.5 de alto: el UV esta estirado (2 veces mas texeles por unidad en vertical que en horizontal).
    fn cuadrado() -> [SurfTri; 2] {
        let p = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let uv = [[0.25, 0.25], [0.5, 0.25], [0.5, 0.75], [0.25, 0.75]];
        [
            SurfTri {
                p: [p[0], p[1], p[2]],
                uv: [uv[0], uv[1], uv[2]],
            },
            SurfTri {
                p: [p[0], p[2], p[3]],
                uv: [uv[0], uv[2], uv[3]],
            },
        ]
    }

    #[test]
    fn el_pincel_mide_en_el_modelo_y_no_en_la_textura() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let (center, radius) = (Vec3::new(1.0, 1.0, 0.0), 0.5);
        let tris = cuadrado();
        assert!(
            s.dab_surface(
                &mut pixels,
                &tris,
                center,
                radius,
                [255, 0, 0],
                1.0,
                DabOptions::default()
            )
            .is_some()
        );
        let (mut pintados, mut revisados) = (0, 0);
        for ty in 0..64u32 {
            for tx in 0..64u32 {
                // Posicion 3D de este texel dentro del cuadrado.
                let x = (tx as f32 / 63.0 - 0.25) / 0.25 * 2.0;
                let y = (ty as f32 / 63.0 - 0.25) / 0.5 * 2.0;
                if !(0.0..=2.0).contains(&x) || !(0.0..=2.0).contains(&y) {
                    continue;
                }
                let d = ((x - 1.0).powi(2) + (y - 1.0).powi(2)).sqrt();
                // Los que estan casi justo en el borde del circulo no se comprueban (redondeos).
                if (d - radius).abs() < 0.06 {
                    continue;
                }
                let a = px(&pixels, 64, tx, ty)[3];
                assert_eq!(
                    a > 0,
                    d < radius,
                    "texel ({tx},{ty}) a distancia {d}: alfa {a}"
                );
                revisados += 1;
                if a > 0 {
                    pintados += 1;
                }
            }
        }
        // La prueba cubre texeles pintados y sin pintar.
        assert!(
            pintados > 20 && revisados > pintados + 20,
            "pintados {pintados} de {revisados}"
        );
    }

    #[test]
    fn la_isla_se_rellena_unos_texeles_mas_alla_de_su_borde() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        // Toque pegado al borde izquierdo del cuadrado (x = 0 esta en el texel 15.75).
        let r = s.dab_surface(
            &mut pixels,
            &cuadrado(),
            Vec3::new(0.1, 1.0, 0.0),
            0.5,
            [255, 0, 0],
            1.0,
            DabOptions::default(),
        );
        assert!(r.is_some());
        // El texel 15 queda fuera de la isla por 0.75 texeles: se rellena. El 10 queda lejos: no.
        assert!(px(&pixels, 64, 15, 31)[3] > 0);
        assert_eq!(px(&pixels, 64, 10, 31)[3], 0);
    }

    #[test]
    fn el_rectangulo_que_devuelve_el_toque_es_el_de_lo_que_cambio_y_cabe_en_el_previo() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let tris = cuadrado();
        let previo = surface_rect(64, &tris).unwrap();
        let r = s
            .dab_surface(
                &mut pixels,
                &tris,
                Vec3::new(1.0, 1.0, 0.0),
                0.5,
                [255, 0, 0],
                1.0,
                DabOptions::default(),
            )
            .unwrap();
        // Cabe en el rectangulo previo (el de las baldosas de deshacer) y es mas chico (el pincel no cubre toda la caja).
        assert!(
            r.0 >= previo.0 && r.1 >= previo.1 && r.2 <= previo.2 && r.3 <= previo.3,
            "{r:?} no cabe en {previo:?}"
        );
        assert!(
            (r.2 - r.0) < (previo.2 - previo.0) || (r.3 - r.1) < (previo.3 - previo.1),
            "{r:?} = {previo:?}"
        );
        // Todo lo que se pinto esta dentro del rectangulo devuelto (es lo que se recompone y sube a la GPU).
        for y in 0..64u32 {
            for x in 0..64u32 {
                if px(&pixels, 64, x, y)[3] > 0 {
                    assert!(
                        x >= r.0 && x <= r.2 && y >= r.1 && y <= r.3,
                        "texel ({x},{y}) fuera de {r:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn un_triangulo_enorme_se_pinta_solo_donde_llega_el_pincel() {
        // Un triangulo que ocupa casi toda la textura de 1024: el toque de radio 0.1 solo toca una zona chica.
        let p = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let tri = SurfTri {
            p,
            uv: [[0.05, 0.05], [0.95, 0.05], [0.05, 0.95]],
        };
        let mut s = StrokeState::new(TEX_SIZE);
        let mut pixels = layer(TEX_SIZE, [0, 0, 0, 0]);
        s.begin_stroke();
        let r = s
            .dab_surface(
                &mut pixels,
                &[tri],
                Vec3::new(0.5, 0.5, 0.0),
                0.1,
                [255, 0, 0],
                1.0,
                DabOptions::default(),
            )
            .unwrap();
        let (w, h) = (r.2 - r.0 + 1, r.3 - r.1 + 1);
        // El radio 0.1 de 2 unidades son ~46 texeles: la zona cambiada mide ~92 de lado, no ~900.
        assert!(w < 120 && h < 120, "zona de {w}x{h}");
        // El centro (0.5, 0.5) cae en el texel ~(0.05 + 0.9 * 0.25) * 1023 = 281.
        assert_eq!(px(&pixels, TEX_SIZE, 281, 281)[3], 255);
        assert_eq!(px(&pixels, TEX_SIZE, 600, 100)[3], 0);
    }

    #[test]
    fn un_trazo_cruza_la_costura_entre_dos_islas() {
        // El mismo cuadrado, pero el segundo triangulo esta en otra isla de la textura (desplazada a la izquierda).
        let mut tris = cuadrado();
        for uv in tris[1].uv.iter_mut() {
            uv[0] -= 0.25;
        }
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        s.dab_surface(
            &mut pixels,
            &tris,
            Vec3::new(1.0, 1.0, 0.0),
            0.6,
            [255, 0, 0],
            1.0,
            DabOptions::default(),
        );
        let pintado = |desde: u32, hasta: u32| {
            (0..64u32).any(|ty| (desde..hasta).any(|tx| px(&pixels, 64, tx, ty)[3] > 0))
        };
        // Hay pintura en la isla de la derecha (texeles 16 en adelante) y tambien en la de la izquierda (0 a 12).
        assert!(pintado(16, 64));
        assert!(pintado(0, 13));
    }

    #[test]
    fn el_borrador_funciona_en_la_superficie_y_respeta_el_bloqueo_alfa() {
        let tris = cuadrado();
        let opts = DabOptions {
            erase: true,
            ..Default::default()
        };
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [200, 10, 10, 255]);
        s.begin_stroke();
        s.dab_surface(
            &mut pixels,
            &tris,
            Vec3::new(1.0, 1.0, 0.0),
            0.5,
            [0, 0, 0],
            1.0,
            opts,
        );
        // El texel del centro (23.6, 31.5) queda borrado.
        assert_eq!(px(&pixels, 64, 24, 31)[3], 0);
        // Con bloqueo alfa no se borra nada.
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [200, 10, 10, 255]);
        s.begin_stroke();
        s.dab_surface(
            &mut pixels,
            &tris,
            Vec3::new(1.0, 1.0, 0.0),
            0.5,
            [0, 0, 0],
            1.0,
            DabOptions {
                alpha_lock: true,
                ..opts
            },
        );
        assert_eq!(px(&pixels, 64, 24, 31)[3], 255);
    }

    #[test]
    fn la_tiza_pinta_con_grano_y_solo_dentro_del_radio() {
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Chalk,
            normal: Vec3::Z,
            ..Default::default()
        };
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let (center, radius) = (Vec3::new(1.0, 1.0, 0.0), 0.8);
        assert!(
            s.dab_surface(
                &mut pixels,
                &tris,
                center,
                radius,
                [255, 255, 255],
                1.0,
                opts
            )
            .is_some()
        );
        let (mut alfas, mut lejos) = (Vec::new(), 0);
        for ty in 0..64u32 {
            for tx in 0..64u32 {
                let x = (tx as f32 / 63.0 - 0.25) / 0.25 * 2.0;
                let y = (ty as f32 / 63.0 - 0.25) / 0.5 * 2.0;
                if !(0.0..=2.0).contains(&x) || !(0.0..=2.0).contains(&y) {
                    continue;
                }
                let d = ((x - 1.0).powi(2) + (y - 1.0).powi(2)).sqrt();
                let a = px(&pixels, 64, tx, ty)[3];
                if a > 0 {
                    alfas.push(a);
                }
                // Mas alla del radio no se pinta nada.
                if d > radius + 0.06 && a > 0 {
                    lejos += 1;
                }
            }
        }
        assert_eq!(lejos, 0);
        assert!(alfas.len() > 20, "pinto pocos texeles: {}", alfas.len());
        // Tiene grano: la opacidad no es pareja (el pincel suave a opacidad 1 llega a 255 en casi todo el centro).
        let (lo, hi) = (*alfas.iter().min().unwrap(), *alfas.iter().max().unwrap());
        assert!(hi - lo > 40, "sin grano: de {lo} a {hi}");
    }

    #[test]
    fn la_tiza_con_el_mismo_toque_da_el_mismo_resultado() {
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Chalk,
            normal: Vec3::Z,
            ..Default::default()
        };
        let pintar = || {
            let mut s = StrokeState::new(64);
            let mut pixels = layer(64, [0, 0, 0, 0]);
            s.begin_stroke();
            s.dab_surface(
                &mut pixels,
                &tris,
                Vec3::new(1.0, 1.0, 0.0),
                0.8,
                [255, 0, 0],
                1.0,
                opts,
            );
            pixels
        };
        assert_eq!(pintar(), pintar());
    }

    #[test]
    fn el_punteado_pinta_gotas_sueltas_y_no_pasa_de_su_alcance() {
        use crate::pinceles::STIPPLE_REACH;
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Stipple,
            normal: Vec3::Z,
            ..Default::default()
        };
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let (center, radius) = (Vec3::new(1.0, 1.0, 0.0), 0.5);
        assert!(
            s.dab_surface(
                &mut pixels,
                &tris,
                center,
                radius,
                [255, 255, 255],
                1.0,
                opts
            )
            .is_some()
        );
        // Lo mas lejos que puede llegar un toque: su radio mas lo que se desplaza su centro.
        let alcance = radius * (1.0 + STIPPLE_REACH);
        let (mut pintados, mut en_zona, mut lejos) = (0, 0, 0);
        for ty in 0..64u32 {
            for tx in 0..64u32 {
                let x = (tx as f32 / 63.0 - 0.25) / 0.25 * 2.0;
                let y = (ty as f32 / 63.0 - 0.25) / 0.5 * 2.0;
                if !(0.0..=2.0).contains(&x) || !(0.0..=2.0).contains(&y) {
                    continue;
                }
                let d = ((x - 1.0).powi(2) + (y - 1.0).powi(2)).sqrt();
                let a = px(&pixels, 64, tx, ty)[3];
                if d < alcance - 0.06 {
                    en_zona += 1;
                }
                if a > 0 {
                    pintados += 1;
                    if d > alcance + 0.06 {
                        lejos += 1;
                    }
                }
            }
        }
        assert_eq!(lejos, 0, "pinto mas alla del alcance");
        assert!(pintados > 5, "pinto pocos texeles: {pintados}");
        // Son gotas: quedan huecos sin pintar dentro de la zona (un disco sin huecos pintaria todo).
        assert!(
            pintados < en_zona,
            "pintados {pintados} de {en_zona}: sin huecos"
        );
    }

    #[test]
    fn el_punteado_con_el_mismo_toque_da_el_mismo_resultado() {
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Stipple,
            normal: Vec3::Z,
            ..Default::default()
        };
        let pintar = || {
            let mut s = StrokeState::new(64);
            let mut pixels = layer(64, [0, 0, 0, 0]);
            s.begin_stroke();
            s.dab_surface(
                &mut pixels,
                &tris,
                Vec3::new(1.0, 1.0, 0.0),
                0.5,
                [255, 0, 0],
                1.0,
                opts,
            );
            pixels
        };
        assert_eq!(pintar(), pintar());
    }

    #[test]
    fn la_esponja_empieza_de_cero_su_profundidad_en_una_pasada_nueva() {
        let tris = cuadrado();
        // Sin techo: un hueco de mas de 3 toques empieza una pasada nueva.
        let opts = DabOptions {
            brush: BrushType::InkSponge,
            normal: Vec3::Z,
            ..Default::default()
        };
        let (centro, lejos, radio) = (Vec3::new(1.0, 1.0, 0.0), Vec3::new(0.3, 0.3, 0.0), 0.5);
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        s.dab_surface(&mut pixels, &tris, centro, radio, [255, 0, 0], 1.0, opts);
        let antes = s.stroke_depth.clone();
        assert!(
            antes.iter().any(|d| *d > 0.9),
            "la primera pasada no dejo profundidad"
        );
        for _ in 0..5 {
            s.dab_surface(&mut pixels, &tris, lejos, 0.1, [255, 0, 0], 1.0, opts);
        }
        s.dab_surface(&mut pixels, &tris, centro, radio, [255, 0, 0], 1.0, opts);
        // Cada toque gira distinto: si la profundidad no se reiniciara, solo podria subir (es el maximo de las puntas).
        // Al reiniciarse, donde la segunda punta toca menos que la primera la profundidad baja.
        let baja = (0..antes.len()).any(|i| {
            antes[i] > 0.9 && s.stroke_depth[i] > 0.0 && s.stroke_depth[i] < antes[i] - 0.2
        });
        assert!(baja, "la profundidad no se reinicio en la pasada nueva");
    }

    /// Texel (37, 32) con el aerografo (`t * t`) y radio 10 desde el centro (32, 32): `t` = 0.5, cobertura 0.25.
    fn alfa_tras(n: usize, ceiling: bool) -> u8 {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        let opts = DabOptions {
            brush: BrushType::Airbrush,
            ceiling,
            ..Default::default()
        };
        for _ in 0..n {
            s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 1.0, opts);
        }
        px(&pixels, 64, 37, 32)[3]
    }

    #[test]
    fn sin_techo_repetir_el_toque_no_oscurece() {
        // La regla de siempre: el maximo de los toques.
        let uno = alfa_tras(1, false);
        assert!((uno as i32 - 64).abs() <= 1, "alfa = {uno}");
        assert_eq!(alfa_tras(10, false), uno);
    }

    #[test]
    fn con_techo_la_cobertura_sube_hacia_la_opacidad_sin_pasarla() {
        // c + v * (1 - c) con v = 0.25: 0.25, 0.4375, ...
        let (a1, a2, a30) = (alfa_tras(1, true), alfa_tras(2, true), alfa_tras(30, true));
        assert!((a1 as i32 - 64).abs() <= 1, "alfa = {a1}");
        assert!((a2 as i32 - 112).abs() <= 1, "alfa = {a2}");
        assert!(a30 >= 250, "alfa = {a30}");
    }

    #[test]
    fn con_techo_el_trazo_no_pasa_de_la_opacidad_del_pincel() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        let opts = DabOptions {
            ceiling: true,
            ..Default::default()
        };
        for _ in 0..8 {
            s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        }
        let a = px(&pixels, 64, 32, 32)[3];
        assert!((a as i32 - 128).abs() <= 1, "alfa = {a}");
    }

    /// Como `alfa_tras`, pero con el flujo del pincel.
    fn alfa_con_flujo(n: usize, flow: f32) -> u8 {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        let c = 32.0 / 63.0;
        let opts = DabOptions {
            brush: BrushType::Airbrush,
            ceiling: true,
            flow,
            ..Default::default()
        };
        for _ in 0..n {
            s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 1.0, opts);
        }
        px(&pixels, 64, 37, 32)[3]
    }

    #[test]
    fn el_flujo_inicial_no_cambia_nada() {
        assert_eq!(DabOptions::default().flow, 1.0);
        assert_eq!(alfa_con_flujo(1, 1.0), alfa_tras(1, true));
        assert_eq!(alfa_con_flujo(2, 1.0), alfa_tras(2, true));
    }

    #[test]
    fn con_flujo_bajo_cada_toque_suma_poco_y_se_acerca_al_techo() {
        // Cobertura del texel = 0.25 (aerografo) * 0.5 (flujo) = 0.125; luego c + 0.125 * (1 - c) = 0.234...
        let (a1, a2, a40) = (
            alfa_con_flujo(1, 0.5),
            alfa_con_flujo(2, 0.5),
            alfa_con_flujo(40, 0.5),
        );
        assert!((a1 as i32 - 32).abs() <= 1, "alfa = {a1}");
        assert!((a2 as i32 - 60).abs() <= 1, "alfa = {a2}");
        // Sigue subiendo hacia la opacidad (1.0) sin pasarla.
        assert!(a40 > a2 && a40 >= 250, "alfa = {a40}");
    }

    #[test]
    fn con_techo_un_trazo_nuevo_si_suma_sobre_el_anterior() {
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        let c = 32.0 / 63.0;
        let opts = DabOptions {
            ceiling: true,
            ..Default::default()
        };
        s.begin_stroke();
        s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        s.begin_stroke();
        s.dab(&mut pixels, c, c, 10.0, [255, 0, 0], 0.5, opts);
        let a = px(&pixels, 64, 32, 32)[3];
        assert!((a as i32 - 191).abs() <= 1, "alfa = {a}");
    }

    #[test]
    fn volver_tras_un_hueco_suma_sin_techo_y_no_suma_con_techo() {
        let pintar = |ceiling: bool| {
            let mut s = StrokeState::new(64);
            let mut pixels = layer(64, [0, 0, 0, 0]);
            s.begin_stroke();
            let (centro, lejos) = (32.0 / 63.0, 10.0 / 63.0);
            let opts = DabOptions {
                ceiling,
                ..Default::default()
            };
            s.dab(&mut pixels, centro, centro, 10.0, [255, 0, 0], 0.5, opts);
            // Un hueco de mas de 3 toques en otro sitio.
            for _ in 0..5 {
                s.dab(&mut pixels, lejos, lejos, 10.0, [255, 0, 0], 0.5, opts);
            }
            s.dab(&mut pixels, centro, centro, 10.0, [255, 0, 0], 0.5, opts);
            px(&pixels, 64, 32, 32)[3]
        };
        // Sin techo la segunda pasada suma: 0.5 + 0.5 * (1 - 0.5) = 0.75.
        assert!((pintar(false) as i32 - 191).abs() <= 1);
        // Con techo se queda en la opacidad.
        assert!((pintar(true) as i32 - 128).abs() <= 1);
    }

    /// Mide el coste de un toque (pintar + componer) sobre modelos de prueba, con la textura de tamano real. No es una
    /// prueba de correccion: se corre a mano con
    /// `cargo test --release medir_ -- --ignored --nocapture` (o sin `--release` para ver el perfil de `cargo run`).
    fn medir(
        nombre: &str,
        centros: &[Vec3],
        radio: f32,
        tris_de: &dyn Fn(Vec3, f32) -> Vec<SurfTri>,
    ) {
        use crate::layers::LayerStack;
        let mut stack = LayerStack::new(TEX_SIZE);
        let mut s = StrokeState::new(TEX_SIZE);
        s.begin_stroke();
        let (mut t_pintar, mut t_componer, mut area) = (0u128, 0u128, 0u64);
        for &c in centros {
            let tris = tris_de(c, radio);
            let a = std::time::Instant::now();
            let r = s.dab_surface(
                &mut stack.layers[0].pixels,
                &tris,
                c,
                radio,
                [255, 0, 0],
                1.0,
                DabOptions::default(),
            );
            t_pintar += a.elapsed().as_micros();
            let b = std::time::Instant::now();
            if let Some(r) = r {
                area += u64::from(r.2 - r.0 + 1) * u64::from(r.3 - r.1 + 1);
                stack.composite_rect(r);
            }
            t_componer += b.elapsed().as_micros();
        }
        let n = centros.len() as u128;
        eprintln!(
            "{nombre}: {} us/toque pintando + {} us/toque componiendo ({} texeles recompuestos por toque)",
            t_pintar / n,
            t_componer / n,
            area / n as u64
        );
    }

    #[test]
    #[ignore]
    fn medir_toque_en_esfera() {
        use crate::bvh::TriBvh;
        use crate::obj::ObjMesh;
        let mut mesh = ObjMesh::uv_sphere(48, 24);
        for p in mesh.positions.iter_mut() {
            *p *= 4.5;
        }
        let bvh = TriBvh::new(&mesh);
        let radio = 18.0 * 4.5 / 450.0;
        // Un arco de 150 toques separados un cuarto de radio.
        let paso = radio * 0.25 / 2.25;
        let centros: Vec<Vec3> = (0..150)
            .map(|i| 2.25 * Vec3::new((i as f32 * paso).sin(), -(i as f32 * paso).cos(), 0.0))
            .collect();
        medir("esfera 48x24", &centros, radio, &|c, r| {
            let mut near = Vec::new();
            bvh.query_sphere(c, r, &mut near);
            near.iter()
                .map(|&t| {
                    let (p, uv) = bvh.tri(t);
                    SurfTri { p: *p, uv: *uv }
                })
                .collect()
        });
    }

    #[test]
    #[ignore]
    fn medir_toque_en_triangulos_grandes() {
        // Un cuadrado de 2x2 que ocupa casi toda la textura: sus dos triangulos tienen cajas de ~0.4 millones de texeles.
        let p = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(2.0, 2.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let uv = [[0.05, 0.05], [0.95, 0.05], [0.95, 0.95], [0.05, 0.95]];
        let tris = vec![
            SurfTri {
                p: [p[0], p[1], p[2]],
                uv: [uv[0], uv[1], uv[2]],
            },
            SurfTri {
                p: [p[0], p[2], p[3]],
                uv: [uv[0], uv[2], uv[3]],
            },
        ];
        let centros: Vec<Vec3> = (0..60)
            .map(|i| Vec3::new(0.5 + i as f32 * 0.02, 1.0, 0.0))
            .collect();
        medir("cuadrado grande", &centros, 0.1, &|_, _| tris.clone());
    }

    #[test]
    fn con_techo_la_tiza_conserva_el_grano_del_papel_aunque_se_acumulen_toques() {
        let tris = cuadrado();
        let opts = DabOptions {
            brush: BrushType::Chalk,
            normal: Vec3::Z,
            ceiling: true,
            ..Default::default()
        };
        let mut s = StrokeState::new(64);
        let mut pixels = layer(64, [0, 0, 0, 0]);
        s.begin_stroke();
        // Muchos toques casi en el mismo sitio: la cobertura acumulada llega cerca de 1 en el centro.
        for i in 0..30 {
            let c = Vec3::new(1.0 + (i % 3) as f32 * 0.02, 1.0, 0.0);
            s.dab_surface(&mut pixels, &tris, c, 0.8, [255, 255, 255], 1.0, opts);
        }
        let mut alfas = Vec::new();
        for ty in 0..64u32 {
            for tx in 0..64u32 {
                let x = (tx as f32 / 63.0 - 0.25) / 0.25 * 2.0;
                let y = (ty as f32 / 63.0 - 0.25) / 0.5 * 2.0;
                if ((x - 1.0).powi(2) + (y - 1.0).powi(2)).sqrt() < 0.3 {
                    alfas.push(px(&pixels, 64, tx, ty)[3]);
                }
            }
        }
        let (lo, hi) = (*alfas.iter().min().unwrap(), *alfas.iter().max().unwrap());
        assert!(hi - lo > 40, "sin papel tras acumular: de {lo} a {hi}");
    }

    #[test]
    fn con_techo_la_tiza_no_pasa_de_la_opacidad_y_se_va_rellenando() {
        let tris = cuadrado();
        let (center, radius, opacity) = (Vec3::new(1.0, 1.0, 0.0), 0.8, 0.6);
        let pintar = |toques: usize| {
            let opts = DabOptions {
                brush: BrushType::Chalk,
                normal: Vec3::Z,
                ceiling: true,
                ..Default::default()
            };
            let mut s = StrokeState::new(64);
            let mut pixels = layer(64, [0, 0, 0, 0]);
            s.begin_stroke();
            for _ in 0..toques {
                s.dab_surface(
                    &mut pixels,
                    &tris,
                    center,
                    radius,
                    [255, 255, 255],
                    opacity,
                    opts,
                );
            }
            let alfas: Vec<u32> = pixels.chunks(4).map(|p| p[3] as u32).collect();
            (alfas.iter().sum::<u32>(), *alfas.iter().max().unwrap())
        };
        let (uno, _) = pintar(1);
        let (diez, maximo) = pintar(10);
        // Cada toque gira distinto y rellena huecos del grano: hay mas pintura.
        assert!(diez > uno, "{diez} no pasa de {uno}");
        // Pero nunca mas que la opacidad del pincel (0.6 * 255 = 153).
        assert!(maximo <= 154, "maximo {maximo}");
    }
}
