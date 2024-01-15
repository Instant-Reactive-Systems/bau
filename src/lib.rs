//! # bau
//! A collection of utilities for [`bevy`] apps.
//! 
//! Each module is treated as it's own utility component.
//!
//! ## Features
//! - Parallel events - write to an event channel from multiple systems in parallel
//! - Deferred deletion - schedule entities for deletion from inside a system without invalidating the world for the
//!   duration of the tick
//! - Custom schedules - provides a set of common schedules for a common [`bevy`] app structure
//! - Error and response logging helper systems
//! - Auxiliary index - a map of entities to a custom ID type, used for fast lookup of entities by an arbitrary ID
//! - Tick deferred commands - schedule commands to run at the end of the tick, after all systems have run
//! - App utility extensions - adds useful methods to a [`bevy::app::App`] used for testing and debugging
//!
//! [`bevy`]: https://bevyengine.org/
//! [`bevy::app::App`]: https://docs.rs/bevy/latest/bevy/app/struct.App.html

pub mod par_events;
pub mod defer_delete;
pub mod app_ext;
pub mod schedules;
pub mod logging;
pub mod auxiliary_index;
pub mod tick_deferred_commands;

pub mod prelude {
	pub use crate::{app_ext::*, auxiliary_index::*, defer_delete::*, logging::*, par_events::*, schedules::*, tick_deferred_commands::*};
}
