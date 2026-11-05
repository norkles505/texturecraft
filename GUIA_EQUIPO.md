# Guía del equipo: traer ideas de ArtCraft TextureCraft a nuestro programa, por fases

Esta guía explica, paso a paso, cómo estamos mejorando nuestro programa de texturas mirando cómo lo hace otro proyecto
parecido y con el mismo nombre (**ArtCraft TextureCraft**, también hecho en Rust). Sirve para que cualquiera del equipo
pueda seguir el trabajo, con o sin asistente de IA, sin depender de una sola persona.

Estado de esta guía: 5 de noviembre de 2026. El plan vivo está en `FASES.md` y el manual de uso de las capas en
`CAPAS.md`. Si algo de aquí contradice a esos dos archivos, mandan ellos.

En esta guía llamamos **NUESTRO** a este proyecto y **ARTCRAFT** al otro.

## 1. Qué hay que tener

- Rust y `cargo` instalados (`cargo --version` en una terminal debe responder).
- VS Code con la extensión `rust-analyzer` (marca los errores mientras escribes).
- NUESTRO proyecto abierto como carpeta principal en VS Code (la que tiene `Cargo.toml` y `src/`).
- Una copia de ARTCRAFT en otra carpeta del disco, **solo para leer**. Cuando haga falta mirarla, se agrega al mismo
  espacio de trabajo con *Archivo → Agregar carpeta al espacio de trabajo*. Nunca se edita.
- `git`, para guardar cada paso que funciona (un commit por paso).
- Opcional: un asistente de IA con acceso a VS Code (ver la sección 8).

## 2. Las reglas de oro

1. **Una cosa por vez.** Se termina, se prueba en pantalla y recién ahí se pasa a la siguiente. Apilar cambios sin
   probar es la forma más fácil de perder tiempo.
2. **Primero se comprueba si ya existe.** Dos veces nos pasó que "lo que faltaba" ya estaba hecho (ver la sección 6).
3. **Se aprende la idea y se reescribe con nuestras estructuras.** Los dos programas están en Rust, pero están
   organizados distinto: pegar código ajeno casi nunca compila ni encaja.
4. **Se respeta la licencia** (sección 3).
5. **Un solo autor por archivo a la vez.** Si dos personas, o dos sesiones de IA, escriben en el mismo archivo, se pisan.
   Antes de empezar: `git status` y `git diff`.
6. **Toda función nueva trae pruebas automáticas** (`cargo test`) **y una prueba en pantalla** (`cargo run`). Si solo se
   hizo la primera, en `FASES.md` se marca `[~]` (hecho, falta probar) y no `[x]`.
7. **Se anota lo que se hizo** en `FASES.md` (estado) y en `CAPAS.md` u otro manual (cómo se usa).
8. **Un commit por cada paso que funciona**, con un mensaje que diga qué cambió.

## 3. Licencia y créditos

- ARTCRAFT se publica con licencia **MIT o Apache-2.0** (a elección). Eso permite usar y adaptar su código si se
  conserva su aviso de derechos de autor y el texto de la licencia. El texto exacto está en los archivos `LICENSE` y
  `NOTICE` de su carpeta: se copia de ahí, no de esta guía.
- **Lo que no se copia nunca:** su logo, el nombre "ArtCraft", su mascota o ícono, y las fuentes e imágenes que traiga.
  Tienen licencias propias, listadas en su `ATTRIBUTION.md`.
- No se copia nada de programas comerciales (Substance Painter y similares): solo ideas generales de cómo funcionan.
- **Hasta hoy no se pegó código de ARTCRAFT en NUESTRO proyecto.** Se leyó cómo resuelve cada cosa y se escribió el
  código nuestro. Si algún día se adapta código suyo, se crea un `ATTRIBUTION.md` en nuestro proyecto (plantilla en la
  sección 9).
- NUESTRO proyecto aún no tiene una licencia decidida por el equipo. Conviene decidirla antes de compartirlo o venderlo.

