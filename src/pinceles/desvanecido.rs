//! DESVANECIDO de inicio y fin de trazo: la opacidad de cada toque depende de DONDE va en el trazo. Lo usa el pincel
//! Arcane (`BrushType::Arcane`), portado del QR de un pincel de ibisPaint y de capturas de sus trazos.
//!
//! Medido en las capturas de ese pincel (3 trazos verticales rojos de 54 px de ancho, sobre blanco):
//!
//! - Inicio: en ibisPaint el trazo nace casi transparente y se afirma RAPIDO (la opacidad vale 0.15 en el borde de la
//!   punta, 0.58 a 10 px, 0.91 a 30 px y 1 a 50 px, con un radio de 27 px). Aqui NO se copia eso: por pedido del equipo el
//!   inicio es el ESPEJO del final (la misma rampa lineal de 5.6 radios, de 0.01 a 1), para que se vea mejor. La
//!   version rapida medida era `1 - exp(-(s + 0.1) / 0.42)` con `s` en radios.
//! - Final: la opacidad baja de 1 a casi 0 en 100, 135 y 190 px en los tres trazos (media unos 145 px, o sea 2.7
//!   diametros). Coincide con el QR (longitud del desvanecido 0.4, que la guia calibra como 2.8 diametros): se usa una
//!   rampa lineal de 5.6 radios (2.8 diametros) que llega a 0.01 en el ultimo toque (opacidad al final 0.01 del QR).
//!   Los tres trazos del ejemplo difieren entre si (parece depender de la velocidad o la presion del lapiz); aqui es
//!   fijo.
//! - Ancho constante y opacidad 1 en el cuerpo del trazo.
//!
//! El final del trazo no se sabe hasta soltar el boton, asi que el trazo se pinta primero con el desvanecido de
//! inicio y al soltar se REPINTA entero con los dos (`stroke_end` en `main.rs`). Todas las distancias son en radios
//! del pincel (`paso` es el espaciado entre toques, tambien en radios).

/// Largo del desvanecido de final (radios) y la opacidad con la que llega al ultimo toque.
const END_LEN: f32 = 5.6;
const END_MIN: f32 = 0.01;

/// Multiplicador de opacidad de un toque que esta a `s` radios del primero del trazo (0.01..1). Es el espejo del
/// desvanecido de final: la misma rampa lineal (se pidio que el inicio fuera como el final).
pub fn entrada(s: f32) -> f32 {
    salida(s)
}

/// Multiplicador de opacidad de un toque que esta a `e` radios del ultimo del trazo (0.01..1).
pub fn salida(e: f32) -> f32 {
    (END_MIN + (1.0 - END_MIN) * e / END_LEN).clamp(END_MIN, 1.0)
}

