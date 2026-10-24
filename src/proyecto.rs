//! Proyectos guardados: cada proyecto es una carpeta dentro de `projects_dir()` con
//!   - `proyecto.txt`: datos en texto (nombre, ruta del modelo .obj, y por cada pieza su fondo y sus capas),
//!   - `pieza<N>_capa<M>.png`: los pixeles de cada capa que tiene pintura (RGBA recto, sin voltear),
//!   - `pieza<N>_capa<M>_mascara.png`: la mascara de la capa, si tiene (grises, opaca),
//!   - `miniatura.png`: la textura de la pieza activa, reducida (es la que se ve en la galeria).
//! Se guarda al pulsar "Regresar a mi galería" y la galeria lee las carpetas con `list`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::blend::BlendMode;
use crate::export::write_png;
use crate::layers::{Background, ChannelKind, LayerMask, LayerSource, LayerStack, PaintLayer};

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
    save_in(&projects_dir(), id, name, model, stacks, active)
}

/// Lo mismo que `save`, dentro de la carpeta `root` (las pruebas usan una temporal para no tocar los proyectos de verdad).
fn save_in(
    root: &Path,
    id: &str,
    name: &str,
    model: Option<&Path>,
    stacks: &[&LayerStack],
    active: usize,
) -> Result<PathBuf, String> {
    let dir = root.join(id);
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
            // Una capa sin nada de pintura no necesita archivo (y si antes lo tenia, se borra). Lo de una carpeta no se
            // guarda: es el resultado de componer sus capas de dentro y se rehace al abrir.
            let painted = !l.is_group() && l.pixels.chunks_exact(4).any(|p| p[3] != 0);
            if painted {
                write_png(&file, stack.size, &l.pixels, false)?;
            } else {
                let _ = fs::remove_file(&file);
            }
            // La mascara va en su propio PNG (siempre entera: es opaca). Sin mascara, se borra el archivo de antes.
            let mask_file = dir.join(format!("pieza{pi}_capa{li}_mascara.png"));
            let (has_mask, mask_on, mask_inv) = match &l.mask {
                Some(m) => {
                    write_png(&mask_file, stack.size, &m.pixels, false)?;
                    (true, m.enabled, m.invert)
                }
                None => {
                    let _ = fs::remove_file(&mask_file);
                    (false, true, false)
                }
            };
            // Hojas de los canales extra (rugosidad, metalico, altura): un PNG por canal pintado.
            let mut chan_flags = String::new();
            for k in ChannelKind::EXTRA {
                let Some(slot) = k.slot() else { continue };
                let cfile = dir.join(format!("pieza{pi}_capa{li}_{}.png", k.file_tag()));
                match &l.chans[slot] {
                    Some(px) if !l.is_group() => {
                        write_png(&cfile, stack.size, px, false)?;
                        chan_flags.push_str(&format!("\tc_{}=true", k.file_tag()));
                    }
                    _ => {
                        let _ = fs::remove_file(&cfile);
                    }
                }
            }
            // Capa de color liso: su color va en la linea (`relleno=RRGGBB`); las de pintura normal no llevan el campo.
            if let LayerSource::Solid(c) = l.source {
                chan_flags.push_str(&format!("\trelleno={:02X}{:02X}{:02X}", c[0], c[1], c[2]));
            }
            // Carpeta (`tipo=carpeta`) y, en las capas de dentro, la carpeta a la que pertenecen (`en_carpeta=<id>`).
            if l.is_group() {
                chan_flags.push_str("\ttipo=carpeta");
            }
            if let Some(p) = l.parent {
                chan_flags.push_str(&format!("\ten_carpeta={p}"));
            }
            meta.push_str(&format!(
                "capa\t{pi}\t{li}\tid={}\tnombre={}\tvisible={}\topacidad={}\tbloqueada={}\tbloqueo_alfa={}\tmezcla={}\trecorte={}\tpixeles={}\tmascara={}\tmascara_activa={}\tmascara_invertida={}{}\n",
                l.id,
                clean(&l.name),
                l.visible,
                l.opacity,
                l.locked,
                l.alpha_lock,
                l.blend.name(),
                l.clip_to_below,
                painted,
                has_mask,
                mask_on,
                mask_inv,
                chan_flags
            ));
        }
    }

    // Miniatura: la textura compuesta de la pieza activa (lo que se ve), reducida. Se voltea como al exportar.
    if let Some(stack) = stacks.get(active).or(stacks.first()) {
        // La miniatura sale de todas las capas con su fondo, no de lo que se ve ahora (que puede ser una sola capa por el
        // Modo Solo, o un canal en grises).
        let (small, side) = downscale(&stack.export_composite(true), stack.size, THUMB);
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

/// Color de seis cifras hexadecimales (`RRGGBB`); `None` si no lo es.
fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

/// Lee un PNG de capa o de mascara (RGBA, lado `size`). Si mide otra cosa no se puede usar y da error.
fn read_layer_png(file: &Path, size: u32) -> Result<Vec<u8>, String> {
    let img = image::open(file)
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
    Ok(img.into_raw())
}

/// Lee el proyecto `id` (lo que escribio `save`). `size` es el lado de las texturas de la app: una capa guardada con
/// otro tamano no se puede usar y da error.
pub fn load(id: &str, size: u32) -> Result<Loaded, String> {
    load_in(&projects_dir(), id, size)
}

/// Lo mismo que `load`, dentro de la carpeta `root` (para las pruebas).
fn load_in(root: &Path, id: &str, size: u32) -> Result<Loaded, String> {
    let dir = root.join(id);
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
                read_layer_png(&dir.join(format!("pieza{pi}_capa{li}.png")), size)?
            } else {
                vec![0u8; (size * size * 4) as usize]
            };
            // Mascara (los proyectos guardados antes de que existieran no traen el campo: capa sin mascara).
            let mask = if field(&f, "mascara=") == Some("true") {
                Some(LayerMask {
                    enabled: field(&f, "mascara_activa=") != Some("false"),
                    invert: field(&f, "mascara_invertida=") == Some("true"),
                    pixels: read_layer_png(
                        &dir.join(format!("pieza{pi}_capa{li}_mascara.png")),
                        size,
                    )?,
                })
            } else {
                None
            };
            // Canales extra (los proyectos viejos no traen estos campos: sin canales).
            let mut chans: [Option<Vec<u8>>; 3] = [None, None, None];
            for k in ChannelKind::EXTRA {
                let Some(slot) = k.slot() else { continue };
                if field(&f, &format!("c_{}=", k.file_tag())) == Some("true") {
                    chans[slot] = Some(read_layer_png(
                        &dir.join(format!("pieza{pi}_capa{li}_{}.png", k.file_tag())),
                        size,
                    )?);
                }
            }
            // Tipo de capa (los proyectos viejos no traen estos campos: pintura normal y sin carpetas).
            let source = if field(&f, "tipo=") == Some("carpeta") {
                LayerSource::Group
            } else {
                match field(&f, "relleno=").and_then(parse_hex_color) {
                    Some(c) => LayerSource::Solid(c),
                    None => LayerSource::Paint,
                }
            };
            let parent = field(&f, "en_carpeta=").and_then(|v| v.parse().ok());
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
                mask,
                editing_mask: false,
                chans,
                source,
                parent,
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
    fn el_color_del_relleno_se_lee_en_hexadecimal() {
        assert_eq!(parse_hex_color("C81E28"), Some([200, 30, 40]));
        assert_eq!(parse_hex_color("000000"), Some([0, 0, 0]));
        // Cualquier otra cosa no es un color.
        assert_eq!(parse_hex_color("C81E2"), None);
        assert_eq!(parse_hex_color("GGGGGG"), None);
        assert_eq!(parse_hex_color("é1234"), None);
    }

    /// Carpeta temporal propia de una prueba (no toca los proyectos de verdad de `projects_dir`).
    fn carpeta_de_prueba(nombre: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "texturecraft_{nombre}_{}_{}",
            std::process::id(),
            new_id()
        ))
    }

    #[test]
    fn guardar_y_abrir_conserva_todo_lo_que_hacen_las_capas() {
        let root = carpeta_de_prueba("ida_y_vuelta");
        let size = 64;
        let i = |x: u32, y: u32| ((y * size + x) * 4) as usize;
        let mut s = LayerStack::new(size);
        s.background = Background::White;
        // Capa de pintura con color, rugosidad, mascara (apagada e invertida) y ajustes.
        s.layers[0].name = "base áéí".to_string();
        s.layers[0].opacity = 70;
        s.layers[0].blend = BlendMode::Multiply;
        s.layers[0].locked = true;
        s.layers[0].pixels[i(5, 5)..i(5, 5) + 4].copy_from_slice(&[255, 0, 0, 255]);
        let mut rugosidad = vec![0u8; (size * size * 4) as usize];
        rugosidad[i(6, 6)..i(6, 6) + 4].copy_from_slice(&[200, 200, 200, 255]);
        s.layers[0].chans[0] = Some(rugosidad);
        assert!(s.add_mask());
        let m = s.layers[0].mask.as_mut().unwrap();
        m.pixels[i(7, 7)..i(7, 7) + 4].copy_from_slice(&[0, 0, 0, 255]);
        m.enabled = false;
        m.invert = true;
        // Capa de color liso con su mascara.
        s.selected = 0;
        assert!(s.add_solid_layer([10, 20, 30]));
        s.layers[1].name = "color".to_string();
        s.layers[1].mask.as_mut().unwrap().pixels[i(8, 8)..i(8, 8) + 4]
            .copy_from_slice(&[255, 255, 255, 255]);
        // Carpeta con la capa base dentro y una capa mas: [base, nueva, carpeta, color].
        s.selected = 0;
        assert!(s.add_folder());
        s.selected = 0;
        assert!(s.add_layer());
        s.layers[1].pixels[i(9, 9)..i(9, 9) + 4].copy_from_slice(&[0, 255, 0, 255]);
        s.layers[2].name = "mi carpeta".to_string();
        s.layers[2].opacity = 55;
        s.selected = 3;
        s.composite_all();
        let antes = s.composite.clone();
        let carpeta_id = s.layers[2].id;

        save_in(&root, "t1", "Mi proyecto", None, &[&s], 0).unwrap();
        let abierto = load_in(&root, "t1", size);
        let _ = fs::remove_dir_all(&root);
        let mut c = abierto.unwrap();
        let t = &mut c.stacks[0];
        t.composite_all();

        assert_eq!(c.name, "Mi proyecto");
        assert_eq!(t.layers.len(), 4);
        assert_eq!(t.selected, 3);
        assert_eq!(t.background, Background::White);
        for k in 0..4 {
            let (a, b) = (&s.layers[k], &t.layers[k]);
            assert_eq!(a.id, b.id, "id de la capa {k}");
            assert_eq!(a.name, b.name, "nombre de la capa {k}");
            assert_eq!(a.visible, b.visible, "visible de la capa {k}");
            assert_eq!(a.opacity, b.opacity, "opacidad de la capa {k}");
            assert_eq!(a.blend, b.blend, "mezcla de la capa {k}");
            assert_eq!(a.locked, b.locked, "bloqueo de la capa {k}");
            assert_eq!(a.source, b.source, "tipo de la capa {k}");
            assert_eq!(a.parent, b.parent, "carpeta de la capa {k}");
        }
        // Pintura, canal y mascara de la capa base.
        assert_eq!(t.layers[0].pixels, s.layers[0].pixels);
        assert_eq!(t.layers[0].chans[0], s.layers[0].chans[0]);
        let (ma, mb) = (
            s.layers[0].mask.as_ref().unwrap(),
            t.layers[0].mask.as_ref().unwrap(),
        );
        assert_eq!(ma.pixels, mb.pixels);
        assert_eq!((ma.enabled, ma.invert), (mb.enabled, mb.invert));
        assert_eq!((mb.enabled, mb.invert), (false, true));
        // Carpeta: sus capas apuntan a ella y ella es una carpeta; la capa de color liso conserva color y mascara.
        assert!(t.layers[2].is_group());
        assert_eq!(t.layers[0].parent, Some(carpeta_id));
        assert_eq!(t.layers[1].parent, Some(carpeta_id));
        assert_eq!(t.layers[3].source, LayerSource::Solid([10, 20, 30]));
        assert_eq!(
            t.layers[3].mask.as_ref().unwrap().pixels,
            s.layers[3].mask.as_ref().unwrap().pixels
        );
        // Lo que importa: al componer sale exactamente la misma imagen.
        assert_eq!(t.composite, antes);
    }

    #[test]
    fn un_proyecto_guardado_antes_de_mascaras_y_carpetas_se_abre_igual() {
        let root = carpeta_de_prueba("viejo");
        let dir = root.join("v1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("proyecto.txt"),
            "nombre=Viejo\npieza_activa=0\n\
             pieza\t0\tfondo=White\tseleccionada=0\tcapas=1\n\
             capa\t0\t0\tid=0\tnombre=1\tvisible=true\topacidad=80\tbloqueada=false\tbloqueo_alfa=false\tmezcla=NORMAL\trecorte=false\tpixeles=false\n",
        )
        .unwrap();
        let abierto = load_in(&root, "v1", 32);
        let _ = fs::remove_dir_all(&root);
        let c = abierto.unwrap();
        assert_eq!(c.stacks.len(), 1);
        let capas = &c.stacks[0].layers;
        assert_eq!(capas.len(), 1);
        assert_eq!(capas[0].opacity, 80);
        assert!(capas[0].mask.is_none());
        assert_eq!(capas[0].source, LayerSource::Paint);
        assert_eq!(capas[0].parent, None);
        assert!(capas[0].chans.iter().all(|c| c.is_none()));
    }

    #[test]
    fn una_imagen_chica_no_se_agranda() {
        let img = vec![9u8; 2 * 2 * 4];
        let (out, side) = downscale(&img, 2, 256);
        assert_eq!(side, 2);
        assert_eq!(out, img);
    }
}
