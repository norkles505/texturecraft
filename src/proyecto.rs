//! Proyectos guardados: cada proyecto es una carpeta dentro de `projects_dir()` con
//!   - `proyecto.txt`: datos en texto (nombre, ruta del modelo .obj, y por cada pieza su fondo y sus capas),
//!   - `pieza<N>_capa<M>.png`: los pixeles de cada capa que tiene pintura (RGBA recto, sin voltear),
//!   - `miniatura.png`: la textura de la pieza activa, reducida (es la que se ve en la galeria).
//! Se guarda al pulsar "Regresar a mi galería" y la galeria lee las carpetas con `list`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::blend::BlendMode;
use crate::export::write_png;
use crate::layers::{Background, LayerStack, PaintLayer};

/// Lado (en pixeles) de la miniatura.
pub const THUMB: u32 = 256;

/// Carpeta donde viven todos los proyectos: `~/.local/share/texturecraft/proyectos` (o `proyectos_guardados` en la
/// carpeta actual si no hay HOME).
pub fn projects_dir() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".local/share/texturecraft/proyectos"),
        None => PathBuf::from("proyectos_guardados"),
    }
}

/// Identificador nuevo para un proyecto: los milisegundos desde 1970 (sirve de nombre de carpeta y para ordenar).
pub fn new_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string()
}

/// Texto sin tabuladores ni saltos de linea (el archivo `proyecto.txt` va una cosa por linea, separada por tabuladores).
fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// Reduce una imagen cuadrada RGBA de lado `size` a lado `target` promediando bloques. Devuelve los pixeles y el lado
/// (si la imagen ya es mas chica que `target`, se devuelve tal cual).
fn downscale(rgba: &[u8], size: u32, target: u32) -> (Vec<u8>, u32) {
    let t = target.min(size);
    if t == size {
        return (rgba.to_vec(), size);
    }
    let mut out = vec![0u8; (t * t * 4) as usize];
    for oy in 0..t {
        let y0 = oy * size / t;
        let y1 = ((oy + 1) * size / t).max(y0 + 1);
        for ox in 0..t {
            let x0 = ox * size / t;
            let x1 = ((ox + 1) * size / t).max(x0 + 1);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = ((y * size + x) * 4) as usize;
                    for (c, a) in acc.iter_mut().enumerate() {
                        *a += rgba[i + c] as u32;
                    }
                    n += 1;
                }
            }
            let o = ((oy * t + ox) * 4) as usize;
            for (c, a) in acc.iter().enumerate() {
                out[o + c] = (a / n) as u8;
            }
        }
    }
    (out, t)
}

/// Guarda el proyecto `id` (carpeta nueva o la misma de antes, que se sobrescribe). `stacks` son las capas de cada
/// pieza y `active` la pieza activa (su textura es la miniatura). Devuelve la carpeta donde quedo.
pub fn save(
    id: &str,
    name: &str,
    model: Option<&Path>,
    stacks: &[&LayerStack],
    active: usize,
) -> Result<PathBuf, String> {
    let dir = projects_dir().join(id);
    fs::create_dir_all(&dir).map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;

    let mut meta = String::new();
    meta.push_str(&format!("nombre={}\n", clean(name)));
    if let Some(m) = model {
        let full = fs::canonicalize(m).unwrap_or_else(|_| m.to_path_buf());
        meta.push_str(&format!("modelo={}\n", clean(&full.display().to_string())));
    }
    meta.push_str(&format!("pieza_activa={active}\n"));
    for (pi, stack) in stacks.iter().enumerate() {
        meta.push_str(&format!(
            "pieza\t{pi}\tfondo={:?}\tseleccionada={}\tcapas={}\n",
            stack.background,
            stack.selected,
            stack.layers.len()
        ));
        for (li, l) in stack.layers.iter().enumerate() {
            let file = dir.join(format!("pieza{pi}_capa{li}.png"));
            // Una capa sin nada de pintura no necesita archivo (y si antes lo tenia, se borra).
            let painted = l.pixels.chunks_exact(4).any(|p| p[3] != 0);
            if painted {
                write_png(&file, stack.size, &l.pixels, false)?;
            } else {
                let _ = fs::remove_file(&file);
            }
            meta.push_str(&format!(
                "capa\t{pi}\t{li}\tid={}\tnombre={}\tvisible={}\topacidad={}\tbloqueada={}\tbloqueo_alfa={}\tmezcla={}\trecorte={}\tpixeles={}\n",
                l.id,
                clean(&l.name),
                l.visible,
                l.opacity,
                l.locked,
                l.alpha_lock,
                l.blend.name(),
                l.clip_to_below,
                painted
            ));
        }
    }

    // Miniatura: la textura compuesta de la pieza activa (lo que se ve), reducida. Se voltea como al exportar.
    if let Some(stack) = stacks.get(active).or(stacks.first()) {
        let (small, side) = downscale(&stack.composite, stack.size, THUMB);
        write_png(&dir.join("miniatura.png"), side, &small, true)?;
    }

    fs::write(dir.join("proyecto.txt"), meta)
        .map_err(|e| format!("No se pudo guardar proyecto.txt: {e}"))?;
    Ok(dir)
}

/// Un proyecto guardado, tal como lo necesita la galeria.
pub struct Entry {
    pub id: String,
    pub name: String,
    /// Miniatura: ancho, alto y pixeles RGBA (`None` si no se pudo leer).
    pub thumb: Option<(u32, u32, Vec<u8>)>,
}

