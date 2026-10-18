# PaintCraft — Especificación técnica (para portar a Rust)

Documento sacado del código de la app Android (`/home/kenny/Escritorio/proyectos/paintcraft`, paquete `com.meshcraft.app`, applicationId `com.paintcraft.app`). Describe **cómo se comporta** cada pieza para poder reescribirla en Rust sin depender del código viejo.

**Qué se leyó para este documento (completo):** `TexturedMeshGeometry.kt`, `LayerCompositor.kt`, `BlendModes.kt`, `PaintTool.kt`, `BrushDynamics.kt`, `MyGLRenderer.kt`, `MyGLSurfaceView.kt`, `MainActivity.kt`, `ProjectSerializer.kt`, `TriBvh.kt`, `ObjLoader.kt`, `IbisBrush.kt`, `SceneObject.kt`, `PngExport.kt`, `BackgroundKind.kt`, `app/build.gradle.kts`.

**Qué NO se leyó (solo interfaz de Android, no afecta a la lógica):** `PaintPanel.kt`, `LayersPanel.kt`, `ColorPanel.kt`, `ColorWheelView.kt`, `ColorPickerView.kt`, `IbisSlider.kt`, `GizmoView.kt`, `OutlinedTextView.kt`, `QrImageReader.kt`, `ImportedBrushStore.kt`, `GLUtils.kt` y los recursos `res/`. Si hace falta saber cómo se ve o se comporta algún panel, hay que leerlos aparte.

Nota: los comentarios del código dicen "estilo ArmorPaint" para el visor y "estilo ibisPaint" para el panel y los pinceles. Las secciones marcadas **[PROPUESTA]** no vienen del código: son ideas para la versión de PC.

---

## 1. Resumen

App móvil de pintura de texturas sobre modelos 3D:

- Importa un modelo `.obj`, lo muestra con cámara orbital y se pinta directamente sobre él con el dedo.
- La pintura vive en una textura cuadrada por pieza, hecha de capas (con carpetas, modos de mezcla y recorte).
- Herramientas: pincel, borrador, cuentagotas, difuminar, relleno, borrosidad (smudge).
- Deshacer/rehacer, guardado automático, exportación a PNG.
- Importa pinceles de ibisPaint leyendo su código QR.

Hoy solo se pinta **color** (RGBA). No hay canales PBR (roughness, metallic, normal...).

---

## 2. Arquitectura actual

Android nativo, Kotlin, OpenGL ES 2.0 directo (`GLSurfaceView`), sin motor ni librerías de gráficos. Única dependencia externa: `com.google.zxing:core` (lectura de QR). minSdk 24, targetSdk 34.

| Archivo | Responsabilidad |
|---|---|
| `MainActivity` | Pantalla única: vista 3D, panel de pintura abajo, menú File (New/Save/Import/Export), deshacer/rehacer, gizmo de ejes. Autoguardado en `onPause`. |
| `MyGLSurfaceView` | Gestos táctiles (pintar / girar / zoom / desplazar). |
| `MyGLRenderer` | Cámara, escena, raycast de pantalla a textura, trazos (espaciado, desvanecidos), historial de deshacer, guardado. |
| `TexturedMeshGeometry` | Una pieza: malla en GPU, capas, textura compuesta, **todas las herramientas de pintura**, undo por baldosas, exportación. |
| `LayerCompositor` | Combina las capas en una textura RGBA. |
| `BlendModes` | 27 modos de mezcla y la fórmula "over". |
| `TriBvh` | Acelerador de rayos (BVH) para el raycast. |
| `ObjLoader` | Lector de OBJ, partes y piezas. |
| `ProjectSerializer` | Formato de guardado (JSON + binarios). |
| `PngExport` | Escritor de PNG propio (sin premultiplicar). |
| `PaintTool` (+`BrushType`) | Herramientas y presets de pincel. |
| `BrushDynamics` | Desvanecidos y grano de un pincel. |
| `IbisBrush` | Lector del QR de pinceles de ibisPaint. |
| `SceneObject`, `BackgroundKind` | Objeto de escena y fondo bajo las capas. |

Hay carpetas de respaldo (`_backup_motor_claude`, `_backup_pluma_afilado`) con versiones anteriores de varios archivos. No son parte de la app.

Restos de una app anterior ("MeshCraft", un editor tipo Blender con primitivas): `MeshType` (cubo, esfera...), gizmos, selección de objetos. El código actual solo usa modelos importados; en Rust no hace falta portar nada de eso.

---

## 3. Modelo 3D

### 3.1 `ObjMesh`

Malla **triangulada y sin índices**: cada 3 vértices consecutivos forman un triángulo.

- `positions`: x,y,z por vértice. `normals`: x,y,z por vértice. `uvs`: u,v por vértice.
- `hasUvs`: false si el OBJ no trae UVs (entonces u,v = 0,0 y no se puede pintar bien).
- `parts`: lista opcional de partes (tramos seguidos de vértices, de triángulos completos, que cubren toda la malla). `partVisible[i]` indica si una parte se ve; una parte oculta no se dibuja **ni se puede pintar** (el rayo la atraviesa).

### 3.2 Lector de OBJ (`ObjLoader.load`)

- Líneas soportadas: `v`, `vt`, `vn`, `f`, `o`, `g`. Ignora materiales y suavizado.
- Caras de N vértices se trianguglan en **abanico**: `(0,k,k+1)`.
- Soporta índices negativos (relativos al final).
- Si algún vértice de un triángulo no trae normal, **todo el triángulo usa la normal plana** (producto cruzado normalizado).
- **Conversión de ejes:** el OBJ viene con Y arriba; la app usa **Z arriba**: `(x, y, z) → (x, -z, y)` (igual para normales). Parámetro `yUp = true` por defecto.
- **Normalización de tamaño** (`fitSize`, la app usa 1.0): centra el modelo en el origen (centro de su caja) y lo escala para que su lado más largo mida `fitSize`.
- **Partes:** cada línea `o`/`g` abre un tramo hasta la siguiente. Tramos vacíos se descartan. Caras anteriores a la primera línea forman una parte llamada "Sin nombre". Con menos de 2 partes se ignoran.