/// Multiplicador del toque numero `k` (desde 0) de un trazo de `n` toques separados `paso` radios.
pub fn toque(k: usize, n: usize, paso: f32) -> f32 {
    let desde_el_inicio = k as f32 * paso;
    let hasta_el_final = n.saturating_sub(1).saturating_sub(k) as f32 * paso;
    entrada(desde_el_inicio) * salida(hasta_el_final)
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::brush::BrushType;
    use crate::paint::{DabOptions, StrokeState, SurfTri};

    #[test]
    fn la_entrada_es_el_espejo_de_la_salida() {
        for i in 0..200 {
            let r = i as f32 * 0.05;
            assert_eq!(entrada(r), salida(r));
        }
        assert!((entrada(0.0) - 0.01).abs() < 1e-6);
        assert_eq!(entrada(5.6), 1.0);
        assert_eq!(entrada(30.0), 1.0);
    }

    #[test]
    fn la_salida_baja_en_linea_recta_de_1_a_0_01_en_5_6_radios() {
        assert!((salida(0.0) - 0.01).abs() < 1e-6);
        assert_eq!(salida(5.6), 1.0);
        assert_eq!(salida(20.0), 1.0);
        assert!((salida(2.8) - 0.505).abs() < 1e-3);
        let mut prev = 0.0;
        for i in 0..200 {
            let v = salida(i as f32 * 0.05);
            assert!(v >= prev, "no sube");
            prev = v;
        }
    }

    #[test]
    fn el_toque_combina_los_dos_y_un_trazo_corto_no_llega_a_1() {
        let paso = 0.05;
        let n = 400;
        // En el medio de un trazo largo vale 1; el primero y el ultimo, casi nada.
        assert!(toque(200, n, paso) > 0.999);
        assert!(toque(0, n, paso) < 0.25);
        assert!((toque(n - 1, n, paso) - 0.01).abs() < 1e-6);
        // Un solo toque y un trazo de 2 toques no fallan.
        assert!((0.0..=1.0).contains(&toque(0, 1, paso)));
        assert!((0.0..=1.0).contains(&toque(1, 2, paso)));
        // Un trazo corto (3 radios) nunca llega a la opacidad plena: el final lo alcanza antes.
        let corto = 60;
        let max = (0..corto)
            .map(|k| toque(k, corto, paso))
            .fold(0.0f32, f32::max);
        assert!(max < 0.9, "un trazo corto llega a {max}");
    }

    fn cuadrado() -> [SurfTri; 2] {
        let p = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(4.0, 4.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        ];
        let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
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
    fn un_trazo_con_desvanecido_nace_y_muere_suave_y_el_cuerpo_queda_pleno() {
        let radio = 0.2f32;
        let paso = BrushType::Arcane.spacing();
        let tris = cuadrado();
        for ceiling in [false, true] {
            let opts = DabOptions {
                brush: BrushType::Arcane,
                normal: Vec3::Z,
                dir: Vec3::X,
                ceiling,
                ..Default::default()
            };
            let mut s = StrokeState::new(256);
            let mut pixels = vec![0u8; 256 * 256 * 4];
            s.begin_stroke();
            let n = 300;
            let x0 = 0.4f32;
            for k in 0..n {
                let c = Vec3::new(x0 + k as f32 * paso * radio, 2.0, 0.0);
                s.dab_surface(
                    &mut pixels,
                    &tris,
                    c,
                    radio,
                    [200, 20, 20],
                    toque(k, n, paso),
                    opts,
                );
            }
            // Opacidad a lo largo del eje del trazo, de izquierda a derecha.
            let fila = (2.0f32 / 4.0 * 255.0).round() as usize;
            let alfa = |x: f32| pixels[(fila * 256 + (x / 4.0 * 255.0).round() as usize) * 4 + 3];
            let ultimo = x0 + (n - 1) as f32 * paso * radio;
            // Cuerpo: pleno y con el color del pincel.
            let medio = (x0 + ultimo) / 2.0;
            assert!(alfa(medio) >= 250, "ceiling {ceiling}: el cuerpo vale {}", alfa(medio));
            let i = (fila * 256 + (medio / 4.0 * 255.0).round() as usize) * 4;
            assert_eq!(&pixels[i..i + 3], &[200, 20, 20]);
            // Inicio: en el borde de la punta casi no hay pintura.
            assert!(
                alfa(x0 - 0.9 * radio) < 90,
                "ceiling {ceiling}: el inicio vale {}",
                alfa(x0 - 0.9 * radio)
            );
            // Final: la opacidad baja sin volver a subir y termina casi transparente.
            let (mut prev, desde) = (255u8, ultimo - 5.0 * radio);
            let mut x = desde;
            while x < ultimo + 0.9 * radio {
                let a = alfa(x);
                assert!(a <= prev.saturating_add(3), "ceiling {ceiling}: vuelve a subir en x {x}");
                prev = prev.min(a);
                x += 0.02;
            }
            assert!(
                alfa(ultimo + 0.9 * radio) < 20,
                "ceiling {ceiling}: el final vale {}",
                alfa(ultimo + 0.9 * radio)
            );
            assert!(alfa(ultimo) < 120, "ceiling {ceiling}: el ultimo toque vale {}", alfa(ultimo));
        }
    }
}
