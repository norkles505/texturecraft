# Guia de capas de TextureCraft

Esta guia resume el sistema de capas (inspirado en Ucupaint, el addon de Blender): que hay, como se usa y que se puede
deshacer. El detalle de diseno esta en `ESPECIFICACION.md` (secciones 4 y 5) y en los comentarios de `src/layers.rs`.

## Que hay

| Funcion | Que hace |
|---|---|
| Capas | Hasta 10 por pieza, con ojo, opacidad, modo de mezcla, bloqueo, recorte y nombre. |
| Mascaras | Hasta 4 por capa. Blanco muestra la pintura, negro la tapa, los grises a medias. Se combinan: donde cualquiera tapa, queda tapado. |
| Canales | Color, Rugosidad, Metalico y Altura. Cada uno se pinta aparte y se ve en grises. |
| Capas de color solido | Toda la capa es de un color y se ve donde la deja pasar su mascara. El color se cambia despues. |
| Carpetas | De un solo nivel. Juntan sus capas de dentro y tienen su propia opacidad, modo, mascara y ojo. |
| Orden | Subir y bajar capas (menu o arrastrando las tres rayitas). |
| Modo Solo | En pantalla solo se ve la capa elegida (no cambia lo que se exporta ni lo que se guarda). |

## Como se usa (panel de capas)

**Tira izquierda:** Nueva capa, Duplicar capa, Carpeta nueva (mete la capa elegida en una carpeta).

**Tira derecha:** Borrar capa (hay que tocarla dos veces), Combinar hacia abajo, Invertir colores, Limpiar capa.

**Menu de tres puntos** (las filas en gris no se pueden usar con la capa elegida):

1. Anadir (otra) mascara
2. Borrar la mascara elegida
3. Invertir mascara (solo cambia como se lee; no toca lo pintado)
4. Apagar o encender mascara
5. Siguiente mascara
6. Nueva capa de color (usa el color del pincel)
7. Cambiar color de la capa (de color solido; usa el color del pincel)
8. Sacar de la carpeta
9. Meter en la carpeta de arriba
10. Subir capa
11. Bajar capa
12. Bloquear o desbloquear capa (tambien la tecla L)
13. Renombrar capa (Enter acepta, Esc cancela)
14. Iniciar o terminar Modo Solo

**En cada fila de la lista:** el ojo, la miniatura de la capa, la miniatura cuadrada de la mascara (con varias, un
numerito "2/3"; tocarla otra vez pasa a la siguiente), el candado si esta bloqueada y las tres rayitas para arrastrar.

**Desplegable "Canal"** (bajo el modo de mezcla): elige que se pinta y se ve: Color, Rugosidad, Metalico, Altura.
"Vista: material" muestra el modelo con rugosidad, metal y relieve aplicados.

## Reglas que conviene saber

- **Pintar en una mascara:** el color del pincel se convierte a gris. El borrador pinta negro (tapa).
  En la **primera mascara de una capa de color solido** el pincel siempre "pinta el color" (blanco) y empieza negra.
- **Una carpeta no se pinta:** se pinta en sus capas de dentro (o en su mascara).
- **Una carpeta aisla:** lo que hace un modo de mezcla (Multiplicar, etc.) en una capa de dentro solo afecta a sus
  hermanas, no a lo de fuera.
- **Una capa suelta salta por encima de una carpeta entera** al subir o bajar; para meterla se usa "Meter en la carpeta
  de arriba".
- **Combinar hacia abajo** hornea mascaras y opacidad en los pixeles; la capa resultante queda sin mascara y al 100 %.
  No funciona con carpetas, ni entre capas de carpetas distintas, ni con capas ocultas, bloqueadas o recortadas.

## Que se puede deshacer (Ctrl+Z)

| Se deshace | No se deshace |
|---|---|
| Trazos de pincel y balde (en la capa, un canal o una mascara) | Combinar capas hacia abajo |
| Limpiar capa | Anadir, borrar, invertir o apagar una mascara |
| Invertir colores | Cambiar opacidad, modo de mezcla, nombre, ojo, candado o recorte de una capa |
| Crear, duplicar y borrar capas (una carpeta se borra con sus capas y vuelve entera) | Cambiar el color de una capa de color solido |
| Subir y bajar capas, crear carpetas, meter capas en una carpeta o sacarlas | |

Las capas borradas se guardan enteras (con su pintura, mascaras y canales) mientras el paso siga en el historial, que
guarda hasta 50 pasos o 64 MB. Combinar pide cuidado porque no se deshace, y al combinar los pasos anteriores de las dos
capas se descartan solos. Borrar sigue pidiendo dos toques, aunque ahora Ctrl+Z lo trae de vuelta.

## Guardado

Cada proyecto es una carpeta en `~/.local/share/texturecraft/proyectos/<id>/` con `proyecto.txt` y un PNG por cada
capa, mascara y canal que tenga pintura (`pieza<N>_capa<M>.png`, `..._mascara.png`, `..._mascara<K>.png`,
`..._<canal>.png`). Las carpetas no guardan pixeles: se recomponen al abrir. Los proyectos viejos (sin mascaras,
canales ni carpetas) se abren igual. El Modo Solo y la mascara que se esta editando no se guardan.

## Para quien programe

- Capas y composicion: `src/layers.rs` (el rendimiento se mide con
  `cargo test rendimiento_de_las_capas -- --ignored --nocapture`).
- Historial, "limpiar" e "invertir": `src/undo.rs`. Guardado: `src/proyecto.rs`. Panel: `src/ui/panel_capas.rs`.
- Todo lo que cambia lo que se ve debe terminar en `LayerStack::composite_rect` o `composite_all`.
- Las pruebas del guardado (`proyecto::tests`) usan una carpeta temporal y no tocan los proyectos de verdad.
