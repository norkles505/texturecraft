//! Exportacion a PNG (ESPECIFICACION.md, seccion 13): RGBA de 8 bits con alfa RECTO (sin premultiplicar).
//! Se voltea en vertical porque las UVs del OBJ tienen v = 0 abajo, la textura guarda v = 0 en la fila 0 y en un
//! PNG la fila 0 es arriba; con el volteo la textura cae en su lugar en Blender.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use crate::layers::{ChannelKind, LayerStack, NORMAL_STRENGTH};

/// Motor o programa al que se exporta el material, ya empaquetado como lo espera (ver `engine_maps`). Las convenciones
/// (que va en cada canal, y si las normales son OpenGL o DirectX) son las publicas de cada motor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnginePreset {
    Unreal,
    UnityStandard,
    UnityHdrp,
    Godot,
    Blender,
}

impl EnginePreset {
    pub const ALL: [EnginePreset; 5] = [
        EnginePreset::Unreal,
        EnginePreset::UnityStandard,
        EnginePreset::UnityHdrp,
        EnginePreset::Godot,
        EnginePreset::Blender,
    ];

    /// Nombre para mostrar.
    pub fn label(self) -> &'static str {
        match self {
            EnginePreset::Unreal => "Unreal Engine",
            EnginePreset::UnityStandard => "Unity (Standard)",
            EnginePreset::UnityHdrp => "Unity HDRP",
            EnginePreset::Godot => "Godot 4",
            EnginePreset::Blender => "Blender",
        }
    }

    /// Que archivos crea, en una linea (se muestra bajo el nombre en la ventana de exportar).
    pub fn detail(self) -> &'static str {
        match self {
            EnginePreset::Unreal => "BaseColor, ORM (oclusión, rugosidad, metálico) y Normal DirectX",
            EnginePreset::UnityStandard => "Albedo, MetallicSmoothness (suavidad en el alfa) y Normal",
            EnginePreset::UnityHdrp => "BaseMap, MaskMap (metálico, oclusión, suavidad) y Normal",
            EnginePreset::Godot => "albedo, orm (oclusión, rugosidad, metálico) y normal",
            EnginePreset::Blender => "BaseColor, Metallic, Roughness, Normal y Height por separado",
        }
    }

    /// Nombre para la carpeta (sin acentos ni espacios).
    pub fn folder_tag(self) -> &'static str {
        match self {
            EnginePreset::Unreal => "unreal",
            EnginePreset::UnityStandard => "unity",
            EnginePreset::UnityHdrp => "unity_hdrp",
            EnginePreset::Godot => "godot",
            EnginePreset::Blender => "blender",
        }
    }
}

/// Lo que se exporta de una pieza, ya compuesto. Todo es RGBA de `size` x `size` con la misma fila 0 que la textura. Los
/// canales en grises valen lo que dice su primer byte; `None` = nadie pinto ese canal.
pub struct MaterialInputs<'a> {
    pub size: u32,
    /// Color con alfa recto (`LayerStack::export_composite(false)`).
    pub color: &'a [u8],
    pub roughness: Option<&'a [u8]>,
    pub metallic: Option<&'a [u8]>,
    pub height: Option<&'a [u8]>,
    /// Mapa de normales en formato OpenGL (el que calcula `LayerStack::export_normal`); `None` si no hay altura pintada.
    pub normal: Option<&'a [u8]>,
}

/// Un mapa listo para guardar: `<nombre>_<suffix>.png`.
pub struct ExportMap {
    pub suffix: &'static str,
    pub rgba: Vec<u8>,
}

/// Valor de un canal en grises en el texel que empieza en el byte `i`, o `default` si nadie pinto ese canal.
fn sample(chan: Option<&[u8]>, i: usize, default: u8) -> u8 {
    chan.map_or(default, |c| c[i])
}

/// Imagen de `size` x `size` donde cada texel (su byte `i`) lo calcula `f`.
fn pack(size: u32, f: impl Fn(usize) -> [u8; 4]) -> Vec<u8> {
    let mut out = vec![0u8; (size * size * 4) as usize];
    for i in (0..out.len()).step_by(4) {
        out[i..i + 4].copy_from_slice(&f(i));
    }
    out
}