### 3.3 Piezas (`splitParts`)

Si el modelo tiene entre 2 y `MAX_PIECES = 12` partes, se separa en **una malla por parte**; cada pieza tiene su **propia textura, capas e historial**. Así las piezas que comparten zona del mapa UV no se pisan. Con más de 12 partes el modelo queda como una sola malla con una sola textura. Las posiciones no cambian: todas las piezas quedan juntas como en el original.

La pieza activa es la que muestra el panel de capas y la que se exporta. Al apoyar el pincel sobre otra pieza, esa pasa a ser la activa.

---

## 4. Textura y capas

### 4.1 Constantes

| Constante | Valor | Nota |
|---|---|---|
| `PAINT_TEX_SIZE` | 1024 | Lado de la textura de cada pieza (fijo). |
| `MAX_LAYERS` | 10 | Capas con píxeles por pieza (4 MB cada una). |
| `MAX_FOLDERS` | 5 | Carpetas por pieza (no pesan). |
| `UNDO_TILE` | 64 | Lado de la baldosa de deshacer. |

### 4.2 `PaintLayer`

- `pixels`: RGBA de 8 bits, **alfa recto (sin premultiplicar)**, `size*size*4` bytes. Origen: la fila 0 es v=0.
- Ajustes: `name`, `visible`, `opacity` (0..100), `locked`, `alphaLock`, `blendMode`, `clipToBelow`, `isFolder`, `folderId` (-1 = suelta), `collapsed` (solo UI).
- Una **carpeta** no tiene píxeles (`pixels` vacío) pero sí ojo, opacidad, modo de mezcla y recorte. **Las carpetas no se anidan.**
- Una carpeta se guarda **después** de sus capas en la lista (de abajo hacia arriba).
- Bloqueada u oculta: no se puede pintar en ella. Bloqueo alfa: solo se cambia el color de lo que ya tiene algo.

### 4.3 Operaciones sobre capas

- **Añadir capa:** transparente, justo encima de la elegida (si la elegida es una carpeta, entra dentro; si la elegida está dentro de una carpeta, la nueva entra en la misma). Máximo `MAX_LAYERS`. Numeración automática ("1", "2"...).
- **Clonar:** copia píxeles y ajustes (menos `locked`), nombre = original + " copia".
- **Añadir carpeta:** vacía, suelta, encima de la elegida (o de la carpeta que la contiene).
- **Mover (`arrangeLayer`):** a una carpeta (o suelta) y a una posición entre sus hermanas. Una carpeta se lleva sus capas. No toca píxeles. Se puede deshacer (guarda el orden anterior).
- **Borrar:** siempre debe quedar al menos una capa con píxeles. Borrar una carpeta borra sus capas. **No se puede deshacer** (vacía el historial).
- **Combinar hacia abajo (`mergeSelectedDown`):** solo dos capas con píxeles del mismo grupo. Aplica modo, opacidad (0 si está oculta) y recorte de la de arriba sobre la de abajo; la de abajo conserva sus ajustes. **No se puede deshacer.**
- **Limpiar:** pone todo en 0. **Invertir:** invierte RGB donde alfa ≠ 0, el alfa no cambia. Ambas se pueden deshacer.
- **Modo Solo:** solo se ve una capa (o el contenido de una carpeta), sin cambiar su visibilidad real; su recorte se sigue calculando contra su base.

### 4.4 Fondo (`BackgroundKind`)

Color opaco bajo todas las capas (importa para Multiplicar, Trama, etc.):

- `WHITE`: 0xFFFFFF.
- `CHECKER` y `NONE`: tablero de casillas de 32 texeles, 0xFFFFFF / 0xDEDEDE (`x/32 + y/32` par = claro).
- `CHECKER_DARK`: 0x5A5A5A / 0x373737.
- `NONE` además **exporta transparente**.

Existe una constante de depuración `USE_UV_CHECKER` (apagada) con un tablero que muestra U en rojo y V en verde.

---

## 5. Composición de capas (`LayerCompositor`)

Entrada: lista de capas (de abajo hacia arriba), `soloId`, color base opcional, rectángulo de texeles. Salida: RGBA recto.

Para cada texel:

1. Color acumulado = fondo (con alfa 255) o transparente.
2. Se recorren los **elementos de primer nivel** (capas sueltas y carpetas) de abajo hacia arriba.
3. Para una **carpeta**: sus capas hijas se componen entre sí sobre transparente (mismas reglas) y el resultado entra como **una sola capa** con el modo, opacidad, ojo y recorte de la carpeta.
4. Para cada capa: `sa = alfa_pixel * opacidad/100` (en 0..255). Si está oculta (o con opacidad 0) o fuera del modo Solo, no se dibuja.
5. **Recorte:** si `clipToBelow`, `sa = sa * alfaBase / 255`, donde `alfaBase` es el alfa (del píxel) de la **última capa de su grupo que no está recortada**. Si esa base está oculta, el alfa base es 0 (la recortada tampoco se ve). **La capa de más abajo de cada grupo ignora su recorte.**
6. Se mezcla con `BlendMixer.over` (ver sección 6).

Regla del modo Solo: se dibuja la capa elegida; si es una carpeta, todo su contenido; si es una capa dentro de una carpeta, la carpeta se dibuja pero solo esa capa dentro.

Recomposición:
- Tras un trazo se recompone **solo el rectángulo tocado** y se sube ese rectángulo a la GPU (`glTexSubImage2D`).
- Tras cambios de ajustes (ojo, opacidad, modo...) se recompone **toda** la textura una vez por cuadro (`compositeDirty`), repartida en franjas de filas entre hasta 4 hilos.
- Se usa la misma función para la textura visible y para exportar, así salen iguales.

---

## 6. Modos de mezcla (`BlendMode`, `BlendMixer`)

Todo en enteros 0..255 salvo donde se indique. Orden de la lista (los nombres del enum son lo que se guarda en el proyecto):

