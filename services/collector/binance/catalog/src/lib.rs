//! Binance catalog archiving and prospective symbol universe.
//!
//! Does not call `exchangeInfo` or a live user data stream itself — see
//! `README.md` for what still requires a real, credentialed run. This
//! crate is the construction logic: given already-obtained snapshots and
//! observations, build the monotonic universe correctly.

pub mod archive;
pub mod universe;

pub use archive::{CatalogArchive, CatalogError, CatalogSnapshot};
pub use universe::{StreamObservation, SymbolUniverse, UniverseSource};
