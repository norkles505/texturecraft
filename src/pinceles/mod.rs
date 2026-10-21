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
//! - `salpicadura.rs`: la SALPICADURA (racimos de gotas, 1 o 2 por toque, con tamano, giro, volteo y dispersion al azar).
//!
//! Lo compartido esta aparte:
//!
//! - `azar.rs`: azar determinista y ruido.
//! - `punta.rs`: el bitmap de la punta y como se lee.
//! - `papel.rs`: el papel de la tiza y del carboncillo.
//! - `medio_seco.rs`: el tipo `DryKind` y el toque `DryDab` de los cuatro pinceles de medio seco.
//!
//! Desde fuera del modulo se usa lo que se re-exporta aqui abajo (`crate::pinceles::...`).

mod azar;
mod carboncillo;
mod dual_grain;
mod esponja;
mod follaje;
mod medio_seco;
mod papel;
mod pastel;
mod punta;
mod punteado;
mod salpicadura;
mod tiza;

pub use dual_grain::{DUAL_GRAIN_REACH, DualDab};
pub use esponja::wet_factor;
pub use follaje::FOLIAGE_REACH;
pub use medio_seco::{DryDab, DryKind, dry_subtract};
pub use papel::paper_mask;
pub use pastel::PASTEL_SCATTER;
pub use punteado::STIPPLE_REACH;
pub use salpicadura::SPATTER_REACH;
pub use tiza::{ChalkDab, PREVIEW_UNITS_PER_PX};