- **Oscurecer:** `NORMAL`, `DARKEN`, `MULTIPLY`, `COLOR_BURN`, `LINEAR_BURN`, `DARKER_COLOR`
- **Aclarar:** `LIGHTEN`, `SCREEN`, `COLOR_DODGE`, `LINEAR_DODGE`, `ADD`, `LIGHTER_COLOR`
- **Contraste:** `OVERLAY`, `SOFT_LIGHT`, `HARD_LIGHT`, `VIVID_LIGHT`, `LINEAR_LIGHT`, `PIN_LIGHT`, `HARD_MIX`
- **Diferencia:** `INVERT`, `DIFFERENCE`, `EXCLUSION`, `SUBTRACT`, `DIVIDE`
- **Color:** `HUE`, `SATURATION`, `COLOR`, `LUMINOSITY`

Un nombre de modo desconocido al cargar cae en `NORMAL`.

### 6.1 Fórmulas por canal (b = fondo, s = fuente)

```
DARKEN        min(b, s)
MULTIPLY      b*s/255
COLOR_BURN    burn(b, s)
LINEAR_BURN   max(0, b+s-255)
LIGHTEN       max(b, s)
SCREEN        b + s - b*s/255
COLOR_DODGE   dodge(b, s)
LINEAR_DODGE  min(255, b+s)            (ADD es igual)
OVERLAY       hardLight(s, b)          (argumentos intercambiados)
SOFT_LIGHT    ver abajo
HARD_LIGHT    hardLight(b, s)
VIVID_LIGHT   s<128 ? burn(b, 2s) : dodge(b, 2(s-128))
LINEAR_LIGHT  clamp(b + 2s - 255, 0, 255)
PIN_LIGHT     s<128 ? min(b, 2s) : max(b, 2s-255)
HARD_MIX      b+s >= 255 ? 255 : 0
INVERT        255 - b
DIFFERENCE    |b - s|
EXCLUSION     b + s - 2*b*s/255
SUBTRACT      max(0, b-s)
DIVIDE        s==0 ? 255 : min(255, b*255/s)

burn(b,s):  b>=255 → 255;  s<=0 → 0;  255 - min(255, (255-b)*255/s)
dodge(b,s): b<=0 → 0;  s>=255 → 255;  min(255, b*255/(255-s))
hardLight(b,s): s<128 ? b*(2s)/255 : (s2=2s-255; b + s2 - b*s2/255)

SOFT_LIGHT (con cb=b/255, cs=s/255, resultado *255+0.5, clamp 0..255):
  cs <= 0.5:  cb - (1-2cs)*cb*(1-cb)
  si no:      d = cb<=0.25 ? ((16cb-12)cb+4)cb : sqrt(cb);   cb + (2cs-1)(d-cb)
```

### 6.2 Modos no separables

- `DARKER_COLOR`: elige el color entero (fondo o fuente) con menor `luma = 77r + 151g + 28b`. `LIGHTER_COLOR`: el de mayor luma.
- `HUE`, `SATURATION`, `COLOR`, `LUMINOSITY` usan las funciones W3C/Photoshop en float 0..1:
  - `lum = 0.3r + 0.59g + 0.11b`, `sat = max - min`.
  - `setLum(c, l)`: suma `d = l - lum(c)` a los tres canales y luego `clip`.
  - `clip`: con `l = lum`, `n = min`, `x = max`: si `n < 0` → `c = l + (c-l)*l/(l-n)`; si `x > 1` → `c = l + (c-l)*(1-l)/(x-l)`.
  - `setSat(c, s)`: ordena canales; si max == min → todo 0; si no, `mid = (mid-min)*s/(max-min)`, `max = s`, `min = 0`.
  - `HUE`: `c = fuente; setSat(c, sat(fondo)); setLum(c, lum(fondo))`.
  - `SATURATION`: `c = fondo; setSat(c, sat(fuente)); setLum(c, lum(fondo))`.
  - `COLOR`: `c = fuente; setLum(c, lum(fondo))`.
  - `LUMINOSITY`: `c = fondo; setLum(c, lum(fuente))`.

### 6.3 `over` (mezcla con alfa, alfa recto)

Entrada: destino `(dr,dg,db,da)`, fuente `(sr,sg,sb)` con alfa `sa` (ya con opacidad de capa).

```
cr,cg,cb = sr,sg,sb
si modo != NORMAL y da > 0:
    m = mix(modo, destino, fuente)
    cr = (sr*(255-da) + m.r*da) / 255        (igual para g, b)
si da == 0:  resultado = (sa, cr, cg, cb)
keep = da*(255-sa)/255
outA = sa + keep
r = (cr*sa + dr*keep) / outA                 (igual para g, b)
resultado = (outA, r, g, b)
```

Es decir: donde el fondo es transparente la capa se queda tal cual; donde tiene algo, el color se mezcla con el modo en proporción al alfa del fondo.

---

## 7. Pincel (`paintDab`)

Un trazo es una secuencia de **toques** (dabs). El estado del trazo vive por texel:

- `strokeAlpha[idx]` (float): mayor opacidad aplicada a ese texel en la pasada actual (0 = sin tocar).
- `strokeBase[idx*4..]` (RGBA): color de la capa **antes del trazo**.
- `strokeLastDab[idx]` (int): último número de toque que tocó el texel.
- `strokeDab`: contador de toques.
- Rectángulo tocado (para limpiar `strokeAlpha` sin recorrer toda la textura).
- Estos tres arreglos se crean la primera vez que se usan (12 MB por pieza).

`beginStroke()` (al apoyar el dedo): limpia `strokeAlpha` solo en el rectángulo tocado por el trazo anterior, reinicia el rectángulo y el estado del smudge.

### 7.1 Algoritmo de un toque

Parámetros: centro `(u,v)`, `nominalRadius` (texeles), color, `opacity`, `brushType`, `erase`, `grain`, `grainRadius`, `edgeAa`.