## 4. El método, paso a paso (así se hace una fase)

Se repite igual para cada función nueva. Cada paso tiene una explicación **en simple**.

### Paso 1. Elegir una función
Se toma una de `FASES.md` (por ejemplo "simetría" o "deshacer capas"). Una función por vez.

*En simple:* no se empieza por lo más grande; se empieza por lo que da más ayuda con menos trabajo.

### Paso 2. Comprobar si NUESTRO programa ya la tiene
Se busca en nuestro código con *Ctrl+Mayús+F* de VS Code (o `grep -rniE "palabra" src` en la terminal), probando
palabras en español y en inglés: por ejemplo `simetr`, `symmetr`, `mirror`, `espejo`. Se lee también `FASES.md`, porque
otra persona pudo haberla hecho ya.

*En simple:* antes de construir una escalera, mira si ya hay una.

### Paso 3. Mirar cómo lo hace ARTCRAFT
- `docs/features.md` de ARTCRAFT lista todo lo que tiene (es su propia lista, escrita por ellos).
- Cada función vive en una carpeta dentro de `crates/<módulo>/src/`. Para encontrarla se busca en inglés con
  *Ctrl+Mayús+F* en su carpeta: `smudge`, `symmetry`, `blend`, `history`, `bake`...
- Se lee el código buscando **la idea**, no las líneas. Preguntas útiles: ¿qué datos necesita?, ¿qué calcula?,
  ¿dónde se engancha con el resto?, ¿qué casos raros contempla (bordes, capas ocultas, saltos)?

*En simple:* es como ver cómo otra persona armó un mueble para entender el truco, y luego armarlo con nuestras piezas.

### Paso 4. Decidir dónde se engancha en NUESTRO programa
Con el mapa de la sección 5 se elige el archivo y la función donde entra el cambio. Se busca **el cambio más chico que
funcione**. Antes de escribir, se anota en una frase qué se va a hacer y qué archivos se van a tocar.

### Paso 5. Escribir el código con nuestras estructuras
- Los comentarios del código van en español, sin tildes ni ñ (es la costumbre de nuestros archivos `.rs`).
- Si una función se parece a una que ya existe, se reutiliza su camino. Ejemplo: Difuminar y la línea recta con
  Mayús+clic pasan por el mismo camino de pintura que el pincel, y por eso respetan solos la simetría, el bloqueo alfa,
  las máscaras y el deshacer.
- Se escriben las **pruebas automáticas** junto con el código (en el módulo `#[cfg(test)]` del mismo archivo).

### Paso 6. Compilar y correr las pruebas
```
cargo test
```
Debe terminar con `test result: ok` y `0 failed`. Los `warning` (avisos amarillos) no son errores; los `error`, sí.
La primera vez tarda; las siguientes, menos.

### Paso 7. Probar en pantalla
```
cargo run
```
Se usa la función como la usaría una persona. La lista de comprobaciones está en la sección 7. Lo que se vea raro se
anota y se corrige antes de pasar a otra cosa.

### Paso 8. Dejar todo anotado
- `FASES.md`: se marca `[~]` (hecho, falta probar en pantalla) o `[x]` (hecho y probado), con una línea de dónde está.
- `CAPAS.md` (o el manual que toque): cómo se usa, en simple, y qué límites tiene.
- Si se adaptó código de ARTCRAFT: `ATTRIBUTION.md`.
- Un commit con un mensaje claro.

## 5. Mapa del código de NUESTRO programa

Todo está en `src/`. Antes de tocar un archivo, se lee el comentario de arriba: casi todos explican su propósito.

