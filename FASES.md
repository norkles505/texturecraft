# Plan por fases de TextureCraft

Reglas: una cosa por vez; cada fase se termina y se prueba en la app (`cargo run`) antes de pasar a la siguiente; no se
apilan cambios sin probar; `cargo test` debe pasar. Se reescribe con las estructuras propias en vez de pegar codigo del
otro programa (`/home/kenny/Documentos/programas/texturecraft`) y, cuando nos inspiremos en algo suyo, se anota en
`ATTRIBUTION.md`. Antes de cada fase se mira como lo resuelven y se explica en simple que se va a hacer.
El orden se puede cambiar segun lo que mas importe.

Como se hace cada fase, el mapa del codigo, los casos ya resueltos y las plantillas estan en `GUIA_EQUIPO.md` (para todo
el equipo, con o sin asistente de IA).

## Fase 0: dejar limpio lo que ya hay

- [ ] Probar en pantalla: Renombrar capa, Modo Solo, Difuminar (D) y Borrosidad (U).
- [x] `cargo test` pasa (269 pruebas, segun la sesion anterior).
- [x] Quitar la fila "Capa de seleccion" del panel de capas (volvera con una funcion real en la mascara rapida).
- [ ] Probar el arrastre de capas con las tres rayitas y que la lista se vea bien sin la fila de seleccion.

## Fase 1: mejoras pequenas

- [~] Difuminar que arrastre la imagen real: hecho en `dab_filter` (`src/paint.rs`), por desplazamiento y acumulando el
      movimiento menor de un texel; falta probar la fuerza en pantalla (`gain` = 0.6).
- [~] Simetria (espejo en X, Y, Z): hecha en el panel de reglas; falta probarla en pantalla y optimizarla
      (con `TEXTURECRAFT_PERF=1 cargo run` el programa imprime cuanto tarda cada movimiento del raton).
- [~] Linea recta con Mayus+clic: hecha (recta desde donde termino el ultimo trazo de pincel o borrador, `line_anchor` en
      `src/main.rs`; solo vale mientras la vista no cambie); falta probarla en pantalla.
- [x] Mas modos de mezcla: ya hay 28 (el mismo juego que Photoshop; el otro programa tiene 32), no hace falta agregar.

## Fase 2: comodidad

- [~] Que crear, duplicar, borrar y mover capas (y meterlas o sacarlas de una carpeta) se pueda deshacer: hecho
      (`LayoutEdit` en `src/layers.rs`, pasos mezclados con los trazos en `src/undo.rs`); falta probarlo en pantalla.
      Tambien se deshacen las mascaras (anadir y quitar: `MaskEdit` en `src/layers.rs`) y los ajustes de capa (opacidad,
      modo de mezcla, nombre, ojo, candados, recorte, color liso, mascara encendida o invertida: `SettingsEdit`,
      comparados cuadro a cuadro en `render` de `src/main.rs`; un arrastre de deslizador es un solo paso). Hecho, con
      pruebas en `src/undo.rs`; falta probarlo en pantalla. Todavia no se deshace: combinar hacia abajo.
- [ ] Panel de historial para volver a cualquier paso.
- [~] Exportar con configuracion lista para Unreal, Unity (Standard y HDRP), Godot y Blender: hecho (menu -> "Guardar
      imagen" abre la ventana Exportar; `engine_maps` y `stack_engine_maps` en `src/export.rs`, la ventana en
      `src/ui/panel_menu.rs`); crea una subcarpeta `<nombre>_<motor>` con los mapas. Falta probarlo en pantalla y
      abrir los PNG en cada motor. Sin canal de oclusion todavia (va en blanco).
- [~] Elegir el tamano de la textura: hecho (512, 1024 o 2048, se elige en la galeria antes de crear el proyecto con "+"
      o al arrastrar un .obj; cada proyecto guarda su tamano en `proyecto.txt` como `tamano=`, y los guardados antes se
      abren como 1024). No se cambia despues de crear el proyecto. Falta probarlo en pantalla (crear uno de 512 y uno
      de 2048, guardar, volver a la galeria y reabrirlos).

## Fase 3: mas contenido para pintar

- [ ] Capas de relleno con patrones generados por codigo (nubes, rasguños, madera, grunge).
- [ ] Mascara rapida y clonar.

## Fase 4: cosas grandes

- [ ] Filtros por capa que no destruyen lo pintado.
- [ ] Mascaras que se generan solas desde el modelo (desgaste en bordes, suciedad).
- [ ] Abrir mas formatos de modelo (glTF, FBX).

## Fase 5: lo enorme (solo si de verdad se quiere)

- [ ] Hornear mapas del modelo y render con trazado de rayos.
