//! PROTOTYPE: throwaway.
pub mod blocking;
pub mod core;
pub mod fake;
#[cfg(feature = "tokio")]
pub mod tokio;
pub use blocking::Client;
pub use core::{Entry, Error};
