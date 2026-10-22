//! Malla y lector de OBJ. Portado de `ObjLoader.kt` (ver ESPECIFICACION.md, seccion 3).
//!
//! La malla queda triangulada y SIN indices: cada 3 vertices consecutivos son un triangulo.

use std::fmt;

/// Un tramo seguido de vertices (lineas `o`/`g` del OBJ). `first_vertex` y `vertex_count` son multiplos de 3.
#[derive(Debug, Clone)]
pub struct MeshPart {
    pub name: String,
    pub first_vertex: usize,
    pub vertex_count: usize,
    /// Una parte oculta no se dibuja ni se puede pintar (el rayo la atraviesa, seccion 3.1).
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct ObjMesh {
    /// x,y,z por vertice.
    pub positions: Vec<f32>,
    /// x,y,z por vertice.
    pub normals: Vec<f32>,
    /// u,v por vertice.
    pub uvs: Vec<f32>,
    /// false si el OBJ no trae UVs (entonces u,v = 0,0 y no se puede pintar bien).
    pub has_uvs: bool,
    /// Vacio si el OBJ no trae partes (o trae una sola).
    pub parts: Vec<MeshPart>,
}

impl ObjMesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    pub fn triangle_count(&self) -> usize {
        self.vertex_count() / 3
    }

    /// Las mallas del modelo tal como las muestra el panel de mallas: las partes del OBJ o, si no trae partes (o trae
    /// una sola), una sola malla llamada `fallback_name` que cubre todo el modelo. Todas empiezan visibles.
    pub fn scene_parts(&self, fallback_name: &str) -> Vec<MeshPart> {
        if !self.parts.is_empty() {
            return self.parts.clone();
        }
        vec![MeshPart {
            name: fallback_name.to_string(),
            first_vertex: 0,
            vertex_count: self.vertex_count(),
            visible: true,
        }]
    }

    /// Vertices entrelazados para la GPU: x y z nx ny nz u v (8 floats = 32 bytes por vertice).
    pub fn interleaved(&self) -> Vec<f32> {
        let n = self.vertex_count();
        let mut out = Vec::with_capacity(n * 8);
        for i in 0..n {
            out.extend_from_slice(&self.positions[i * 3..i * 3 + 3]);
            out.extend_from_slice(&self.normals[i * 3..i * 3 + 3]);
            out.extend_from_slice(&self.uvs[i * 2..i * 2 + 2]);
        }
        out
    }