```
si capa es carpeta, está bloqueada u oculta → nada
radius = edgeAa > 0 ? nominalRadius + edgeAa/2 : nominalRadius
cx = u*(size-1);  cy = v*(size-1)
caja = [cx-radius, cx+radius] x [cy-radius, cy+radius] recortada a la textura
islaDelToque = mapa de islas en (cx,cy)   (0 si el mapa aún no está listo)
strokeDab++
grainOffX = hash01(strokeDab, 7)  * 5000
grainOffY = hash01(strokeDab, 13) * 5000
guarda las baldosas de deshacer que toca la caja (antes de modificar nada)

para cada texel (x,y) de la caja:
    d = distancia al centro;  si d >= radius → saltar
    si hay mapa de islas, islaDelToque != 0 y isla(x,y) != islaDelToque → saltar
    t = 1 - d/radius
    falloff = brushType.falloff(t)
    si edgeAa > 0:  falloff = min(falloff, (radius - d)/edgeAa)
    si falloff <= 0 → saltar
    grainMul = (grain>0 y grainRadius>0) ? 1 - grain + grain*grainAt(x,y,grainRadius,offX,offY) : 1
    sel = selección ? selMask[idx]/255 : 1
    dabAlpha = falloff * opacity * grainMul * sel
    prev = strokeAlpha[idx]
    si prev > 0 y (strokeDab - strokeLastDab[idx]) > 3:  prev = 0      # el trazo se fue y volvió: pasada nueva
    strokeLastDab[idx] = strokeDab
    si dabAlpha <= prev → saltar
    si prev == 0:  strokeBase[idx] = color actual de la capa en ese texel
    strokeAlpha[idx] = dabAlpha
    aplicar según modo (ver 7.2) usando strokeBase como "color de antes"

recomponer el rectángulo [x0..x1]x[y0..y1] y subirlo a la GPU
```

Consecuencia clave: dentro de una misma pasada **la opacidad nunca supera la elegida** (se usa el máximo de los toques, no se acumula), pero si el trazo cruza sobre sí mismo (hueco de más de 3 toques) la segunda pasada **sí suma**.

### 7.2 Cómo se escribe el píxel

Con `baseA = strokeBase.alfa`, `old = strokeBase.rgb`:

- **Borrar (`erase`):** `alfa = baseA * (1 - dabAlpha)`. El color no cambia.
- **Bloqueo alfa:** si `baseA == 0` no se hace nada; si no, `rgb = old + (color - old)*dabAlpha`. El alfa no cambia.
- **Pintar (modo Normal):**
  ```
  a0 = baseA/255;  keep = a0*(1-dabAlpha);  outA = dabAlpha + keep
  rgb = (color*dabAlpha + old*keep) / outA
  alfa = outA*255 + 0.5
  ```
  El modo de mezcla es **de la capa entera**, no del pincel: se aplica al componer.

### 7.3 Grano

```
hash01(ix,iy):  h = ix*374761393 + iy*668265263          (enteros de 32 bits con desborde)
                h = (h xor (h ushr 13)) * 1274126177
                h = h xor (h ushr 16)
                (h and 0xFFFFFF) / 16777216
valueNoise(x,y,cell): celdas de 'cell' texeles, interpolación suavizada (smoothstep f*f*(3-2f)) de hash01 en las 4 esquinas
grainAt(x,y,radius,offX,offY):
    xf = x+offX;  yf = y+offY
    n = 0.6*valueNoise(xf,   yf,   max(0.5, radius*0.3)) +
        0.4*valueNoise(xf+917, yf+331, max(0.5, radius*0.6))
    return clamp((n-0.5)*2 + 0.5, 0, 1)
```

El desplazamiento cambia en cada toque (como la punta de ibisPaint que gira al azar); con el máximo por pasada eso da un gris parejo con motas.

### 7.4 Espaciado y relleno del trazo (`MyGLRenderer`)

- Distancia entre toques: `max(0.5, radio * brushType.spacing)` **texeles** (no píxeles de pantalla), así no depende del zoom.
- Para cada evento de movimiento se trazan dos rayos (extremo anterior y nuevo) y `strokeSegment` **parte el tramo por la mitad** recursivamente hasta que los dos extremos quedan a un espaciado o menos en la textura. Cortes de seguridad: tramo de menos de 0.05 px de pantalla o profundidad 12 (4096 trozos). Si un extremo cae fuera del modelo o en otra pieza se mide en pantalla (umbral 6 px).
- Solo se pinta el extremo final de cada tramo (el inicial ya se pintó).
- Android entrega varias posiciones por evento (`historicalX/Y`): se pintan todas en orden, después la actual.

### 7.5 Presets de pincel (`BrushType`)

`falloff(t)` con `t = 1 - d/radio` (1 en el centro, 0 en el borde). El slider de tamaño va de 1 a 100 (radio en texeles).

| Pincel | Tamaño inicial | Espaciado | falloff(t) |
|---|---|---|---|
| Suave (SOFT) | 18 | 0.25 | `min(1, 2t)` |
| Duro (HARD) | 18 | 0.25 | `min(1, 8t)` |
| Aerógrafo (AIRBRUSH) | 18 | 0.25 | `t²` |
| Lápiz (PENCIL) | 3 | 0.15 | `min(1, 10t)` |
| Lápiz (Difuminado) | 48 | 0.20 | `t²(3-2t)` |
| Rotulador (Suave) | 6 | 0.15 | `min(1, 4t)` |
| Rotulador (Fuerte) | 14 | 0.12 | `min(1, 20t)` |

Valores iniciales del panel: color (225, 70, 60), radio 18, opacidad 1.

### 7.6 Dinámicas del trazo (`BrushDynamics`)

Campos: `startSize`, `endSize`, `startOpacity`, `endOpacity` (fracción, 1 = normal), `startLength`, `endLength` (en **diámetros** del pincel), `opacityScale`, `grain` (0..1), `edgeAa` (texeles de suavizado del borde).

