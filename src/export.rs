//! Exportacion a PNG (ESPECIFICACION.md, seccion 13): RGBA de 8 bits con alfa RECTO (sin premultiplicar).
//! Se voltea en vertical porque las UVs del OBJ tienen v = 0 abajo, la textura guarda v = 0 en la fila 0 y en un
//! PNG la fila 0 es arriba; con el volteo la textura cae en su lugar en Blender.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

/// Copia de `rgba` (cuadrada de lado `size`) con las filas en orden inverso.
pub fn flip_rows(rgba: &[u8], size: u32) -> Vec<u8> {
    let row = (size * 4) as usize;
    let mut out = Vec::with_capacity(rgba.len());
    for y in (0..size as usize).rev() {
        out.extend_from_slice(&rgba[y * row..(y + 1) * row]);
    }
    out
}

/// Escribe un PNG RGBA de `size` x `size`. `flip_vertical` es `true` en todos los casos de la app.
pub fn write_png(path: &Path, size: u32, rgba: &[u8], flip_vertical: bool) -> Result<(), String> {
    let file =
        File::create(path).map_err(|e| format!("No se pudo crear {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    let result = if flip_vertical {
        writer.write_image_data(&flip_rows(rgba, size))
    } else {
        writer.write_image_data(rgba)
    };
    result.map_err(|e| e.to_string())
}

/// Primer nombre libre `base.png`, `base_1.png`, `base_2.png`... en la carpeta actual (no pisa archivos).
pub fn free_path(base: &str) -> PathBuf {
    let mut n = 0u32;
    loop {
        let name = if n == 0 {
            format!("{base}.png")
        } else {
            format!("{base}_{n}.png")
        };
        let path = PathBuf::from(name);
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_volteo_invierte_las_filas() {
        // 2x2: fila 0 = rojo,rojo; fila 1 = azul,azul.
        let img = [
            255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255,
        ];
        let f = flip_rows(&img, 2);
        assert_eq!(&f[0..4], &[0, 0, 255, 255]);
        assert_eq!(&f[8..12], &[255, 0, 0, 255]);
        // Voltear dos veces devuelve la imagen original.
        assert_eq!(flip_rows(&f, 2), img);
    }

    #[test]
    fn escribe_un_png_valido() {
        let dir = std::env::temp_dir().join("texturecraft_prueba_png");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prueba.png");
        let img = vec![200u8; 4 * 4 * 4];
        write_png(&path, 4, &img, true).unwrap();
        // Firma PNG.
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            &bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        let _ = std::fs::remove_file(&path);
    }
}