| Archivo | Para qué sirve |
|---|---|
| `main.rs` | Ventana, ratón y teclado, atajos, el trazo en curso (`stroke_begin`, `stroke_move`, `stroke_end`), deshacer y rehacer (`undo_redo`), y la parte que atiende lo que pide el panel de capas. Es el más grande. |
| `paint.rs` | Pintar sobre el modelo: `StrokeState`, el toque de pincel (`dab_surface`), Difuminar y Borrosidad (`dab_filter`) y el degradado. |
| `layers.rs` | Las capas: pila, máscaras, carpetas (`parent`), composición (`composite_all`, `composite_rect`), Modo Solo y los cambios de estructura que entran al historial (`LayoutEdit`). |
| `undo.rs` | Historial: baldosas de pixeles de cada trazo y pasos de estructura de las capas. |
| `blend.rs` | Los modos de mezcla (hoy 28). |
| `brush.rs` y `pinceles/` | Tipos de pincel; en `pinceles/` hay un archivo por tipo (crayón, carboncillo, tiza, etc.). |
| `fill.rs` | El relleno (balde). |
| `proyecto.rs` | Guardar y abrir proyectos (una carpeta con `proyecto.txt` y un PNG por cada hoja con pintura). |
| `export.rs` | Exportar la textura a PNG. |
| `obj.rs` | Leer modelos `.obj`. |
| `camera.rs` | La cámara y su movimiento. |
| `bvh.rs` | Encontrar rápido qué punto del modelo está bajo el cursor. |
| `uvmap.rs`, `face_edit.rs` | Mapa UV y modo de edición de caras. |
| `shader.wgsl` | El sombreador que dibuja el modelo con la textura. |
| `ui/` | Los paneles: `panel_capas.rs`, `barra_herramientas.rs`, `panel_reglas.rs` (reglas y simetría), `panel_pinceles.rs`, `panel_color.rs`, `panel_opciones.rs`, `panel_mallas.rs`, `galeria.rs`, `ventana_referencia.rs`... |

Para el resto de archivos, se abre y se lee el comentario del inicio.

### Dónde mirar en ARTCRAFT
- `README.md` y `docs/features.md`: qué tiene y cómo se ve.
- `ATTRIBUTION.md`: licencias de lo que trae (fuentes, imágenes, etc.).
- `AGENTS.md` y `CLAUDE.md`: sus reglas de trabajo para asistentes; vale la pena leerlos.
- `crates/<módulo>/src/`: el código, dividido por módulos (pintura, render, horneado, formatos...). Ejemplo ya usado:
  `crates/paint/src/pickup.rs` explica cómo hacen Difuminar.

## 6. Casos reales que ya hicimos (sirven de ejemplo)

### Caso 1. Difuminar y Borrosidad (eran solo botones)
- **Qué había:** los botones D y U existían, pero `stroke_begin` y `stroke_move` en `main.rs` salían sin hacer nada.
- **Qué se hizo:** una función nueva `StrokeState::dab_filter` en `paint.rs`, y en `main.rs` el toque elige entre pintar
  color o filtrar según la herramienta. Se quitaron los bloqueos y se muestra el círculo del pincel.
- **Borrosidad:** cada punto se mezcla con el promedio de sus vecinos (ventana de 7x7, `BLUR_REACH`).
- **Difuminar:** arrastra la imagen según cuánto se movió el centro del toque desde el anterior. La idea viene de
  `pickup.rs` de ARTCRAFT (tomar lo que había detrás); el código es nuestro.
- **Ajustes fáciles:** el multiplicador `gain` de cada modo (fuerza) y `BLUR_REACH` (cuánto desenfoca).

### Caso 2. Línea recta con Mayús+clic
- **Idea:** pintar un trazo, soltar, y con Mayús+clic en otro punto se traza una recta desde donde terminó el anterior.
- **Cómo:** `line_anchor` en `main.rs` recuerda dónde terminó el trazo y `view_key` resume la vista (cámara y tamaño de
  ventana) para saber si cambió: si cambió, ese punto de pantalla ya no es el mismo sitio del modelo y no se usa.
  `stroke_begin_line` empieza el trazo sin poner un toque en el punto de partida (si no, con opacidad baja ahí se vería
  una mancha más oscura).

