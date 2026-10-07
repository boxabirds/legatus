//! Simulation tests. Test sources live in the tier folders next to `src/`;
//! a tier compiles only when its Cargo feature is on.
#[cfg(feature = "tier-virtual")]
#[path = "../virtual/mod.rs"]
pub mod tier_virtual;
#[cfg(feature = "tier-scaled")]
#[path = "../scaled/mod.rs"]
pub mod tier_scaled;
#[cfg(feature = "tier-real")]
#[path = "../real/mod.rs"]
pub mod tier_real;