/// Un canal en grises como imagen opaca.
fn gray_map(size: u32, chan: &[u8]) -> Vec<u8> {
    pack(size, |i| [chan[i], chan[i], chan[i], 255])
}

/// Normales OpenGL (verde hacia +V, las de Blender, Unity y Godot) pasadas a DirectX (verde hacia abajo, las de Unreal):
/// solo se invierte el verde.
fn normal_directx(normal_opengl: &[u8]) -> Vec<u8> {
    let mut out = normal_opengl.to_vec();
    for p in out.chunks_exact_mut(4) {
        p[1] = 255 - p[1];
    }
    out
}

/// Los mapas que pide `preset`, empaquetados como los espera ese motor. Este programa no tiene canal de oclusion: donde
/// el motor lo pide en un canal, va en blanco (255 = sin sombra), y no se crea un mapa de oclusion aparte. Donde pide
/// "suavidad" (Unity), es 255 menos la rugosidad. La rugosidad y el metalico sin pintar valen lo que su valor por
/// defecto (`ChannelKind::default_value`). Sin altura pintada no hay normales y no se crea ese mapa.
pub fn engine_maps(preset: EnginePreset, m: &MaterialInputs) -> Vec<ExportMap> {
    let size = m.size;
    let rough = |i: usize| sample(m.roughness, i, ChannelKind::Roughness.default_value());
    let metal = |i: usize| sample(m.metallic, i, ChannelKind::Metallic.default_value());
    let color = |suffix| ExportMap {
        suffix,
        rgba: m.color.to_vec(),
    };
    let normal = |suffix, directx: bool| {
        m.normal.map(|n| ExportMap {
            suffix,
            rgba: if directx {
                normal_directx(n)
            } else {
                n.to_vec()
            },
        })
    };
    let mut maps = Vec::new();
    match preset {
        EnginePreset::Unreal => {
            maps.push(color("BaseColor"));
            maps.push(ExportMap {
                suffix: "OcclusionRoughnessMetallic",
                rgba: pack(size, |i| [255, rough(i), metal(i), 255]),
            });
            maps.extend(normal("Normal", true));
        }
        EnginePreset::Godot => {
            maps.push(color("albedo"));
            maps.push(ExportMap {
                suffix: "orm",
                rgba: pack(size, |i| [255, rough(i), metal(i), 255]),
            });
            maps.extend(normal("normal", false));
        }
        EnginePreset::UnityStandard => {
            maps.push(color("Albedo"));
            maps.push(ExportMap {
                suffix: "MetallicSmoothness",
                rgba: pack(size, |i| [metal(i), 0, 0, 255 - rough(i)]),
            });
            maps.extend(normal("Normal", false));
        }
        EnginePreset::UnityHdrp => {
            maps.push(color("BaseMap"));
            // R = metalico, G = oclusion, B = mascara de detalle (ninguna), A = suavidad.
            maps.push(ExportMap {
                suffix: "MaskMap",
                rgba: pack(size, |i| [metal(i), 255, 0, 255 - rough(i)]),
            });
            maps.extend(normal("Normal", false));
        }
        EnginePreset::Blender => {
            maps.push(color("BaseColor"));
            // Blender lee cada mapa por separado: solo se crean los que alguien pinto.
            for (suffix, chan) in [
                ("Metallic", m.metallic),
                ("Roughness", m.roughness),
            ] {
                if let Some(c) = chan {
                    maps.push(ExportMap {
                        suffix,
                        rgba: gray_map(size, c),
                    });
                }
            }
            maps.extend(normal("Normal", false));
            if let Some(h) = m.height {
                maps.push(ExportMap {
                    suffix: "Height",
                    rgba: gray_map(size, h),
                });
            }
        }
    }
    maps
}

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

/// Lo que se eligio en la ventana de Exportar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveKind {
    /// Un PNG de color con los canales pintados al lado, como siempre.
    Png,
    /// Los mapas ya empaquetados para un motor, en una carpeta propia.
    Engine(EnginePreset),
}