- Los pinceles propios usan `BrushDynamics.NONE` (sin cambios).
- `hasStart` = `startLength > 0` y (`startSize < 1` o `startOpacity < 1`). `hasEnd` análogo.
- `ramp(from, dist, len)`: vale `from` en `dist = 0` y sube **linealmente** a 1 en `dist = len`; con `len <= 0` vale 1.
- Durante el trazo: `dist` = suma de distancias en texeles entre toques consecutivos de la misma pieza. `sizeF = ramp(startSize, dist, startLen)`, `opacityF = ramp(startOpacity, dist, startLen)`, con `startLen = startLength * diámetro`. El toque usa radio `max(0.5, radio*sizeF)` y opacidad `opacidad * opacityScale * opacityF`.
- **Desvanecido del final (`applyEndFade`):** como no se sabe dónde termina el trazo hasta soltar el dedo, los pinceles con `hasEnd` pintan normal y recuerdan sus últimos toques (`recentDabs`, guardando solo los que un desvanecido puede cambiar: ventana de `startLen + endLen + 4*radio`). Al soltar:
  1. Si el trazo es más corto que `startLen + endLen`, ambos largos se acortan proporcionalmente (`k = total/(startLen+endLen)`).
  2. **A** = toques dentro del tramo final (o todos si es corto). **S** = toques anteriores cuyo círculo toca algún círculo de A (comparten texeles y como cada texel guarda el máximo, hay que repintarlos).
  3. `revertStrokeCircles` devuelve al color previo los círculos de A y los marca "sin tocar" (`strokeAlpha = 0`).
  4. Se repiten, **en el mismo orden**, los toques de S (como se pintaron) y los de A con `sizeF = ramp(startSize, dist, startLen) * ramp(endSize, total - dist, endLen)` y análogo para opacidad. Cada toque repintado usa su mismo número de toque (`setDabSerial`) para tener el mismo grano.
- `opacityScale` = 0.7 si el pincel tiene grano (`GRAIN_OPACITY_SCALE`), 1 si no. Constante `EDGE_AA_TEXELS = 1`.
- `FADE_DIAMETERS_PER_UNIT = 7` (diámetros por cada 100 % de "longitud"); excepciones calibradas: pincel de fábrica 1 (Pluma Fuerte) = 30; el inicio del pincel 149 = 1.6. Estos valores salen de mediciones sobre capturas de ibisPaint y el propio código los marca como **hipótesis** en algunos casos.

---

## 8. Otras herramientas

Todas respetan: capa carpeta/bloqueada/oculta (no actúan), selección, bloqueo alfa y mapa de islas UV. Todas son deshacibles (guardan baldosas antes de modificar).

### 8.1 Borrador
Es `paintDab` con `erase = true`, sin grano ni dinámicas (usa `paintRadius`, `paintOpacity`, `paintBrushType`).

### 8.2 Cuentagotas
Lee el color de la **textura compuesta** (lo que se ve, con todas las capas) en `(u,v)`; devuelve RGB. Se aplica también al arrastrar.

### 8.3 Difuminar (`blurDab`)
Para cada texel del círculo: promedio 3×3 de los vecinos (tomados de una **copia de la zona antes del toque**), **ponderado por alfa**, en espacio premultiplicado. El texel se acerca a ese promedio con `k = falloff * strength * sel` (`strength` = opacidad del panel). Si el texel y sus vecinos están vacíos, se salta. Escritura con `writePremultiplied`.

### 8.4 Borrosidad / smudge (`smudgeDab`)
- Buffer de floats `side×side×4` (premultiplicado), con `rad = ceil(radio)`, `side = 2*rad + 1`, centrado en el texel redondeado.
- El **primer toque** del trazo solo recoge los colores bajo el pincel (no cambia nada, no guarda undo).
- Los siguientes: en cada texel del círculo `pix = lerp(pix, buffer, k)` con `k = falloff*strength*sel`, y **después** el buffer recoge un poco de lo que pisó: `buffer += (pix_antes - buffer) * 0.15` (`SMUDGE_PICKUP`), para todo el cuadrado.
- Si el radio cambia o el trazo pasa a otra isla UV, se reinicia (vuelve a recoger).

### 8.5 `writePremultiplied(pix, o, pr, pg, pb, pa, alphaLock)`
- Con bloqueo alfa: si el texel está vacío o `pa < 0.5`, no hace nada; solo cambia el color (el alfa no se toca).
- Sin bloqueo: si `pa < 0.5` pone 0,0,0,0.
- Si no: `inv = 255/pa`; `rgb = pr*inv + 0.5` (clamp 0..255); `alfa = pa + 0.5`.

### 8.6 Relleno (`fillAt`, tolerancia 32)
1. Parte del texel tocado. Se descarta si: la selección está en 0 ahí, ya es de ese color opaco, o hay bloqueo alfa y el texel está vacío.
2. Zona = texeles **conectados** (4 vecinos, por franjas horizontales con pila de semillas) donde `|alfa - alfaInicial| ≤ tol` y (si el inicial no es transparente) `|r|,|g|,|b|` también dentro de `tol`. Debe estar dentro de la **cobertura UV** (ver 10.2), dentro de la **misma isla** y dentro de la selección. No cruza costuras entre islas.
3. **Borde suave** (solo sin bloqueo alfa): desde el borde de la zona se avanza mientras el alfa siga **subiendo** (sin tocar lo ya opaco); esos texeles se marcan con 2.
4. Escritura: zona normal → color opaco (con bloqueo alfa solo cambia el color donde ya hay algo). Texeles marcados 2 → el relleno va **debajo**: `rgb = old*ea + color*(1-ea)` con `ea = alfa/255`, y alfa = 255. Así no queda un anillo sin rellenar en trazos desvanecidos.
5. Un solo toque por apoyo; arrastrar no vuelve a rellenar.

### 8.7 Selección
`selectByOpacity`: máscara de 1 byte por texel = alfa de la capa elegida. Falla si la capa es carpeta o está vacía. Después, **todo toque se multiplica por `mask/255`**. `clearSelection` la quita. No se puede deshacer. Hay miniatura (naranja 242,128,26 con alfa = máscara).