/// Proyectos guardados, del mas nuevo al mas viejo. Las carpetas sin `proyecto.txt` se ignoran.
pub fn list() -> Vec<Entry> {
    let Ok(read) = fs::read_dir(projects_dir()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let dir = entry.path();
        let Ok(meta) = fs::read_to_string(dir.join("proyecto.txt")) else {
            continue;
        };
        let id = entry.file_name().to_string_lossy().into_owned();
        let name = meta
            .lines()
            .find_map(|l| l.strip_prefix("nombre="))
            .unwrap_or("Sin nombre")
            .to_string();
        let thumb = image::open(dir.join("miniatura.png")).ok().map(|img| {
            let rgba = img.to_rgba8();
            (rgba.width(), rgba.height(), rgba.into_raw())
        });
        out.push(Entry { id, name, thumb });
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.id.parse::<u128>().unwrap_or(0)));
    out
}

/// Un proyecto leido de disco, listo para ponerlo en la app.
pub struct Loaded {
    pub name: String,
    /// Ruta del .obj con el que se guardo (`None` = la esfera de prueba).
    pub model: Option<PathBuf>,
    /// Pieza que estaba activa al guardar.
    pub active: usize,
    /// Capas de cada pieza, en el mismo orden en que se guardaron.
    pub stacks: Vec<LayerStack>,
}

/// Valor de un campo `clave=valor` entre los campos de una linea separada por tabuladores.
fn field<'a>(parts: &[&'a str], key: &str) -> Option<&'a str> {
    parts.iter().find_map(|p| p.strip_prefix(key))
}

/// Lee el proyecto `id` (lo que escribio `save`). `size` es el lado de las texturas de la app: una capa guardada con
/// otro tamano no se puede usar y da error.
pub fn load(id: &str, size: u32) -> Result<Loaded, String> {
    let dir = projects_dir().join(id);
    let text = fs::read_to_string(dir.join("proyecto.txt"))
        .map_err(|e| format!("No se pudo leer el proyecto {id}: {e}"))?;

    let mut name = String::from("Sin nombre");
    let mut model = None;
    let mut active = 0usize;
    // Por pieza: fondo, capa elegida y capas.
    let mut pieces: Vec<(Background, usize, Vec<PaintLayer>)> = Vec::new();

    for line in text.lines() {
        if let Some(v) = line.strip_prefix("nombre=") {
            name = v.to_string();
        } else if let Some(v) = line.strip_prefix("modelo=") {
            model = Some(PathBuf::from(v));
        } else if let Some(v) = line.strip_prefix("pieza_activa=") {
            active = v.parse().unwrap_or(0);
        } else if line.starts_with("pieza\t") {
            let f: Vec<&str> = line.split('\t').collect();
            let bg = Background::from_name(field(&f, "fondo=").unwrap_or("Checker"));
            let sel = field(&f, "seleccionada=")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            pieces.push((bg, sel, Vec::new()));
        } else if line.starts_with("capa\t") {
            let f: Vec<&str> = line.split('\t').collect();
            let pi: usize = f.get(1).and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
            let li: usize = f.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
            let Some(piece) = pieces.get_mut(pi) else {
                continue;
            };
            let pixels = if field(&f, "pixeles=") == Some("true") {
                let file = dir.join(format!("pieza{pi}_capa{li}.png"));
                let img = image::open(&file)
                    .map_err(|e| format!("No se pudo leer {}: {e}", file.display()))?
                    .to_rgba8();
                if img.width() != size || img.height() != size {
                    return Err(format!(
                        "{} mide {}x{} y la app usa {size}x{size}",
                        file.display(),
                        img.width(),
                        img.height()
                    ));
                }
                img.into_raw()
            } else {
                vec![0u8; (size * size * 4) as usize]
            };
            piece.2.push(PaintLayer {
                id: field(&f, "id=").and_then(|v| v.parse().ok()).unwrap_or(li as u32),
                name: field(&f, "nombre=").unwrap_or("1").to_string(),
                visible: field(&f, "visible=") != Some("false"),
                opacity: field(&f, "opacidad=")
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(100)
                    .min(100),
                locked: field(&f, "bloqueada=") == Some("true"),
                alpha_lock: field(&f, "bloqueo_alfa=") == Some("true"),
                blend: BlendMode::from_name(field(&f, "mezcla=").unwrap_or("NORMAL")),
                clip_to_below: field(&f, "recorte=") == Some("true"),
                pixels,
            });
        }
    }

    if pieces.is_empty() {
        return Err(format!("El proyecto {id} no tiene piezas guardadas"));
    }
    let stacks = pieces
        .into_iter()
        .map(|(bg, sel, layers)| LayerStack::from_saved(size, layers, sel, bg))
        .collect();
    Ok(Loaded {
        name,
        model,
        active,
        stacks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_miniatura_promedia_bloques() {
        // 4x4: la mitad izquierda negra y la derecha blanca, reducida a 2x2.
        let mut img = vec![0u8; 4 * 4 * 4];
        for y in 0..4 {
            for x in 0..4 {
                let v = if x < 2 { 0 } else { 255 };
                let i = (y * 4 + x) * 4;
                img[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
        let (small, side) = downscale(&img, 4, 2);
        assert_eq!(side, 2);
        assert_eq!(&small[0..4], &[0, 0, 0, 255]);
        assert_eq!(&small[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn una_imagen_chica_no_se_agranda() {
        let img = vec![9u8; 2 * 2 * 4];
        let (out, side) = downscale(&img, 2, 256);
        assert_eq!(side, 2);
        assert_eq!(out, img);
    }
}
