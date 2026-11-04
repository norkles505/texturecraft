//! Modo edicion de caras (como el modo edicion de Blender): elegir caras con clic, ocultarlas y mostrarlas.
//!
//! Este modulo solo lleva la cuenta (que caras estan elegidas y cuales ocultas); no sabe nada de la GPU. `main.rs` le
//! pregunta que triangulos se ven (`tri_visible`) y que bandera lleva cada vertice (`vertex_flags`).
//!
//! Teclas (en `main.rs`): Tab = entrar/salir, clic = elegir una cara, Mayus+clic = sumar o quitar una, Alt+clic = bucle
//! de caras (fila o columna), Ctrl+clic = camino mas corto desde la ultima cara tocada (Mayus lo suma a lo elegido),
//! A = todas, Alt+A = ninguna, Ctrl+I = invertir, H = ocultar las elegidas, Mayus+H = ocultar las NO elegidas
//! (aislar), Alt+H = mostrar todas.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::obj::ObjMesh;

/// Una arista identificada por la posicion de sus dos puntas (en orden), para juntar las que se repiten.
type EdgeKey = ([i32; 3], [i32; 3]);

/// Una arista del borde de una cara: su llave y sus dos puntas.
type BoundaryEdge = (EdgeKey, [[f32; 3]; 2]);

/// Llave de un punto: la posicion redondeada a 1e-4 (el modelo mide ~4.5), asi dos vertices que son el mismo punto con
/// una diferencia minuscula (la costura de una esfera, por ejemplo) cuentan como uno solo.
fn pos_key(p: [f32; 3]) -> [i32; 3] {
    [
        (p[0] * 1e4).round() as i32,
        (p[1] * 1e4).round() as i32,
        (p[2] * 1e4).round() as i32,
    ]
}