---

## 9. Raycast para pintar

### 9.1 De pantalla a textura (`MyGLRenderer`)
1. Punto de pantalla → NDC: `ndcX = 2x/w - 1`, `ndcY = 1 - 2y/h`.
2. Se invierte `proyección × vista × rotación` (cacheada por evento) y se transforman los puntos `(ndc, -1)` y `(ndc, +1)` para obtener origen y dirección del rayo.
3. Para cada pieza visible con geometría ya creada: se lleva el rayo al espacio local (inversa de la matriz modelo, cacheada por evento) y se llama a `pickUv`.
4. Gana el impacto de menor `t`. El resultado es `(geo, u, v, id de pieza)`.

### 9.2 Intersección (`pickUv` / `TriBvh.pick`)
- **Möller–Trumbore, sin descartar caras traseras.** Descarta `|det| < 1e-9`; acepta `t > 1e-5`.
- UV del impacto = interpolación baricéntrica de las UVs de los 3 vértices: `w0 = 1 - bu - bv`.
- Triángulos de partes ocultas (`triVisible[t] = false`) se ignoran.
- **BVH (`TriBvh`):** caja por nodo con holgura `1e-5`; hojas de hasta 4 triángulos; se parte por la **mediana** a lo largo del eje donde más se separan los centros de los triángulos (con `select`, un quickselect); construcción sin recursión (pila de 128 entradas); recorrido con pila ordenando por distancia de entrada a la caja (primero la más cercana, para podar con `bestT`). Se construye en un hilo de baja prioridad; hasta que esté listo se recorren todos los triángulos. Resultado idéntico al recorrido completo.

Nota de portado: en Rust conviene un BVH más estándar (por ejemplo, SAH) o una librería (p. ej. `bvh`, `parry3d`), pero el **resultado** debe ser igual.

---

## 10. Mapas auxiliares por UV

### 10.1 Mapa de islas UV (`islandMap`)
- Para cada texel, el número de isla (1, 2, 3...) o 0 si no lo cubre ningún triángulo.
- Dos triángulos son de la misma isla si **comparten un vértice UV** (misma posición en el mapa). Clave por vértice: UV cuantizado a 20 bits por eje + índice de triángulo (24 bits); se ordenan las claves y se unen con **union-find**. Con ≥ 2²⁴ triángulos no se arma.
- Cada triángulo se rasteriza (coordenadas baricéntricas, `denom` ≈ 0 se salta) con el número de su isla.
- **Margen de 2 texeles:** dos pasadas donde los texeles vacíos junto a una isla toman su número.
- Se construye **la primera vez que se pinta** en un hilo de baja prioridad; hasta entonces el pincel no se limita.
- **Uso:** el pincel, difuminar, smudge y relleno solo actúan en la isla donde cae el **centro** del toque. Evita que el trazo se copie en otras piezas del mapa UV que están cerca en la imagen pero lejos en 3D.

### 10.2 Cobertura UV (`coverage`)
Máscara de 1 byte: 1 donde algún triángulo cubre el texel, con 1 texel de margen. Se calcula la primera vez que se rellena. La usa el relleno.

---

## 11. Deshacer / rehacer

- Cada capa se divide en **baldosas de 64×64** texeles. La primera vez que un trazo (o una operación) toca una baldosa, se guarda una copia de sus píxeles **antes** de cambiarlos (`saveTilesForUndo`, copy-on-write). Un registro (`StrokeEdit`) pertenece a **una sola capa**.
- `applyEdit` **intercambia** los píxeles actuales con los guardados baldosa por baldosa (recompone y sube cada una). Como es un intercambio, el mismo registro sirve para rehacer.
- Mover capas/carpetas guarda el orden anterior (`LayoutSnapshot`: ids y carpeta de cada una) y se deshace igual.
- Pila de deshacer: máximo **50 pasos** y **64 MB** (se descartan los más viejos). Un trazo nuevo vacía la pila de rehacer.
- **Entrega perezosa:** el trazo en curso entra al historial al empezar el siguiente trazo o al pulsar deshacer/rehacer (`commitPendingStrokes`).
- Un paso cuya capa ya no existe, o cuyo orden de capas ya no encaja, se descarta (`canApply`) y se prueba con el anterior.
- Operaciones **no deshacibles** (vacían todo el historial): borrar capa/carpeta, combinar capas, importar/New (el historial era de otro modelo). Al recrearse el contexto GL también se vacía.

---

## 12. Guardado

Un solo "slot" fijo en el almacenamiento interno de la app; se **autoguarda** en `onPause` (si la escena no está vacía) y se **autocarga** al abrir. Escritura atómica (archivo `.tmp` y renombrado).

- `current_project.json`:
  - `nextObjectId`, `objects[]`: por pieza `id`, `type`, `posX/Y/Z`, `rotationMatrix` (16 floats), `shapeMatrix` (16 floats), `selected`, `name`, `visible`, `importedMeshFile`, `parts[]` (`name`, `first`, `count`, `visible`, `pos`).
  - `layers`: por id de objeto → `selected`, `background`, `items[]` con `file`, `name`, `visible`, `opacity`, `locked`, `alphaLock`, `blendMode`, `clip`, `folder`, `parent` (índice de la carpeta en la lista), `collapsed`.
- `imported_meshes/mesh_<id>.bin`: little-endian; `int magic 0x4F424A31 ("OBJ1")`, `int vertexCount`, `int hasUvs`, luego `positions` (3 floats/vértice), `normals` (3), `uvs` (2).
- `paint_layers/layer_<objId>_<index>.lyr`: `int magic 0x4C595231 ("LYR1")`, `int lado`, y los `lado*lado*4` bytes RGBA comprimidos con **deflate** (`BEST_SPEED`). Sin PNG a propósito: el PNG de Android premultiplica el alfa y deforma los píxeles casi transparentes. Los archivos de capas que ya no se usan se borran tras guardar.
- Lectura tolerante: una capa con archivo dañado se omite (y se reasignan los índices de carpeta); debe quedar al menos una capa con píxeles; JSON roto devuelve lo que pudo leer; un objeto cuya malla falta se omite (no se carga un cubo falso); las partes guardadas solo se aceptan si cubren la malla entera, en orden, desde el vértice 0.
- **Compatibilidad:** formato propio, no es de ningún estándar.

