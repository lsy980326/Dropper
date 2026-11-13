mod release_guard;

pub mod command;
pub mod acceleration;
pub mod engine;
pub mod folder;
pub mod jobs;
pub mod media;
pub mod policy;
pub mod parallel;
pub mod process;
pub mod recommendation;

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub use desktop::run;