/// Los mapas de `stack` empaquetados para `preset` (el color sale transparente donde no hay pintura, como el PNG).
pub fn stack_engine_maps(preset: EnginePreset, stack: &LayerStack) -> Vec<ExportMap> {
    let color = stack.export_composite(false);
    let roughness = stack.export_channel(ChannelKind::Roughness);
    let metallic = stack.export_channel(ChannelKind::Metallic);
    let height = stack.export_channel(ChannelKind::Height);
    let normal = stack.export_normal(NORMAL_STRENGTH);
    engine_maps(
        preset,
        &MaterialInputs {
            size: stack.size,
            color: &color,
            roughness: roughness.as_deref(),
            metallic: metallic.as_deref(),
            height: height.as_deref(),
            normal: normal.as_deref(),
        },
    )
}

/// Primer nombre de carpeta libre `parent/base`, `parent/base_1`, `parent/base_2`... (no pisa carpetas).
pub fn free_dir(parent: &Path, base: &str) -> PathBuf {
    let mut n = 0u32;
    loop {
        let name = if n == 0 {
            base.to_string()
        } else {
            format!("{base}_{n}")
        };
        let path = parent.join(name);
        if !path.exists() {
            return path;
        }
        n += 1;
    }
}

/// Crea `dir` (y las carpetas que falten) y guarda ahi cada mapa como `<base>_<suffix>.png`. Devuelve las rutas escritas.
pub fn write_maps(
    dir: &Path,
    base: &str,
    size: u32,
    maps: &[ExportMap],
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("No se pudo crear {}: {e}", dir.display()))?;
    let mut written = Vec::new();
    for m in maps {
        let path = dir.join(format!("{base}_{}.png", m.suffix));
        write_png(&path, size, &m.rgba, true)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entradas de 1x1 texel: color (10,20,30), rugosidad 200, metalico 50 y normal (128,100,255).
    fn entradas<'a>(
        color: &'a [u8],
        rough: Option<&'a [u8]>,
        metal: Option<&'a [u8]>,
        normal: Option<&'a [u8]>,
    ) -> MaterialInputs<'a> {
        MaterialInputs {
            size: 1,
            color,
            roughness: rough,
            metallic: metal,
            height: None,
            normal,
        }
    }

    fn sufijos(maps: &[ExportMap]) -> Vec<&'static str> {
        maps.iter().map(|m| m.suffix).collect()
    }

    fn mapa<'a>(maps: &'a [ExportMap], suffix: &str) -> &'a [u8] {
        &maps.iter().find(|m| m.suffix == suffix).unwrap().rgba
    }

    const COLOR: [u8; 4] = [10, 20, 30, 255];
    const ROUGH: [u8; 4] = [200, 200, 200, 255];
    const METAL: [u8; 4] = [50, 50, 50, 255];
    const NORMAL: [u8; 4] = [128, 100, 255, 255];

    #[test]
    fn unreal_empaqueta_orm_y_normal_directx() {
        let m = entradas(&COLOR, Some(&ROUGH), Some(&METAL), Some(&NORMAL));
        let maps = engine_maps(EnginePreset::Unreal, &m);
        assert_eq!(
            sufijos(&maps),
            ["BaseColor", "OcclusionRoughnessMetallic", "Normal"]
        );
        assert_eq!(mapa(&maps, "BaseColor"), &COLOR);
        // R = oclusion (blanca), G = rugosidad, B = metalico.
        assert_eq!(mapa(&maps, "OcclusionRoughnessMetallic"), &[255, 200, 50, 255]);
        // DirectX: solo se invierte el verde.
        assert_eq!(mapa(&maps, "Normal"), &[128, 155, 255, 255]);
    }

    #[test]
    fn godot_usa_orm_y_normal_opengl() {
        let m = entradas(&COLOR, Some(&ROUGH), Some(&METAL), Some(&NORMAL));
        let maps = engine_maps(EnginePreset::Godot, &m);
        assert_eq!(sufijos(&maps), ["albedo", "orm", "normal"]);
        assert_eq!(mapa(&maps, "orm"), &[255, 200, 50, 255]);
        assert_eq!(mapa(&maps, "normal"), &NORMAL);
    }

    #[test]
    fn unity_standard_pone_la_suavidad_en_el_alfa() {
        let m = entradas(&COLOR, Some(&ROUGH), Some(&METAL), Some(&NORMAL));
        let maps = engine_maps(EnginePreset::UnityStandard, &m);
        assert_eq!(sufijos(&maps), ["Albedo", "MetallicSmoothness", "Normal"]);
        // R = metalico, alfa = suavidad = 255 - rugosidad.
        assert_eq!(mapa(&maps, "MetallicSmoothness"), &[50, 0, 0, 55]);
        assert_eq!(mapa(&maps, "Normal"), &NORMAL);
    }

    #[test]
    fn unity_hdrp_empaqueta_la_mask_map() {
        let m = entradas(&COLOR, Some(&ROUGH), Some(&METAL), None);
        let maps = engine_maps(EnginePreset::UnityHdrp, &m);
        // Sin altura pintada no hay normales: no se crea ese mapa.
        assert_eq!(sufijos(&maps), ["BaseMap", "MaskMap"]);
        // R = metalico, G = oclusion, B = detalle (ninguno), A = suavidad.
        assert_eq!(mapa(&maps, "MaskMap"), &[50, 255, 0, 55]);
    }

    #[test]
    fn sin_pintar_rugosidad_ni_metalico_valen_su_valor_por_defecto() {
        let m = entradas(&COLOR, None, None, None);
        let r = ChannelKind::Roughness.default_value();
        let me = ChannelKind::Metallic.default_value();
        let maps = engine_maps(EnginePreset::Unreal, &m);
        assert_eq!(sufijos(&maps), ["BaseColor", "OcclusionRoughnessMetallic"]);
        assert_eq!(mapa(&maps, "OcclusionRoughnessMetallic"), &[255, r, me, 255]);
    }

    #[test]
    fn blender_solo_crea_los_mapas_pintados() {
        let vacio = entradas(&COLOR, None, None, None);
        assert_eq!(sufijos(&engine_maps(EnginePreset::Blender, &vacio)), ["BaseColor"]);
        let altura = [90u8, 90, 90, 255];
        let m = MaterialInputs {
            height: Some(&altura),
            ..entradas(&COLOR, Some(&ROUGH), Some(&METAL), Some(&NORMAL))
        };
        let maps = engine_maps(EnginePreset::Blender, &m);
        assert_eq!(
            sufijos(&maps),
            ["BaseColor", "Metallic", "Roughness", "Normal", "Height"]
        );
        assert_eq!(mapa(&maps, "Roughness"), &[200, 200, 200, 255]);
        assert_eq!(mapa(&maps, "Height"), &[90, 90, 90, 255]);
    }

    #[test]
    fn cada_motor_tiene_su_carpeta_y_ninguna_se_repite() {
        let mut tags: Vec<&str> = EnginePreset::ALL.iter().map(|p| p.folder_tag()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), EnginePreset::ALL.len());
    }

    #[test]
    fn free_dir_no_pisa_carpetas_y_write_maps_guarda_cada_mapa() {
        let parent = std::env::temp_dir().join("texturecraft_prueba_motores");
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).unwrap();
        let dir = free_dir(&parent, "modelo_unreal");
        assert_eq!(dir, parent.join("modelo_unreal"));
        let img = vec![100u8; 2 * 2 * 4];
        let maps = [
            ExportMap { suffix: "BaseColor", rgba: img.clone() },
            ExportMap { suffix: "Normal", rgba: img },
        ];
        let files = write_maps(&dir, "modelo", 2, &maps).unwrap();
        assert_eq!(files.len(), 2);
        assert!(dir.join("modelo_BaseColor.png").exists());
        assert!(dir.join("modelo_Normal.png").exists());
        // Ya existe: el siguiente nombre libre lleva numero.
        assert_eq!(free_dir(&parent, "modelo_unreal"), parent.join("modelo_unreal_1"));
        let _ = std::fs::remove_dir_all(&parent);
    }

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
