//! Camara orbital con Z arriba (estilo Blender). Portada de `MyGLRenderer.kt` (ver ESPECIFICACION.md, seccion 14.1).
//!
//! La camara no se mueve alrededor del modelo: el ojo queda fijo en `(pan_x, -distancia, pan_z)` mirando a
//! `(pan_x, 0, pan_z)` y es el MUNDO el que gira (`rotacion = Rx(angle_x) * Rz(angle_y)`). Por eso la luz, que esta
//! fija respecto a la camara, no se mueve al girar el modelo.
//!
//! glam 0.34 quito `Mat4::look_at_rh`/`perspective_rh`/`orthographic_rh`: ahora estan en `glam::camera::rh`.
//! Se usa la proyeccion `directx` (profundidad 0..1, Y arriba), que es la que espera wgpu.

use std::time::{Duration, Instant};

use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3};

/// Uno de los seis ejes del gizmo de navegacion. Al hacer clic en uno la camara pasa a la vista que mira ese eje
/// de frente (como los atajos de numpad de Blender).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewAxis {
    PosX,
    NegX,
    PosY,
    NegY,
    PosZ,
    NegZ,
}

impl ViewAxis {
    pub const ALL: [ViewAxis; 6] = [
        ViewAxis::PosX,
        ViewAxis::NegX,
        ViewAxis::PosY,
        ViewAxis::NegY,
        ViewAxis::PosZ,
        ViewAxis::NegZ,
    ];

    /// Direccion del eje en el mundo.
    pub fn dir(self) -> Vec3 {
        match self {
            ViewAxis::PosX => Vec3::X,
            ViewAxis::NegX => Vec3::NEG_X,
            ViewAxis::PosY => Vec3::Y,
            ViewAxis::NegY => Vec3::NEG_Y,
            ViewAxis::PosZ => Vec3::Z,
            ViewAxis::NegZ => Vec3::NEG_Z,
        }
    }

    pub fn is_positive(self) -> bool {
        matches!(self, ViewAxis::PosX | ViewAxis::PosY | ViewAxis::PosZ)
    }

    pub fn opposite(self) -> ViewAxis {
        match self {
            ViewAxis::PosX => ViewAxis::NegX,
            ViewAxis::NegX => ViewAxis::PosX,
            ViewAxis::PosY => ViewAxis::NegY,
            ViewAxis::NegY => ViewAxis::PosY,
            ViewAxis::PosZ => ViewAxis::NegZ,
            ViewAxis::NegZ => ViewAxis::PosZ,
        }
    }

    /// `(angle_x, angle_y)` en grados de la vista que tiene ese eje de cara al ojo. Con `rotacion = Rx * Rz` y el ojo
    /// en -Y: +X -> (0, -90) (derecha), -Y -> (0, 0) (frontal), +Y -> (0, 180) (trasera), +Z -> (90, 0) (superior).
    fn view_angles(self) -> (f32, f32) {
        match self {
            ViewAxis::PosX => (0.0, -90.0),
            ViewAxis::NegX => (0.0, 90.0),
            ViewAxis::NegY => (0.0, 0.0),
            ViewAxis::PosY => (0.0, 180.0),
            ViewAxis::PosZ => (90.0, 0.0),
            ViewAxis::NegZ => (-90.0, 0.0),
        }
    }
}

/// Lleva un angulo en grados al rango -180..180 (para girar siempre por el camino mas corto).
fn wrap_deg(d: f32) -> f32 {
    (d + 180.0).rem_euclid(360.0) - 180.0
}

/// Duracion de la animacion al pulsar un eje del gizmo (seccion 14.1: 220 ms, con desaceleracion).
const VIEW_ANIM_TIME: Duration = Duration::from_millis(220);

/// Giro animado de la vista: de `from` a `from + delta` (grados) desde el instante `start`.
#[derive(Clone, Copy)]
struct ViewAnim {
    from: (f32, f32),
    delta: (f32, f32),
    start: Instant,
}

pub struct Camera {
    /// Inclinacion en grados (rota sobre X mundo).
    pub angle_x: f32,
    /// Giro en grados (rota sobre Z mundo).
    pub angle_y: f32,
    /// Distancia del ojo al origen (zoom), limitada a 0.5..20.
    distance: f32,
    pub pan_x: f32,
    pub pan_z: f32,
    /// true = ortografica (vistas de eje); false = perspectiva.
    pub orthographic: bool,
    /// Giro animado en curso (clic en un eje del gizmo).
    anim: Option<ViewAnim>,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            angle_x: 19.8,
            angle_y: -137.0,
            distance: 7.47,
            pan_x: 0.05,
            pan_z: 0.24,
            orthographic: false,
            anim: None,
        }
    }
}

const NEAR: f32 = 0.1;
const FAR: f32 = 30.0;

impl Camera {
    pub fn distance(&self) -> f32 {
        self.distance
    }

    pub fn set_distance(&mut self, d: f32) {
        self.distance = d.clamp(0.5, 20.0);
    }

    /// Un paso de zoom (rueda del raton): `steps > 0` acerca.
    pub fn zoom(&mut self, steps: f32) {
        self.set_distance(self.distance * 0.85_f32.powf(steps));
    }