### Caso 3. Deshacer también las acciones de capas
- **Problema:** el historial solo guardaba trazos; crear, borrar o mover capas no se podía deshacer.
- **Cómo:** cada operación de `LayerStack` anota un paso `LayoutEdit` con cómo estaba la lista ANTES (orden, carpeta de
  cada capa, capa elegida). Aplicarlo **intercambia** ese estado con el actual, así sirve para deshacer y rehacer. Las
  capas borradas se guardan enteras dentro del paso (movidas, no copiadas). `main.rs` pasa los pasos pendientes al
  historial antes de pintar o deshacer (`flush_layout_logs`), para que queden en orden con los trazos.
- **Pruebas:** en `undo.rs` (crear, borrar, carpeta, mover, duplicar, orden mezclado con trazos y pasos que ya no valen).

### Caso 4. Lo que ya existía
- La **simetría** (espejo en X, Y y Z) ya estaba hecha en el panel de reglas.
- Los **modos de mezcla** ya eran 28 (ARTCRAFT tiene 32): no hacía falta agregar.
- **Lección:** el paso 2 (comprobar) ahorra días de trabajo repetido.

### Caso 5. Quitar lo que confunde
La "Capa de selección" del panel de capas no hacía nada y se quitó. Un botón que no hace nada confunde más que uno que
no existe. Volverá con la máscara rápida, con una función real.

## 7. Qué probar en pantalla (`cargo run`)

Para **cualquier** función nueva:
1. Cargar un modelo y usarla de la forma normal. ¿Hace lo que promete?
2. Probarla con una **capa oculta** y con una **bloqueada** (no debe pintar donde no se puede).
3. Probarla sobre una **máscara** y sobre un **canal** (rugosidad, metálico, altura), no solo sobre el color.
4. Probarla con la **simetría** encendida.
5. **Ctrl+Z** y **Ctrl+Mayús+Z**: ¿deshace y rehace bien, sin saltos raros?
6. **Guardar y volver a abrir** el proyecto: ¿se conserva lo que debe?
7. Cambiar de pieza del modelo y de capa a mitad de uso.
8. Mirar si se siente lenta; con `TEXTURECRAFT_PERF=1 cargo run` el programa imprime cuánto tarda cada movimiento.

Qué anotar si algo falla: qué hiciste, qué esperabas y qué pasó. Una captura de pantalla ayuda mucho.

## 8. Trabajar con un asistente de IA (por ejemplo, Claude en VS Code)

El asistente puede leer el código de los dos proyectos, escribir cambios, correr las pruebas y actualizar los manuales,
pero **no ve la pantalla de la app**: lo que se ve al usarla lo tiene que probar una persona.

**Frases que funcionan bien**
- "Revisa si nuestro proyecto ya tiene X antes de hacer nada."
- "Mira cómo lo hace ARTCRAFT en tal carpeta y explícamelo en simple, sin copiar código."
- "Propón el cambio más chico que funcione y dime qué archivos vas a tocar."
- "Hazlo con pruebas, corre `cargo test` y dime el resultado real."
- "Actualiza `FASES.md` y el manual que toque."
- "Dime qué NO pudiste comprobar."

**Reglas para trabajar con IA**
- **Una sola sesión por archivo.** Ya pasó que otra sesión editaba `paint.rs` a la vez que la nuestra: no rompió nada,
  pero pudo pisar cambios. Si se abren dos sesiones, se reparten los archivos antes de empezar.
- Pedirle que **no toque archivos ajenos** a la tarea (los que otra persona está editando).
- Si dice "listo", preguntar **qué pruebas corrió y qué probó en pantalla**. Compilar no es lo mismo que funcionar.
- Revisar el cambio con `git diff` antes del commit.

**Nota técnica para el asistente**
- Leer primero `GUIA_EQUIPO.md`, `FASES.md` y `CAPAS.md`.
- Los comentarios del código van en español sin tildes ni ñ. Los `.md` de capas y fases también.
- Las terminales de VS Code no devuelven su salida al asistente: se redirige a un archivo temporal
  (`cargo test 2>&1 | tail -60 > /tmp/salida.txt`) y se lee ese archivo. Puede tardar: el archivo se llena al terminar.
