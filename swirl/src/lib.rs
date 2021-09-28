#![deny(warnings)]

#[macro_use]
extern crate diesel;

#[doc(hidden)]
pub extern crate inventory;
#[doc(hidden)]
pub extern crate serde;

mod job;
mod registry;
mod runner;
mod storage;

pub mod errors;
pub mod schema;

pub use swirl_proc_macro::*;

#[doc(hidden)]
pub use serde_derive::{Deserialize, Serialize};

pub use errors::*;
pub use job::*;
pub use registry::Registry;
pub use runner::*;

#[doc(hidden)]
pub use registry::JobVTable;

pub type DieselPool = deadpool_diesel::postgres::Pool;