---

## 13. Exportación a PNG (`PngExport`)

Tres tipos:
1. Textura completa **con el fondo elegido** (con `NONE` sale transparente).
2. Textura completa **transparente** (sin fondo).
3. **Solo una capa** (sus píxeles tal cual, sin opacidad ni visibilidad; no sirve para carpetas).

Ignora el modo Solo. Escritor propio: RGBA 8 bits, sin entrelazado, filtro 0 en todas las filas, zlib (`DEFAULT_COMPRESSION`), alfa **recto**. **`flipVertical = true` por defecto**: las UVs del OBJ tienen v=0 abajo, la textura guarda v=0 en la fila 0 y en un PNG la fila 0 es arriba. Con el volteo la textura cae en su lugar en Blender. Nombre sugerido: `paintcraft_textura[_pieza].png` o `paintcraft_capa[_pieza].png`.

---

## 14. Visor 3D

### 14.1 Cámara
- **Z arriba** (estilo Blender). `angleX` = inclinación (rota sobre X mundo), `angleY` = giro (rota sobre Z mundo).
- Valores iniciales: `angleX = 19.8°`, `angleY = -137°`, `cameraDistance = 7.47` (limitada a 0.5..20), `panX = 0.05`, `panZ = 0.24`.
- Vista: ojo en `(panX, -distancia, panZ)`, mirando a `(panX, 0, panZ)`, arriba `(0,0,1)`. La rotación de la cámara se aplica como rotación del mundo: `rotación = Rx(angleX) · Rz(angleY)`.
- Perspectiva: `frustum` con plano cercano 0.1 y mitad de altura `near*0.5` (o sea `tan(mitad del FOV) = 0.5`, ≈ 53° vertical), lejano 30. Ortográfica (al pulsar un eje del gizmo): tamaño `distancia*0.5`, plano cercano 0.1, lejano 30. Girar a mano vuelve a perspectiva.
- Gizmo de ejes (esquina superior derecha): al tocar un eje se anima (220 ms, desaceleración) a esa vista ortográfica por el camino angular más corto.
- Color de limpieza: (0.11, 0.11, 0.11).
- Modelo: `traslación · rotación · forma (escala/shear)`; hoy siempre identidad (el modelo se normaliza al importar).

### 14.2 Sombreado (shader de la malla)
Luz fija **respecto a la cámara** (las normales pasan a espacio de vista con la parte de rotación de la matriz): al girar el modelo los brillos no se mueven. **No es PBR.**

```
n = normalize(normalVista)
ambient = mix(0.42, 0.78, n.y*0.5 + 0.5)
key  = max(dot(n, normalize(-0.45, 0.65, 0.75)), 0) * 0.50
fill = max(dot(n, normalize( 0.70,-0.25, 0.50)), 0) * 0.14
half = normalize(keyDir + (0,0,1))
spec = pow(max(dot(n, half), 0), 28) * 0.12
color = textura.rgb * (ambient + key + fill) + spec        (clamp 0..1, alfa 1)
```

Textura: filtro lineal, `REPEAT`, **sin mipmaps** (solo se actualiza el nivel 0 al pintar). VBO entrelazado de 8 floats por vértice (posición, normal, uv), `glDrawArrays`, sin índices (sin el límite de 65 535 vértices). Las partes se dibujan en tramos seguidos (se juntan las visibles contiguas en una sola llamada).

### 14.3 Gestos (`MyGLSurfaceView`)
- **Un dedo:** si se apoya **sobre el modelo** → pinta; si se apoya **fuera** → gira la vista. Con la "mano" activa (modo PAN) un dedo desplaza en cualquier parte y no pinta.
- Saber si el dedo cayó sobre el modelo requiere el raycast del hilo de render, así que la decisión llega 1–2 cuadros después. El trazo empieza cuando el dedo se mueve más de `6 dp` (o al soltar, si fue un toque simple: un solo punto). Si llega un segundo dedo antes, no se pinta nada.
- **Dos dedos:** el pellizco hace zoom (`distancia *= distanciaAnterior/distanciaNueva`); el movimiento del punto medio gira (modo ROTATE) o desplaza (modo PAN). Si se levanta un dedo y quedan 2 o más, se toma una base nueva para que la vista no salte.
- Giro: `0.5°` por píxel (en ambos ejes). Desplazamiento: `0.01 * (distancia/6.5)` por píxel (`panX -= dx*s`, `panZ += dy*s`).
- Candado de cámara: con él activo no se gira ni desplaza (el zoom sigue).
- Render continuo (`RENDERMODE_CONTINUOUSLY`); el contexto GL se conserva al pausar y, si se pierde, las capas (que viven en CPU) se restauran en la geometría nueva.

---

## 15. Pinceles importados de ibisPaint (`IbisBrush`)

