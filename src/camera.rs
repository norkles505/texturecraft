//! Camara orbital con Z arriba (estilo Blender). Portada de `MyGLRenderer.kt` (ver ESPECIFICACION.md, seccion 14.1).
//!
//! La camara no se mueve alrededor del modelo: el ojo queda fijo en `(pan_x, -distancia, pan_z)` mirando a
//! `(pan_x, 0, pan_z)` y es el MUNDO el que gira (`rotacion = Rx(angle_x) * Rz(angle_y)`). Por eso la luz, que esta
//! fija respecto a la camara, no se mueve al girar el modelo.
//!
//! glam 0.34 quito `Mat4::look_at_rh`/`perspective_rh`/`orthographic_rh`: ahora estan en `glam::camera::rh`.
//! Se usa la proyeccion `directx` (profundidad 0..1, Y arriba), que es la que espera wgpu.

use glam::camera::rh::{proj::directx, view};
use glam::{Mat4, Vec3};

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
}

impl Default for Camera {
    fn default() -> Self {
        Camera { angle_x: 19.8, angle_y: -137.0, distance: 7.47, pan_x: 0.05, pan_z: 0.24, orthographic: false }
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

    /// Gira `dx`, `dy` pixeles de pantalla: 0.5 grados por pixel. Girar a mano vuelve a perspectiva.
    pub fn rotate_by(&mut self, dx: f32, dy: f32) {
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

    fn view(&self) -> Mat4 {
        view::look_at_mat4(
            Vec3::new(self.pan_x, -self.distance, self.pan_z),
            Vec3::new(self.pan_x, 0.0, self.pan_z),
            Vec3::Z,
        )
    }

    fn rotation(&self) -> Mat4 {
        Mat4::from_rotation_x(self.angle_x.to_radians()) * Mat4::from_rotation_z(self.angle_y.to_radians())
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
