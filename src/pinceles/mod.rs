//! Los pinceles con punta de bitmap (los que no usan solo el `falloff` de `brush.rs`). Cada pincel vive en su propio
//! archivo, para que no se mezclen:
//!
//! - `tiza.rs`: la TIZA (punta de grano que gira en cada toque, con papel).
//! - `carboncillo.rs`: el CARBONCILLO (elipse con vetas que sigue el trazo, con papel que resta).
//! - `pastel.rs`: el PASTEL (punta de grano que gira y se dispersa de lado, con lienzo que resta).
//! - `punteado.rs`: el PUNTEADO (gotas sueltas que giran, se voltean y se dispersan en los dos ejes).
//! - `follaje.rs`: el FOLLAJE (hojas de tamano, giro y color al azar, 1 o 2 por toque, que se dispersan en los dos ejes).
//! - `dual_grain.rs`: el DOBLE GRANO (disco con borde ruidoso multiplicado por una segunda punta de gotas sueltas).
//! - `esponja.rs`: la ESPONJA DE TINTA (punta de esponja con poros que gira en cada toque, con bordes mojados).
//! - `desvanecido.rs`: el DESVANECIDO de inicio y fin de trazo (opacidad de cada toque segun donde va en el trazo; del pincel Arcane, de ibisPaint).
//! - `arcane.rs`: la PUNTA de Arcane (elipse aplastada, ancha de lado a lado del trazo, del pincel de ibisPaint).
//! - `salpicadura.rs`: la SALPICADURA (racimos de gotas, 1 o 2 por toque, con tamano, giro, volteo y dispersion al azar).
//! - `pasto.rs`: el PASTO (un manojo de hojas finas que sube hacia arriba del modelo, con tamano, giro leve, volteo y
//!   dispersion de lado al azar).
//! - `semitono.rs`: los PUNTOS DE SEMITONO (disco suave cuya cobertura acumulada se mezcla con una trama de puntos en
//!   modo HardMix: pinta del todo o nada).
//! - `crayon.rs`: el CRAYON DE CERA (elipse de borde suave que sigue el trazo, con un papel que se mezcla en modo
//!   HardMix con cada toque).
//! - `gp_astillas.rs`: las ASTILLAS (fila de cuatro astillas de lado a lado del trazo; GP_BRUSH_01 del paquete de Blender
//!   `GP_Brushes_3.blend`).
//! - `gp_arco.rs`: el ARCO (pincelada curva de lado a lado del trazo; GP_BRUSH_03 del mismo paquete).
//! - `gp_puntos.rs`: la TRAMA DE PUNTOS (cuadrado de puntitos que gira con el trazo; GP_BRUSH_07 del mismo paquete).
//! - `gp_nube.rs`: la NUBE (mancha difusa de lado a lado del trazo; GP_BRUSH_02 del mismo paquete).
//! - `gp_gouache.rs`: el GOUACHE (golpe de brocha espeso con vetas; GP_BRUSH_04 del mismo paquete).
//! - `gp_mancha.rs`: la MANCHA (mancha con grano para estampar, con espacio entre toques; GP_BRUSH_08 del mismo paquete).
//! - `gp_cinta.rs`: la CINTA (banda con degradado, muy densa; GP_BRUSH_09 del mismo paquete).
//! - `gp_tinta.rs`: la TINTA RUGOSA (pincelada de tinta con vetas, de flujo bajo; "Ink Pen Rough.001" del mismo paquete).
//!
//! Lo compartido esta aparte:
//!
//! - `azar.rs`: azar determinista y ruido.
//! - `punta.rs`: el bitmap de la punta y como se lee.
//! - `papel.rs`: el papel de la tiza y del carboncillo.
//! - `medio_seco.rs`: el tipo `DryKind` y el toque `DryDab` de los cuatro pinceles de medio seco.
//! - `gp_comun.rs`: la lectura de las puntas de Blender (`assets/pinceles/*.txt`) y la orientacion de sus toques.
//!
//! Desde fuera del modulo se usa lo que se re-exporta aqui abajo (`crate::pinceles::...`).

mod azar;
mod carboncillo;
mod arcane;
mod desvanecido;
mod crayon;
mod dual_grain;
mod esponja;
mod follaje;
mod gp_arco;
mod gp_astillas;
mod gp_cinta;
mod gp_comun;
mod gp_gouache;
mod gp_mancha;
mod gp_nube;
mod gp_puntos;
mod gp_tinta;
mod medio_seco;
mod papel;
mod pastel;
mod pasto;
mod punta;
mod punteado;
mod salpicadura;
mod semitono;
mod tiza;

pub use crayon::CRAYON_REACH;
pub use dual_grain::{DUAL_GRAIN_REACH, DualDab};
pub use desvanecido::{entrada as desvanecido_entrada, toque as desvanecido_toque};
pub use esponja::wet_factor;
pub use follaje::FOLIAGE_REACH;
pub use medio_seco::{DryDab, DryKind, dry_hardmix, dry_mezcla_toque, dry_subtract};
pub use papel::paper_mask;
pub use pastel::PASTEL_SCATTER;
pub use pasto::GRASS_REACH;
pub use punteado::STIPPLE_REACH;
pub use salpicadura::SPATTER_REACH;
pub use semitono::{HALFTONE_REACH, dots_at, hard_mix};
pub use tiza::{ChalkDab, PREVIEW_UNITS_PER_PX};