Formato del QR (descubierto comparando muchos pinceles; solo se confirmaron algunos campos):
- Bytes: `"IPBZ"` + `00 02 23 44` + datos con **deflate crudo** (sin cabecera zlib).
- Datos descomprimidos: **380 bytes** base; floats de 32 bits **big-endian** (porcentajes como fracción, 100 % = 1.0), algunos enteros y unos pocos bits.
- Si el pincel tiene nombre propio, va en el offset 196 (largo de 2 bytes + texto UTF-8) y todo lo posterior se corre esos bytes.
- Offsets (formato sin nombre): `brushId` int@16, `size`@20, `maxSize`@24, `startSize`@28, `endSize`@32, `startOpacity`@36, `endOpacity`@40, `startAngle`@44, `spacing`@48, `opacityDesign`@52, `pressureSize`@60, `pressureOpacity`@64, `opacity`@84, `minSize`@152, `fadeStartLength`@364, `fadeEndLength`@368, `tipId` (16 bytes)@160, `textureId`@180, entre otros (ver código para la lista completa). Interruptores confirmados: `antialiasing` = bit 0 del byte 59, `constantOpacity` = bit 3 del byte 56, `addOpacity` = bit 6 del byte 56.
- **Conversión a pincel propio (`BrushType.fromIbis`), aproximada:**
  - `tamaño = clamp(size/2, 1, 100)`; `espaciado = clamp(spacing*2, 0.08, 1)`.
  - `falloff` desde `opacityDesign (h)`: si `h >= 0.5` duro, `min(1, t*k)` con `k = 8 + (h-0.5)*24`; si no, mezcla entre `t²` y `min(1, 2t)` con `mix = clamp((h-0.1)/0.4, 0, 1)`.
  - Dinámicas: `startSize/endSize` solo si `pressureSize > 0` (si no, 1); `startOpacity/endOpacity` solo si `pressureOpacity > 0`; `startLength = fadeStartLength*escalaInicio`, `endLength = fadeEndLength*escalaFin`; grano solo para puntas conocidas (`GRAINY_TIPS`: Carboncillo = `15f82ecdf3343f56de0d932f2c0fd40f`); `edgeAa = 1` si `antialiasing`.
  - Lo que no se aplica todavía (se guarda pero no se usa): punta con forma propia, textura, temblor, presión/velocidad, tamaño que cambia a lo largo del trazo, modos de fusión del pincel.
- Nombres de fábrica vistos: 1 Pluma (Fuerte), 4 Rotulador (Fuerte), 6 Lápiz digital, 7 Aerógrafo (Normal), 38 Marcador, 41 Carboncillo, 142 Acuarela (Mezcla), 145 Contorno.
- **Para decidir antes de portar:** esta parte es ingeniería inversa del formato de otra app. Hay que decidir si se porta tal cual a la versión de PC o si PaintCraft tiene su propio formato de pinceles.

---

## 16. Interfaz actual (solo lo que se vio en `MainActivity`)

- Pantalla única: vista 3D a pantalla completa.
- Arriba a la izquierda: botón **File** (menú New / Save / Import / Export) y botones **Deshacer / Rehacer**. Arriba a la derecha: **gizmo de ejes**.
- Abajo: **panel de pintura** (herramientas, mano, candado de cámara, pinceles, color, tamaño, opacidad) y panel de capas. Estos paneles **no se leyeron**.
- Acento naranja (242, 128, 26), fondo oscuro translúcido (32, 32, 32).
- File > Import: abre `.obj` (carga en hilo aparte). Export: diálogo con los 3 tipos y selector de destino. El menú de pinceles tiene "Importar código QR".

---

## 17. Limitaciones y qué cambia en PC

1. **Todo el pintado y la composición corren en CPU**, texel por texel, sobre `ByteArray`. Es lo que hace que no escale: una capa de 4096² pesa 64 MB (10 capas = 640 MB) y recomponer por CPU sería inviable. En PC el pincel y la composición deben pasar a la GPU.
2. Textura fija de 1024×1024 por pieza; sin mipmaps; sin resolución configurable.
3. Solo canal de color. Sin roughness, metallic, normal, height; sin preview por canal; iluminación simple, no PBR.
4. Un solo slot de guardado; el formato está pensado para Android (`org.json`, rutas internas).
5. Pinceles: sin forma de punta propia ni texturas de pincel (solo círculos con `falloff` y un grano procedural); sin presión de lápiz (el dedo no la tiene; en PC con tableta sí).
6. Sin selección de herramientas de tipo lazo/rectángulo; solo "seleccionar por opacidad".

### 17.1 [PROPUESTA] Cómo mapear a GPU (wgpu)

Ideas, no vienen del código:
- Cada capa = una textura `Rgba8Unorm` (o `Rgba16Float` si se quiere precisión); la composición como pasadas de shader (cada capa mezclada sobre un acumulador) reproduciendo las fórmulas de la sección 6.
- "Máximo de alfa por pasada" (sección 7.1) se puede hacer con una textura de trazo de un canal (`R8`/`R16Float`) usando **blend `Max`**, y al terminar cada toque (o el trazo) componer ese buffer sobre la capa. La regla de "pasada nueva tras 3 toques de hueco" es lo más difícil de replicar en GPU: hace falta decidir si se conserva o se simplifica.
- Deshacer por baldosas sigue sirviendo: copiar baldosas de textura a un buffer antes de pintar.
- Mapa de islas, cobertura UV y BVH pueden quedarse en CPU (se calculan una vez por modelo) o pasar a texturas/buffers de la GPU.

### 17.2 [PROPUESTA] Lo nuevo que aportaría "estilo Ucupaint"
Canales PBR independientes (color, roughness, metallic, normal, height), cada uno con sus capas, máscaras y modificadores; grupos; preview por canal; visor PBR; bake y exportación empaquetada (ORM, normal DirectX/OpenGL).

---

## 18. [PROPUESTA] Orden de trabajo para Rust

1. Ventana (`winit`) + `wgpu` + cámara orbital Z-up con los números de la sección 14.1, cargando un OBJ con las reglas de la sección 3.2.
2. Visor: shader de la sección 14.2 con una textura de prueba.
3. Raycast (BVH) + pintar un color en una textura por GPU (un solo pincel `SOFT`).
4. Capas, modos de mezcla y recorte en GPU (secciones 4–6).
5. Resto de pinceles, grano, dinámicas, borrador, cuentagotas, difuminar, smudge, relleno.
6. Deshacer/rehacer, guardado y exportación PNG.
7. Interfaz con `egui` (paneles de capas, color, pinceles).
8. Canales PBR y visor PBR.

Para comprobar que el portado es fiel, conviene guardar **casos de prueba de referencia** desde la app Android (un trazo con un pincel conocido, una composición con N capas y modos) y comparar el resultado píxel a píxel con la versión en Rust.
