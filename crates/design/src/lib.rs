//! The design optimizer. Classical, not AI: routes along streets, places taps, inserts amps when the level
//! budget runs out, splits nodes at their limits. Each element comes out `Designed`, and the parts list
//! derives from them. See `docs/tasks/08-design-and-records.md`.
#![deny(missing_docs)]

pub mod fiber;
pub mod geo;
pub mod layout;
pub mod migrate;
pub mod optimize;
pub mod osm;
pub mod partition;
pub mod place;
pub mod route;
pub mod v2;

pub use layout::generate;
pub use optimize::{DesignError, DesignParts, NodeDesign, RegionDesign, design_region};