    /// Esfera UV con Z arriba, para tener algo que ver sin cargar un OBJ. Mide 1.0 de diametro.
    pub fn uv_sphere(segments: usize, rings: usize) -> ObjMesh {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let radius = 0.5_f32;
        let point = |seg: usize, ring: usize| -> ([f32; 3], [f32; 2]) {
            let u = seg as f32 / segments as f32;
            let v = ring as f32 / rings as f32;
            let theta = u * std::f32::consts::TAU;
            let phi = v * std::f32::consts::PI; // 0 = polo de arriba
            let n = [phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos()];
            (n, [u, v])
        };
        let mut push = |seg: usize, ring: usize| {
            let (n, uv) = point(seg, ring);
            positions.extend_from_slice(&[n[0] * radius, n[1] * radius, n[2] * radius]);
            normals.extend_from_slice(&n);
            uvs.extend_from_slice(&uv);
        };
        for ring in 0..rings {
            for seg in 0..segments {
                // Dos triangulos por casilla, con el mismo sentido de giro.
                push(seg, ring);
                push(seg, ring + 1);
                push(seg + 1, ring);
                push(seg + 1, ring);
                push(seg, ring + 1);
                push(seg + 1, ring + 1);
            }
        }
        ObjMesh {
            positions,
            normals,
            uvs,
            has_uvs: true,
            parts: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct ObjError(pub String);

impl fmt::Display for ObjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ObjError {}

fn err<T>(line_no: usize, msg: &str) -> Result<T, ObjError> {
    Err(ObjError(format!("Linea {line_no}: {msg}")))
}

/// Indice de OBJ (base 1, o negativo = relativo al final) a indice base 0.
fn resolve(idx: i64, count: usize, line_no: usize) -> Result<usize, ObjError> {
    let r = if idx > 0 { idx - 1 } else { count as i64 + idx };
    if r < 0 || r >= count as i64 {
        return err(line_no, "indice fuera de rango");
    }
    Ok(r as usize)
}

/// Carga un OBJ desde texto.
///
/// * `fit_size > 0`: centra el modelo en el origen y lo escala para que su lado mas largo mida `fit_size`.
/// * `y_up = true`: el OBJ viene con Y arriba (estandar) y se convierte a Z arriba: `(x, y, z) -> (x, -z, y)`.
pub fn load_obj(text: &str, fit_size: f32, y_up: bool) -> Result<ObjMesh, ObjError> {
    let mut pos: Vec<f32> = Vec::new();
    let mut tex: Vec<f32> = Vec::new();
    let mut nor: Vec<f32> = Vec::new();
    let mut out_p: Vec<f32> = Vec::new();
    let mut out_n: Vec<f32> = Vec::new();
    let mut out_t: Vec<f32> = Vec::new();
    let mut any_uv = false;
    // En que vertice de salida empieza cada parte (lineas o/g) y su nombre.
    let mut part_starts: Vec<usize> = Vec::new();
    let mut part_names: Vec<String> = Vec::new();

    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts[0] {
            "o" | "g" => {
                let mut name = parts[1..].join(" ");
                if name.is_empty() {
                    name = format!("Parte {}", part_names.len() + 1);
                }
                part_starts.push(out_p.len() / 3);
                part_names.push(name);
            }
            "v" => {
                if parts.len() < 4 {
                    return err(line_no, "vertice incompleto");
                }
                let (vx, vy, vz) = (
                    num(parts[1], line_no)?,
                    num(parts[2], line_no)?,
                    num(parts[3], line_no)?,
                );
                if y_up {
                    pos.extend_from_slice(&[vx, -vz, vy]);
                } else {
                    pos.extend_from_slice(&[vx, vy, vz]);
                }
            }
            "vt" => {
                if parts.len() < 3 {
                    return err(line_no, "UV incompleta");
                }
                tex.push(num(parts[1], line_no)?);
                tex.push(num(parts[2], line_no)?);
            }
            "vn" => {
                if parts.len() < 4 {
                    return err(line_no, "normal incompleta");
                }
                let (nx, ny, nz) = (
                    num(parts[1], line_no)?,
                    num(parts[2], line_no)?,
                    num(parts[3], line_no)?,
                );
                if y_up {
                    nor.extend_from_slice(&[nx, -nz, ny]);
                } else {
                    nor.extend_from_slice(&[nx, ny, nz]);
                }
            }
            "f" => {
                let n = parts.len() - 1;
                if n < 3 {
                    return err(line_no, "cara con menos de 3 vertices");
                }
                let mut vi = vec![0usize; n];
                let mut ti = vec![-1i64; n];
                let mut ni = vec![-1i64; n];
                for k in 0..n {
                    let tok: Vec<&str> = parts[k + 1].split('/').collect();
                    vi[k] = resolve(int(tok[0], line_no)?, pos.len() / 3, line_no)? as usize;
                    if tok.len() > 1 && !tok[1].is_empty() {
                        ti[k] = resolve(int(tok[1], line_no)?, tex.len() / 2, line_no)? as i64;
                    }
                    if tok.len() > 2 && !tok[2].is_empty() {
                        ni[k] = resolve(int(tok[2], line_no)?, nor.len() / 3, line_no)? as i64;
                    }
                }
                // Abanico: (0, k, k+1).
                for k in 1..n - 1 {
                    let c = [0usize, k, k + 1];
                    // Normal plana por si algun vertice de este triangulo no trae normal propia.
                    let a = vi[c[0]] * 3;
                    let b = vi[c[1]] * 3;
                    let e = vi[c[2]] * 3;
                    let (ux, uy, uz) = (
                        pos[b] - pos[a],
                        pos[b + 1] - pos[a + 1],
                        pos[b + 2] - pos[a + 2],
                    );
                    let (wx, wy, wz) = (
                        pos[e] - pos[a],
                        pos[e + 1] - pos[a + 1],
                        pos[e + 2] - pos[a + 2],
                    );
                    let (mut fx, mut fy, mut fz) =
                        (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
                    let len = (fx * fx + fy * fy + fz * fz).sqrt();
                    if len > 0.0 {
                        fx /= len;
                        fy /= len;
                        fz /= len;
                    }
                    let all_normals = ni[c[0]] >= 0 && ni[c[1]] >= 0 && ni[c[2]] >= 0;
                    for corner in c {
                        let p = vi[corner] * 3;
                        out_p.extend_from_slice(&pos[p..p + 3]);
                        if all_normals {
                            let q = ni[corner] as usize * 3;
                            out_n.extend_from_slice(&nor[q..q + 3]);
                        } else {
                            out_n.extend_from_slice(&[fx, fy, fz]);
                        }
                        if ti[corner] >= 0 {
                            let t = ti[corner] as usize * 2;
                            out_t.extend_from_slice(&tex[t..t + 2]);
                            any_uv = true;
                        } else {
                            out_t.extend_from_slice(&[0.0, 0.0]);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if out_p.is_empty() {
        return Err(ObjError("El archivo no tiene caras (lineas f)".to_string()));
    }

    if fit_size > 0.0 {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in out_p.chunks_exact(3) {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }
        let center = [
            (min[0] + max[0]) / 2.0,
            (min[1] + max[1]) / 2.0,
            (min[2] + max[2]) / 2.0,
        ];
        let extent = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]);
        let s = if extent > 0.0 { fit_size / extent } else { 1.0 };
        for v in out_p.chunks_exact_mut(3) {
            for k in 0..3 {
                v[k] = (v[k] - center[k]) * s;
            }
        }
    }

    // Partes: cada linea o/g abre un tramo que llega hasta la siguiente. Los tramos vacios se descartan, y las caras que
    // vinieran antes de la primera linea forman una parte propia, asi las partes cubren toda la malla sin huecos.
    let total_vertices = out_p.len() / 3;
    let mut mesh_parts: Vec<MeshPart> = Vec::new();
    if !part_starts.is_empty() {
        if part_starts[0] > 0 {
            mesh_parts.push(MeshPart {
                name: "Sin nombre".to_string(),
                first_vertex: 0,
                vertex_count: part_starts[0],
                visible: true,
            });
        }
        for k in 0..part_starts.len() {
            let from = part_starts[k];
            let to = if k + 1 < part_starts.len() {
                part_starts[k + 1]
            } else {
                total_vertices
            };
            if to > from {
                mesh_parts.push(MeshPart {
                    name: part_names[k].clone(),
                    first_vertex: from,
                    vertex_count: to - from,
                    visible: true,
                });
            }
        }
    }
    // Con una sola parte no hay nada que separar.
    let parts = if mesh_parts.len() >= 2 {
        mesh_parts
    } else {
        Vec::new()
    };

    Ok(ObjMesh {
        positions: out_p,
        normals: out_n,
        uvs: out_t,
        has_uvs: any_uv,
        parts,
    })
}

fn num(s: &str, line_no: usize) -> Result<f32, ObjError> {
    s.parse::<f32>()
        .map_err(|_| ObjError(format!("Linea {line_no}: numero invalido '{s}'")))
}

fn int(s: &str, line_no: usize) -> Result<i64, ObjError> {
    s.parse::<i64>()
        .map_err(|_| ObjError(format!("Linea {line_no}: indice invalido '{s}'")))
}

/// Tramos de vertices que hay que dibujar: las partes visibles, juntando las que quedan seguidas en una sola llamada
/// (seccion 14.2).
pub fn visible_ranges(parts: &[MeshPart]) -> Vec<std::ops::Range<u32>> {
    let mut out: Vec<std::ops::Range<u32>> = Vec::new();
    for p in parts.iter().filter(|p| p.visible) {
        let (a, b) = (
            p.first_vertex as u32,
            (p.first_vertex + p.vertex_count) as u32,
        );
        match out.last_mut() {
            Some(last) if last.end == a => last.end = b,
            _ => out.push(a..b),
        }
    }
    out
}

/// Para cada triangulo, si pertenece a una parte visible (lo que el pincel puede tocar).
pub fn triangle_visibility(parts: &[MeshPart], triangle_count: usize) -> Vec<bool> {
    let mut vis = vec![false; triangle_count];
    for p in parts.iter().filter(|p| p.visible) {
        for t in p.first_vertex / 3..(p.first_vertex + p.vertex_count) / 3 {
            if let Some(v) = vis.get_mut(t) {
                *v = true;
            }
        }
    }
    vis
}

/// Maximo de piezas (seccion 3.3): con mas partes el modelo queda como una sola malla con una sola textura.
pub const MAX_PIECES: usize = 12;

/// Tramos `(primer vertice, cantidad)` de cada pieza: una por parte si el modelo tiene de 2 a `MAX_PIECES` partes; si
/// no, una sola pieza que cubre todo el modelo.
pub fn piece_ranges(parts: &[MeshPart]) -> Vec<(usize, usize)> {
    if (2..=MAX_PIECES).contains(&parts.len()) {
        return parts
            .iter()
            .map(|p| (p.first_vertex, p.vertex_count))
            .collect();
    }
    let total = parts.last().map_or(0, |p| p.first_vertex + p.vertex_count);
    vec![(0, total)]
}

/// Posicion en `parts` de la parte a la que pertenece el triangulo `tri`.
pub fn part_of_triangle(parts: &[MeshPart], tri: usize) -> usize {
    let v = tri * 3;
    parts
        .iter()
        .position(|p| v >= p.first_vertex && v < p.first_vertex + p.vertex_count)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, first_tri: usize, tris: usize, visible: bool) -> MeshPart {
        MeshPart {
            name: name.to_string(),
            first_vertex: first_tri * 3,
            vertex_count: tris * 3,
            visible,
        }
    }

    #[test]
    fn los_tramos_visibles_seguidos_se_juntan() {
        let parts = [
            part("a", 0, 2, true),
            part("b", 2, 3, true),
            part("c", 5, 1, false),
            part("d", 6, 4, true),
        ];
        assert_eq!(visible_ranges(&parts), vec![0..15, 18..30]);
    }

    #[test]
    fn con_todo_oculto_no_se_dibuja_nada() {
        let parts = [part("a", 0, 2, false), part("b", 2, 3, false)];
        assert!(visible_ranges(&parts).is_empty());
        assert_eq!(triangle_visibility(&parts, 5), vec![false; 5]);
    }

    #[test]
    fn la_visibilidad_por_triangulo_sigue_a_las_partes() {
        let parts = [
            part("a", 0, 2, true),
            part("b", 2, 1, false),
            part("c", 3, 2, true),
        ];
        assert_eq!(
            triangle_visibility(&parts, 5),
            vec![true, true, false, true, true]
        );
    }

    #[test]
    fn un_modelo_sin_partes_es_una_sola_malla_visible() {
        let mesh = ObjMesh::uv_sphere(8, 4);
        let parts = mesh.scene_parts("Esfera");
        assert_eq!(parts.len(), 1);
        assert_eq!(
            (parts[0].first_vertex, parts[0].vertex_count),
            (0, mesh.vertex_count())
        );
        assert_eq!(parts[0].name, "Esfera");
        assert!(parts[0].visible);
    }

    #[test]
    fn el_lector_deja_las_partes_visibles() {
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\no A\nf 1 2 3\no B\nf 1 2 3\n";
        let mesh = load_obj(obj, 0.0, false).unwrap();
        assert_eq!(mesh.parts.len(), 2);
        assert!(mesh.parts.iter().all(|p| p.visible));
    }

    #[test]
    fn una_pieza_por_parte_de_2_a_12_partes() {
        let parts: Vec<MeshPart> = (0..3).map(|i| part("p", i * 2, 2, true)).collect();
        assert_eq!(piece_ranges(&parts), vec![(0, 6), (6, 6), (12, 6)]);
        let doce: Vec<MeshPart> = (0..MAX_PIECES).map(|i| part("p", i, 1, true)).collect();
        assert_eq!(piece_ranges(&doce).len(), MAX_PIECES);
    }

    #[test]
    fn con_una_parte_o_mas_de_12_es_una_sola_pieza() {
        let una = [part("a", 0, 4, true)];
        assert_eq!(piece_ranges(&una), vec![(0, 12)]);
        let trece: Vec<MeshPart> = (0..MAX_PIECES + 1).map(|i| part("p", i, 1, true)).collect();
        assert_eq!(piece_ranges(&trece), vec![(0, (MAX_PIECES + 1) * 3)]);
    }

    #[test]
    fn cada_triangulo_sabe_de_que_parte_es() {
        let parts = [
            part("a", 0, 2, true),
            part("b", 2, 1, true),
            part("c", 3, 2, true),
        ];
        let of: Vec<usize> = (0..5).map(|t| part_of_triangle(&parts, t)).collect();
        assert_eq!(of, vec![0, 0, 1, 2, 2]);
    }
}