- Si `cargo` dice "Blocking waiting for file lock", hay otro `cargo` corriendo (otra terminal u otra sesión): esperar.
- No editar la carpeta de ARTCRAFT; es solo de lectura.

## 9. Plantillas

### Ficha de una función (para anotar en `FASES.md` o en el commit)
```
Función:            (ejemplo: simetría radial)
Existe ya en lo nuestro: sí / no (dónde)
Idea de ARTCRAFT:   (carpeta y archivo donde lo vimos, y la idea en una frase)
Dónde se engancha:  (archivos y funciones nuestras)
Pruebas:            (nombres de las pruebas automáticas)
Estado:             [ ] pendiente   [~] hecho, falta probar en pantalla   [x] hecho y probado
Límites conocidos:  (lo que no hace todavía)
```

### `ATTRIBUTION.md` (solo si se adapta código de ARTCRAFT)
```
# Créditos

Partes de este programa están adaptadas de ArtCraft TextureCraft
(licencia MIT o Apache-2.0, a elección; ver su LICENSE y NOTICE).

| Nuestro archivo / función | Adaptado de (archivo de ARTCRAFT) | Qué se tomó |
|---|---|---|
| src/ejemplo.rs, función x | crates/<módulo>/src/ejemplo.rs | la idea del cálculo, reescrita |

Aquí se copia el aviso de derechos de autor y el texto de la licencia tal como aparecen en ARTCRAFT.
```

## 10. Problemas frecuentes

- **`cargo test` se queda esperando:** otro `cargo` está compilando (otra terminal, otra sesión). Esperar o cerrarlo.
- **Muchos `warning`:** no son errores. Solo importa que no haya `error` y que las pruebas pasen.
- **Compila pero no se ve el cambio:** ¿se está corriendo `cargo run` desde NUESTRA carpeta y no desde la de ARTCRAFT?
  Mirar la ruta de la terminal antes de correr nada.
- **Dos copias con el mismo nombre (`texturecraft`):** es fácil abrir la equivocada. Antes de editar, confirmar la ruta
  de la carpeta abierta en VS Code.
- **Cambios que no entiendes en un archivo:** `git diff` muestra qué se cambió; `git log` quién y cuándo.
- **Algo se rompió:** `git status` para ver qué cambió y volver al último commit que funcionaba (por eso se hace un
  commit por cada paso que anda).

## 11. El plan por fases (resumen; el estado actual está en `FASES.md`)

| Fase | De qué trata |
|---|---|
| 0 | Dejar limpio lo que ya hay: probar en pantalla y quitar o terminar lo que está a medias. |
| 1 | Mejoras pequeñas: Difuminar real, simetría, línea recta, modos de mezcla. |
| 2 | Comodidad: deshacer capas, panel de historial, exportar para motores de juego, tamaño de textura. |
| 3 | Más contenido: capas de relleno generadas por código, máscara rápida, clonar. |
| 4 | Cosas grandes: filtros por capa, máscaras que se generan solas desde el modelo, más formatos de modelo. |
| 5 | Lo enorme, solo si de verdad hace falta: hornear mapas y render con trazado de rayos. |

Hecho hasta hoy, pendiente de probar en pantalla: renombrar capas, Modo Solo, Difuminar y Borrosidad, línea recta con
Mayús+clic, deshacer capas (con máscaras y ajustes de capa), exportar para motores, tamaño de textura y quitar la capa de
selección. Lo que queda de la Fase 2: el panel de historial (y deshacer "combinar hacia abajo").

Orden recomendado para quien llegue nuevo: **(1)** leer esta guía, **(2)** abrir la app y probar lo que dice "falta
probar" en `FASES.md`, **(3)** elegir la siguiente función pequeña y seguir el método de la sección 4.