    /// Gira `dx`, `dy` pixeles de pantalla: 0.5 grados por pixel. Girar a mano vuelve a perspectiva y corta
    /// cualquier giro animado.
    pub fn rotate_by(&mut self, dx: f32, dy: f32) {
        self.anim = None;
        self.angle_y += dx * 0.5;
        self.angle_x += dy * 0.5;
        self.orthographic = false;
    }

    /// Desplaza la vista `dx`, `dy` pixeles de pantalla (la escala depende del zoom para que siga al cursor).
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        let scale = 0.01 * (self.distance / 6.5);
        self.pan_x -= dx * scale;
        self.pan_z += dy * scale;
    }

    /// Corta el giro animado en curso (si lo hay) y deja los angulos donde esten.
    pub fn stop_animation(&mut self) {
        self.anim = None;
    }

    /// Direccion de un eje del mundo en espacio de vista: +X derecha, +Y arriba, +Z hacia quien mira.
    /// Es lo que necesita el gizmo para saber donde dibujar cada eje y cual queda delante.
    pub fn axis_in_view(&self, axis: Vec3) -> Vec3 {
        (self.view() * self.rotation()).transform_vector3(axis)
    }

    /// Pasa a la vista ortografica que mira `axis` de frente, con un giro animado por el camino angular mas corto.
    /// Si ya se esta mirando ese eje, va a la vista opuesta (como Blender).
    pub fn snap_to(&mut self, axis: ViewAxis) {
        let (mut tx, mut ty) = axis.view_angles();
        let already = self.orthographic
            && wrap_deg(tx - self.angle_x).abs() < 0.5
            && wrap_deg(ty - self.angle_y).abs() < 0.5;
        if already {
            (tx, ty) = axis.opposite().view_angles();
        }
        self.anim = Some(ViewAnim {
            from: (self.angle_x, self.angle_y),
            delta: (wrap_deg(tx - self.angle_x), wrap_deg(ty - self.angle_y)),
            start: Instant::now(),
        });
        self.orthographic = true;
    }

    /// Avanza el giro animado. Devuelve true mientras siga en marcha (hay que pedir otro cuadro).
    pub fn update_animation(&mut self) -> bool {
        let Some(a) = self.anim else { return false };
        let t = (a.start.elapsed().as_secs_f32() / VIEW_ANIM_TIME.as_secs_f32()).min(1.0);
        // Desaceleracion: sale rapido y frena al llegar.
        let e = 1.0 - (1.0 - t).powi(3);
        self.angle_x = a.from.0 + a.delta.0 * e;
        self.angle_y = a.from.1 + a.delta.1 * e;
        if t >= 1.0 {
            self.anim = None;
            false
        } else {
            true
        }
    }

    fn view(&self) -> Mat4 {
        view::look_at_mat4(
            Vec3::new(self.pan_x, -self.distance, self.pan_z),
            Vec3::new(self.pan_x, 0.0, self.pan_z),
            Vec3::Z,
        )
    }

    fn rotation(&self) -> Mat4 {
        Mat4::from_rotation_x(self.angle_x.to_radians())
            * Mat4::from_rotation_z(self.angle_y.to_radians())
    }

    fn projection(&self, aspect: f32) -> Mat4 {
        if self.orthographic {
            // Mide lo mismo que la perspectiva a esta distancia (distancia * tan(mitad del FOV) = distancia * 0.5).
            let s = self.distance * 0.5;
            directx::orthographic(-s * aspect, s * aspect, -s, s, NEAR, FAR)
        } else {
            // En la app Android el frustum tiene mitad de altura = near * 0.5, o sea tan(FOV/2) = 0.5.
            let fov_y = 2.0 * 0.5_f32.atan();
            directx::perspective(fov_y, aspect, NEAR, FAR)
        }
    }

    /// Rayo del punto de pantalla `(px, py)` (pixeles, origen arriba a la izquierda) en un viewport `w` x `h`.
    /// Devuelve `(origen, direccion)` en el espacio local del modelo; la direccion no esta normalizada.
    /// Se invierte la MVP y se pasan los puntos de profundidad 0 (cerca) y 1 (lejos), que es el rango de wgpu
    /// (en la app Android, con OpenGL, eran -1 y +1).
    pub fn ray(&self, px: f32, py: f32, w: f32, h: f32) -> (Vec3, Vec3) {
        let ndc_x = 2.0 * px / w - 1.0;
        let ndc_y = 1.0 - 2.0 * py / h;
        let inv = self.matrices(w / h).0.inverse();
        let near = inv.project_point3(Vec3::new(ndc_x, ndc_y, 0.0));
        let far = inv.project_point3(Vec3::new(ndc_x, ndc_y, 1.0));
        (near, far - near)
    }

    /// (matriz MVP, matriz de normales en espacio de vista) del modelo en el origen.
    /// La de normales es `vista * rotacion`: lleva las normales a espacio de vista (+X derecha, +Y arriba, +Z hacia quien mira).
    pub fn matrices(&self, aspect: f32) -> (Mat4, Mat4) {
        let view_rot = self.view() * self.rotation();
        (self.projection(aspect) * view_rot, view_rot)
    }
}