/// Distancia al cuadrado del punto `p` al segmento `seg`.
fn seg_dist2(seg: [[f32; 3]; 2], p: [f32; 3]) -> f32 {
    let (a, b) = (seg[0], seg[1]);
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ap = [p[0] - a[0], p[1] - a[1], p[2] - a[2]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    let t = if len2 > 0.0 {
        ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t, ap[2] - ab[2] * t];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// Aristas del borde de cada cara y, para cada arista, las caras que la tienen. Una arista del borde de una cara
/// aparece en un solo triangulo de ella; las de adentro (la diagonal de un cuadrado), en dos.
fn boundary_edges(mesh: &ObjMesh) -> (Vec<Vec<BoundaryEdge>>, HashMap<EdgeKey, Vec<u32>>) {
    let mut face_edges: Vec<Vec<BoundaryEdge>> = vec![Vec::new(); mesh.face_count()];
    let mut edge_faces: HashMap<EdgeKey, Vec<u32>> = HashMap::new();
    let tri_pos = |t: usize| -> [[f32; 3]; 3] {
        let at = |k: usize| {
            let i = (t * 3 + k) * 3;
            [
                mesh.positions[i],
                mesh.positions[i + 1],
                mesh.positions[i + 2],
            ]
        };
        [at(0), at(1), at(2)]
    };
    let n = mesh.face_of_tri.len();
    let mut t = 0;
    while t < n {
        let face = mesh.face_of_tri[t];
        let mut end = t;
        while end < n && mesh.face_of_tri[end] == face {
            end += 1;
        }
        let mut local: HashMap<EdgeKey, ([[f32; 3]; 2], u32)> = HashMap::new();
        for tri in t..end {
            let p = tri_pos(tri);
            for k in 0..3 {
                let (a, b) = (p[k], p[(k + 1) % 3]);
                let (ka, kb) = (pos_key(a), pos_key(b));
                if ka == kb {
                    continue;
                }
                let key = if ka < kb { (ka, kb) } else { (kb, ka) };
                local.entry(key).or_insert(([a, b], 0)).1 += 1;
            }
        }
        for (key, (seg, count)) in local {
            if count == 1 {
                face_edges[face as usize].push((key, seg));
                edge_faces.entry(key).or_default().push(face);
            }
        }
        t = end;
    }
    (face_edges, edge_faces)
}

/// Bandera por vertice que lee el shader: 0 = normal, 1 = cara elegida (se tine), 2 = cara oculta (no se dibuja).
pub const FLAG_NORMAL: f32 = 0.0;
pub const FLAG_SELECTED: f32 = 1.0;
pub const FLAG_HIDDEN: f32 = 2.0;

pub struct FaceEdit {
    /// Dentro del modo edicion (Tab). Las caras ocultas siguen ocultas al salir.
    pub active: bool,
    face_of_tri: Vec<u32>,
    selected: Vec<bool>,
    hidden: Vec<bool>,
    /// Aristas del borde de cada cara (para el bucle de caras de Alt+clic).
    face_edges: Vec<Vec<BoundaryEdge>>,
    /// Caras que tienen cada arista como borde (dos = la arista une dos caras).
    edge_faces: HashMap<EdgeKey, Vec<u32>>,
    /// Caras vecinas de cada cara (las que comparten una arista con ella), en orden, para el camino mas corto.
    adjacency: Vec<Vec<usize>>,
    /// Ultima cara tocada con clic (la "activa" de Blender): el camino de Ctrl+clic sale de ella.
    last_face: Option<usize>,
}

impl FaceEdit {
    pub fn new(mesh: &ObjMesh) -> FaceEdit {
        let faces = mesh.face_count();
        let (face_edges, edge_faces) = boundary_edges(mesh);
        // Dos caras son vecinas si una arista del borde une solo a las dos. Se ordenan las listas para que el camino
        // mas corto sea siempre el mismo (el HashMap recorre en cualquier orden).
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); faces];
        for fs in edge_faces.values() {
            if let [a, b] = fs.as_slice() {
                let (a, b) = (*a as usize, *b as usize);
                if a != b {
                    adjacency[a].push(b);
                    adjacency[b].push(a);
                }
            }
        }
        for list in adjacency.iter_mut() {
            list.sort_unstable();
            list.dedup();
        }
        FaceEdit {
            active: false,
            face_of_tri: mesh.face_of_tri.clone(),
            selected: vec![false; faces],
            hidden: vec![false; faces],
            face_edges,
            edge_faces,
            adjacency,
            last_face: None,
        }
    }

    pub fn is_selected(&self, face: usize) -> bool {
        self.selected.get(face).copied().unwrap_or(false)
    }

    /// Si la cara es un cuadrado (4 aristas en el borde): solo por ellos pasa un bucle de caras.
    fn is_quad(&self, face: usize) -> bool {
        self.face_edges[face].len() == 4
    }

    /// La cara al otro lado de `edge` visto desde `face`; `None` si la arista es del borde del modelo (una sola cara) o
    /// une mas de dos caras.
    fn neighbor(&self, face: usize, edge: EdgeKey) -> Option<usize> {
        match self.edge_faces.get(&edge) {
            Some(v) if v.len() == 2 => v.iter().map(|&f| f as usize).find(|&f| f != face),
            _ => None,
        }
    }

    /// La arista de `face` (un cuadrado) opuesta a `edge`: la que no comparte ninguna punta con ella.
    fn opposite(&self, face: usize, edge: EdgeKey) -> Option<EdgeKey> {
        self.face_edges[face]
            .iter()
            .map(|(k, _)| *k)
            .find(|k| k.0 != edge.0 && k.0 != edge.1 && k.1 != edge.0 && k.1 != edge.1)
    }

    /// La arista de `face` mas cercana al punto `p`.
    fn nearest_edge(&self, face: usize, p: [f32; 3]) -> Option<EdgeKey> {
        self.face_edges[face]
            .iter()
            .min_by(|a, b| seg_dist2(a.1, p).total_cmp(&seg_dist2(b.1, p)))
            .map(|(k, _)| *k)
    }

    /// Sigue el bucle desde `start` cruzando `edge`, cara tras cara por las aristas opuestas. Se detiene en el borde del
    /// modelo, en una cara que no es cuadrada, en una oculta o en una ya recorrida. Devuelve true si dio la vuelta y
    /// volvio a `start`.
    fn walk_loop(
        &self,
        start: usize,
        mut edge: EdgeKey,
        out: &mut Vec<usize>,
        seen: &mut HashSet<usize>,
    ) -> bool {
        let mut face = start;
        loop {
            let Some(next) = self.neighbor(face, edge) else {
                return false;
            };
            if next == start {
                return true;
            }
            if seen.contains(&next) || self.hidden[next] || !self.is_quad(next) {
                return false;
            }
            seen.insert(next);
            out.push(next);
            let Some(opp) = self.opposite(next, edge) else {
                return false;
            };
            edge = opp;
            face = next;
        }
    }

    /// Bucle de caras (Alt+clic de Blender): desde `face` cruza la arista mas cercana al punto `point` y sigue por las
    /// aristas opuestas, hacia los dos lados, hasta dar la vuelta o toparse con algo que no es un cuadrado. Siempre
    /// incluye a `face`. Si `face` no es un cuadrado, solo ella.
    pub fn face_loop(&self, face: usize, point: [f32; 3]) -> Vec<usize> {
        let mut out = vec![face];
        if !self.is_quad(face) {
            return out;
        }
        let Some(e0) = self.nearest_edge(face, point) else {
            return out;
        };
        let mut seen: HashSet<usize> = HashSet::from([face]);
        let closed = self.walk_loop(face, e0, &mut out, &mut seen);
        if !closed {
            if let Some(e1) = self.opposite(face, e0) {
                self.walk_loop(face, e1, &mut out, &mut seen);
            }
        }
        out
    }

    /// Alt+clic: elige el bucle de caras que pasa por la cara tocada (`tri`), con la arista mas cercana a `point`
    /// (el punto 3D del clic). Sin `extend` reemplaza la seleccion; con `extend` (Mayus) suma el bucle a la actual.
    pub fn click_loop(&mut self, tri: Option<usize>, point: Option<[f32; 3]>, extend: bool) {
        let face = tri.and_then(|t| self.face_of(t));
        if !extend {
            self.selected.fill(false);
        }
        let (Some(f), Some(p)) = (face, point) else {
            return;
        };
        if self.hidden[f] {
            return;
        }
        for g in self.face_loop(f, p) {
            self.selected[g] = true;
        }
        self.last_face = Some(f);
    }

    /// Camino mas corto entre dos caras, pasando de una cara a otra por las aristas que comparten (busqueda en
    /// anchura). Incluye las dos puntas. Las caras ocultas no se cruzan. `None` si no hay camino (partes separadas del
    /// modelo, o una cara oculta de por medio).
    pub fn face_path(&self, from: usize, to: usize) -> Option<Vec<usize>> {
        if from >= self.hidden.len() || to >= self.hidden.len() {
            return None;
        }
        if self.hidden[from] || self.hidden[to] {
            return None;
        }
        if from == to {
            return Some(vec![from]);
        }
        let mut prev: Vec<Option<usize>> = vec![None; self.hidden.len()];
        let mut seen = vec![false; self.hidden.len()];
        seen[from] = true;
        let mut queue = VecDeque::from([from]);
        while let Some(f) = queue.pop_front() {
            for &g in &self.adjacency[f] {
                if seen[g] || self.hidden[g] {
                    continue;
                }
                seen[g] = true;
                prev[g] = Some(f);
                if g == to {
                    let mut path = vec![to];
                    let mut cur = to;
                    while let Some(p) = prev[cur] {
                        path.push(p);
                        cur = p;
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back(g);
            }
        }
        None
    }

    /// Ctrl+clic (camino mas corto de Blender): elige todas las caras del camino desde la ultima cara tocada hasta la
    /// cara del triangulo `tri`. Sin `extend` reemplaza la seleccion; con `extend` (Mayus) suma el camino a la actual.
    /// Sin cara anterior, o sin camino hasta ella, elige solo la cara tocada. Un clic en el vacio no hace nada.
    pub fn click_path(&mut self, tri: Option<usize>, extend: bool) {
        let Some(f) = tri.and_then(|t| self.face_of(t)) else {
            return;
        };
        if self.hidden[f] {
            return;
        }
        let path = self
            .last_face
            .and_then(|a| self.face_path(a, f))
            .unwrap_or_else(|| vec![f]);
        if !extend {
            self.selected.fill(false);
        }
        for g in path {
            self.selected[g] = true;
        }
        self.last_face = Some(f);
    }

    pub fn face_of(&self, tri: usize) -> Option<usize> {
        self.face_of_tri.get(tri).map(|&f| f as usize)
    }

    pub fn selected_count(&self) -> usize {
        self.selected.iter().filter(|&&s| s).count()
    }

    pub fn hidden_count(&self) -> usize {
        self.hidden.iter().filter(|&&h| h).count()
    }

    /// Clic en la cara que tiene el triangulo `tri` (`None` = clic en el vacio). Sin `extend` reemplaza la seleccion
    /// (clic en el vacio la limpia); con `extend` (Mayus) suma la cara o, si ya estaba elegida, la quita.
    pub fn click(&mut self, tri: Option<usize>, extend: bool) {
        let face = tri.and_then(|t| self.face_of(t));
        if !extend {
            self.selected.fill(false);
        }
        if let Some(f) = face {
            if !self.hidden[f] {
                self.selected[f] = if extend { !self.selected[f] } else { true };
                self.last_face = Some(f);
            }
        } else if !extend {
            self.last_face = None;
        }
    }

    /// A: elige todas las caras que no estan ocultas.
    pub fn select_all(&mut self) {
        for (s, &h) in self.selected.iter_mut().zip(&self.hidden) {
            *s = !h;
        }
    }

    /// Alt+A: no queda ninguna elegida.
    pub fn deselect_all(&mut self) {
        self.selected.fill(false);
    }

    /// Ctrl+I: invierte la seleccion entre las caras que no estan ocultas.
    pub fn invert(&mut self) {
        for (s, &h) in self.selected.iter_mut().zip(&self.hidden) {
            *s = !h && !*s;
        }
    }

    /// H: oculta las caras elegidas.
    pub fn hide_selected(&mut self) {
        for (s, h) in self.selected.iter_mut().zip(self.hidden.iter_mut()) {
            if *s {
                *h = true;
                *s = false;
            }
        }
    }

    /// Mayus+H: oculta todas las caras que NO estan elegidas, o sea deja visibles solo las elegidas.
    pub fn hide_unselected(&mut self) {
        for (s, h) in self.selected.iter().zip(self.hidden.iter_mut()) {
            if !*s {
                *h = true;
            }
        }
    }

    /// Alt+H: muestra todas las caras ocultas (quedan elegidas, como en Blender). Devuelve true si habia alguna.
    pub fn reveal_all(&mut self) -> bool {
        let any = self.hidden.iter().any(|&h| h);
        for (s, h) in self.selected.iter_mut().zip(self.hidden.iter_mut()) {
            if *h {
                *h = false;
                *s = true;
            }
        }
        any
    }

    /// Si el triangulo `tri` se ve (su cara no esta oculta).
    pub fn tri_visible(&self, tri: usize) -> bool {
        self.face_of(tri).is_none_or(|f| !self.hidden[f])
    }

    /// Bandera de cada vertice de la malla (3 por triangulo) para la GPU. La seleccion solo se tine dentro del modo.
    pub fn vertex_flags(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.face_of_tri.len() * 3);
        for &f in &self.face_of_tri {
            let f = f as usize;
            let flag = if self.hidden[f] {
                FLAG_HIDDEN
            } else if self.active && self.selected[f] {
                FLAG_SELECTED
            } else {
                FLAG_NORMAL
            };
            out.extend_from_slice(&[flag; 3]);
        }
        out
    }

    /// Aristas para dibujar en el modo edicion, como lineas sueltas: 2 vertices por arista, y cada vertice son 4 floats
    /// `x y z flag` (flag 1 = arista de una cara elegida, 0 = las demas). Solo salen las aristas del borde de cada cara
    /// (la diagonal que parte un cuadrado en dos triangulos no se dibuja) y solo de los triangulos que `visible` deja
    /// pasar (las caras ocultas no dibujan aristas). `tri_pos(t)` da las tres posiciones del triangulo `t`.
    pub fn edge_lines(
        &self,
        tri_pos: impl Fn(usize) -> [[f32; 3]; 3],
        visible: impl Fn(usize) -> bool,
    ) -> Vec<f32> {
        // Aristas ya decididas: puntas y si alguna cara vecina esta elegida.
        let mut edges: HashMap<EdgeKey, ([[f32; 3]; 2], bool)> = HashMap::new();
        let n = self.face_of_tri.len();
        let mut t = 0;
        while t < n {
            // Los triangulos de una cara van seguidos en la malla.
            let face = self.face_of_tri[t];
            let mut end = t;
            while end < n && self.face_of_tri[end] == face {
                end += 1;
            }
            // Una arista del borde de la cara aparece en un solo triangulo de ella; las de adentro, en dos.
            let mut local: HashMap<EdgeKey, ([[f32; 3]; 2], u32)> = HashMap::new();
            for tri in t..end {
                if !visible(tri) {
                    continue;
                }
                let p = tri_pos(tri);
                for k in 0..3 {
                    let (a, b) = (p[k], p[(k + 1) % 3]);
                    let (ka, kb) = (pos_key(a), pos_key(b));
                    if ka == kb {
                        continue;
                    }
                    let key = if ka < kb { (ka, kb) } else { (kb, ka) };
                    local.entry(key).or_insert(([a, b], 0)).1 += 1;
                }
            }
            let selected = self.selected[face as usize];
            for (key, (seg, count)) in local {
                if count == 1 {
                    edges.entry(key).or_insert((seg, false)).1 |= selected;
                }
            }
            t = end;
        }
        let mut out = Vec::with_capacity(edges.len() * 8);
        for (_, (seg, sel)) in edges {
            let w = if sel { 1.0 } else { 0.0 };
            out.extend_from_slice(&[
                seg[0][0], seg[0][1], seg[0][2], w, seg[1][0], seg[1][1], seg[1][2], w,
            ]);
        }
        out
    }

    /// Texto para el titulo de la ventana dentro del modo.
    pub fn status(&self) -> String {
        format!(
            "EDICION DE CARAS: {} elegidas, {} ocultas (clic = elegir, Mayus+clic = sumar, Alt+clic = fila, Ctrl+clic = camino, A / Alt+A / Ctrl+I, H = ocultar, Mayus+H = aislar, Alt+H = mostrar, Tab = salir)",
            self.selected_count(),
            self.hidden_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Esfera de 8x4: 32 caras de 2 triangulos (triangulos 2k y 2k+1 son la cara k).
    fn sphere_edit() -> FaceEdit {
        FaceEdit::new(&ObjMesh::uv_sphere(8, 4))
    }

    #[test]
    fn el_clic_elige_la_cara_completa_y_reemplaza() {
        let mut e = sphere_edit();
        e.click(Some(5), false); // triangulo 5 -> cara 2
        assert_eq!(e.selected_count(), 1);
        e.click(Some(20), false); // otra cara: reemplaza
        assert_eq!(e.selected_count(), 1);
        e.click(None, false); // vacio: limpia
        assert_eq!(e.selected_count(), 0);
    }

    #[test]
    fn mayus_suma_y_quita() {
        let mut e = sphere_edit();
        e.click(Some(0), false);
        e.click(Some(10), true);
        assert_eq!(e.selected_count(), 2);
        e.click(Some(11), true); // misma cara que el 10: se quita
        assert_eq!(e.selected_count(), 1);
        e.click(None, true); // vacio con Mayus no limpia
        assert_eq!(e.selected_count(), 1);
    }

    #[test]
    fn todas_ninguna_e_invertir() {
        let mut e = sphere_edit();
        e.select_all();
        assert_eq!(e.selected_count(), 32);
        e.deselect_all();
        assert_eq!(e.selected_count(), 0);
        e.click(Some(0), false);
        e.invert();
        assert_eq!(e.selected_count(), 31);
    }

    #[test]
    fn ocultar_elegidas_las_saca_de_la_seleccion_y_de_la_vista() {
        let mut e = sphere_edit();
        e.click(Some(0), false);
        e.hide_selected();
        assert_eq!(e.hidden_count(), 1);
        assert_eq!(e.selected_count(), 0);
        assert!(!e.tri_visible(0) && !e.tri_visible(1));
        assert!(e.tri_visible(2));
        // Una cara oculta no se puede elegir ni entra en "todas".
        e.click(Some(0), false);
        assert_eq!(e.selected_count(), 0);
        e.select_all();
        assert_eq!(e.selected_count(), 31);
    }

    #[test]
    fn mayus_h_deja_solo_las_elegidas() {
        let mut e = sphere_edit();
        e.click(Some(0), false);
        e.click(Some(4), true);
        e.hide_unselected();
        assert_eq!(e.hidden_count(), 30);
        assert_eq!(e.selected_count(), 2);
        assert!(e.tri_visible(0) && e.tri_visible(4));
        assert!(!e.tri_visible(2));
    }

    #[test]
    fn alt_h_muestra_todo_y_lo_deja_elegido() {
        let mut e = sphere_edit();
        e.click(Some(0), false);
        e.hide_unselected();
        assert!(e.reveal_all());
        assert_eq!(e.hidden_count(), 0);
        // Las 31 que volvieron quedan elegidas, y la que nunca se oculto sigue elegida: las 32.
        assert_eq!(e.selected_count(), 32);
        assert!(!e.reveal_all()); // ya no hay ocultas
    }

    /// Las tres posiciones de cada triangulo de `mesh`, para `edge_lines`.
    fn tri_pos(mesh: &ObjMesh) -> impl Fn(usize) -> [[f32; 3]; 3] + '_ {
        move |t| {
            let at = |k: usize| {
                let i = (t * 3 + k) * 3;
                [
                    mesh.positions[i],
                    mesh.positions[i + 1],
                    mesh.positions[i + 2],
                ]
            };
            [at(0), at(1), at(2)]
        }
    }

    #[test]
    fn un_cuadrado_dibuja_cuatro_aristas_sin_la_diagonal() {
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n";
        let mesh = crate::obj::load_obj(obj, 0.0, false).unwrap();
        let e = FaceEdit::new(&mesh);
        let lines = e.edge_lines(tri_pos(&mesh), |_| true);
        // 4 aristas x 2 vertices x 4 floats.
        assert_eq!(lines.len(), 4 * 8);
    }

    #[test]
    fn dos_cuadrados_juntos_comparten_una_arista_y_la_seleccion_la_tine() {
        let obj = "v 0 0 0\nv 1 0 0\nv 2 0 0\nv 0 1 0\nv 1 1 0\nv 2 1 0\nf 1 2 5 4\nf 2 3 6 5\n";
        let mesh = crate::obj::load_obj(obj, 0.0, false).unwrap();
        let mut e = FaceEdit::new(&mesh);
        let lines = e.edge_lines(tri_pos(&mesh), |_| true);
        assert_eq!(lines.len(), 7 * 8);
        // Sin nada elegido, ninguna arista lleva la bandera.
        assert!(lines.chunks(4).all(|v| v[3] == 0.0));
        // Elegida la primera cara: sus 4 aristas (la compartida incluida) van marcadas.
        e.click(Some(0), false);
        let lines = e.edge_lines(tri_pos(&mesh), |_| true);
        assert_eq!(lines.chunks(8).filter(|s| s[3] == 1.0).count(), 4);
        // Una cara oculta no dibuja aristas: queda solo el cuadrado de la primera cara.
        let lines = e.edge_lines(tri_pos(&mesh), |t| t < 2);
        assert_eq!(lines.len(), 4 * 8);
    }

    /// Cuadricula de `n` x `m` cuadrados en el plano XY (cuadrado (i, j) = cara j * n + i, ocupa x en [i, i+1] e y en
    /// [j, j+1]); cada cuadrado son 2 triangulos, asi la cara `k` tiene los triangulos 2k y 2k+1.
    fn grid(n: usize, m: usize) -> ObjMesh {
        let mut obj = String::new();
        for j in 0..=m {
            for i in 0..=n {
                obj.push_str(&format!("v {i} {j} 0\n"));
            }
        }
        let v = |i: usize, j: usize| j * (n + 1) + i + 1;
        for j in 0..m {
            for i in 0..n {
                obj.push_str(&format!(
                    "f {} {} {} {}\n",
                    v(i, j),
                    v(i + 1, j),
                    v(i + 1, j + 1),
                    v(i, j + 1)
                ));
            }
        }
        crate::obj::load_obj(&obj, 0.0, false).unwrap()
    }

    fn elegidas(e: &FaceEdit) -> Vec<usize> {
        (0..e.face_of_tri.len())
            .filter_map(|t| e.face_of(t))
            .filter(|&f| e.is_selected(f))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    #[test]
    fn alt_clic_cerca_de_un_lado_elige_la_columna_o_la_fila() {
        let mesh = grid(3, 3);
        let mut e = FaceEdit::new(&mesh);
        // Cara del centro (4, triangulos 8 y 9): cerca de su lado de abajo cruza hacia arriba y abajo = columna 1.
        e.click_loop(Some(8), Some([1.5, 1.05, 0.0]), false);
        assert_eq!(elegidas(&e), vec![1, 4, 7]);
        // Cerca de su lado izquierdo cruza hacia los lados = fila 1.
        e.click_loop(Some(8), Some([1.05, 1.5, 0.0]), false);
        assert_eq!(elegidas(&e), vec![3, 4, 5]);
    }

    #[test]
    fn mayus_alt_clic_suma_el_bucle_a_lo_elegido() {
        let mesh = grid(3, 3);
        let mut e = FaceEdit::new(&mesh);
        e.click_loop(Some(8), Some([1.5, 1.05, 0.0]), false); // columna 1
        e.click_loop(Some(0), Some([0.05, 0.5, 0.0]), true); // fila 0 (cara 0, cerca del lado izquierdo)
        assert_eq!(elegidas(&e), vec![0, 1, 2, 4, 7]);
    }

    #[test]
    fn un_bucle_cerrado_da_la_vuelta_completa() {
        // Tubo abierto de 4 caras alrededor: el bucle que cruza sus lados verticales cierra el anillo.
        let obj = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 0 0 1\nv 1 0 1\nv 1 1 1\nv 0 1 1\n\
                   f 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n";
        let mesh = crate::obj::load_obj(obj, 0.0, false).unwrap();
        let mut e = FaceEdit::new(&mesh);
        e.click_loop(Some(0), Some([0.05, 0.0, 0.5]), false);
        assert_eq!(elegidas(&e), vec![0, 1, 2, 3]);
    }

    #[test]
    fn el_bucle_se_detiene_en_las_caras_ocultas() {
        let mesh = grid(3, 3);
        let mut e = FaceEdit::new(&mesh);
        e.click(Some(14), false); // cara 7 (la de arriba de la columna 1)
        e.hide_selected();
        e.click_loop(Some(8), Some([1.5, 1.05, 0.0]), false);
        assert_eq!(elegidas(&e), vec![1, 4]);
    }

    #[test]
    fn un_triangulo_no_forma_bucle() {
        let obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let mesh = crate::obj::load_obj(obj, 0.0, false).unwrap();
        let mut e = FaceEdit::new(&mesh);
        e.click_loop(Some(0), Some([0.5, 0.0, 0.0]), false);
        assert_eq!(elegidas(&e), vec![0]);
    }

    #[test]
    fn la_costura_de_la_esfera_cuenta_como_arista_compartida() {
        // En la esfera de prueba la ultima casilla de cada fila termina donde empieza la primera (con un error de
        // redondeo minusculo): la fila de casillas debe dar la vuelta entera (8 casillas).
        let mesh = ObjMesh::uv_sphere(8, 4);
        let mut e = FaceEdit::new(&mesh);
        // Casilla (anillo 1, segmento 0) = cara 8, triangulos 16 y 17. El punto medio de su lado izquierdo (el del
        // segmento 0, en el plano XZ, entre el anillo 1 y el anillo 2).
        let phi = std::f32::consts::PI / 4.0;
        let p = [0.5 * (phi.sin() + 1.0) / 2.0, 0.0, 0.5 * phi.cos() / 2.0];
        e.click_loop(Some(16), Some(p), false);
        assert_eq!(elegidas(&e).len(), 8);
    }

    #[test]
    fn ctrl_clic_elige_el_camino_mas_corto_entre_dos_esquinas() {
        let mesh = grid(4, 4);
        let mut e = FaceEdit::new(&mesh);
        e.click(Some(0), false); // cara 0 (esquina de abajo a la izquierda)
        e.click_path(Some(30), false); // cara 15 (esquina de arriba a la derecha)
        let sel = elegidas(&e);
        // 3 pasos a la derecha + 3 hacia arriba = 7 caras, con las dos puntas.
        assert_eq!(sel.len(), 7);
        assert!(sel.contains(&0) && sel.contains(&15));
        // El camino es continuo: cada cara elegida toca a otra elegida.
        for &f in &sel {
            assert!(e.adjacency[f].iter().any(|g| sel.contains(g)));
        }
    }

    #[test]
    fn ctrl_clic_reemplaza_y_mayus_ctrl_clic_suma() {
        let mesh = grid(3, 3);
        let mut e = FaceEdit::new(&mesh);
        e.click(Some(0), false);
        e.click_path(Some(4), false); // cara 2: camino 0, 1, 2
        assert_eq!(elegidas(&e), vec![0, 1, 2]);
        // Con Mayus suma el camino 2 -> 8 (2, 5, 8) a lo que ya estaba.
        e.click_path(Some(16), true);
        assert_eq!(elegidas(&e), vec![0, 1, 2, 5, 8]);
        // Sin Mayus reemplaza: solo el camino 8 -> 6 (8, 7, 6).
        e.click_path(Some(12), false);
        assert_eq!(elegidas(&e), vec![6, 7, 8]);
    }

    #[test]
    fn ctrl_clic_sin_cara_anterior_elige_solo_la_tocada() {
        let mesh = grid(3, 3);
        let mut e = FaceEdit::new(&mesh);
        e.click_path(Some(8), false);
        assert_eq!(elegidas(&e), vec![4]);
        // Un clic en el vacio no cambia nada.
        e.click_path(None, false);
        assert_eq!(elegidas(&e), vec![4]);
    }

    #[test]
    fn el_camino_rodea_las_caras_ocultas() {
        let mesh = grid(3, 3);
        let mut e = FaceEdit::new(&mesh);
        e.click(Some(8), false); // cara 4 (el centro)
        e.hide_selected();
        e.click(Some(6), false); // cara 3
        e.click_path(Some(10), false); // cara 5, al otro lado del hueco
        let sel = elegidas(&e);
        assert_eq!(sel.len(), 5);
        assert!(!sel.contains(&4));
        assert!(sel.contains(&3) && sel.contains(&5));
    }

    #[test]
    fn sin_camino_elige_solo_la_cara_tocada() {
        // Fila de 3 cuadrados con el del medio oculto: no hay camino de un extremo al otro.
        let mesh = grid(3, 1);
        let mut e = FaceEdit::new(&mesh);
        e.click(Some(2), false); // cara 1
        e.hide_selected();
        e.click(Some(0), false); // cara 0
        e.click_path(Some(4), false); // cara 2
        assert_eq!(elegidas(&e), vec![2]);
    }

    #[test]
    fn el_camino_cruza_la_costura_de_la_esfera() {
        // Casillas del anillo 1: la 8 (segmento 0) y la 15 (segmento 7) se tocan por la costura.
        let mesh = ObjMesh::uv_sphere(8, 4);
        let mut e = FaceEdit::new(&mesh);
        e.click(Some(16), false); // cara 8
        e.click_path(Some(30), false); // cara 15
        assert_eq!(elegidas(&e), vec![8, 15]);
    }

    #[test]
    fn las_banderas_siguen_a_las_caras_y_la_seleccion_solo_se_tine_en_el_modo() {
        let mut e = sphere_edit();
        e.click(Some(0), false);
        e.click(Some(2), true);
        e.hide_selected(); // caras 0 y 1 ocultas
        e.click(Some(4), false); // cara 2 elegida
        let fuera = e.vertex_flags();
        assert_eq!(fuera.len(), 32 * 2 * 3);
        assert_eq!(&fuera[0..6], &[FLAG_HIDDEN; 6]); // cara 0
        assert_eq!(&fuera[6..12], &[FLAG_HIDDEN; 6]); // cara 1
        assert_eq!(&fuera[12..18], &[FLAG_NORMAL; 6]); // cara 2: elegida pero fuera del modo
        e.active = true;
        let dentro = e.vertex_flags();
        assert_eq!(&dentro[12..18], &[FLAG_SELECTED; 6]);
        assert_eq!(&dentro[18..24], &[FLAG_NORMAL; 6]);
    }
}
