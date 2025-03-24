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
//! - Protocol-agnostic communication - sets up everything in order to communicate via any protocol
//! - [`bevy::ecs::event::Event`] wrapper for all types so that they can be sent via the event pipeline in [`bevy`]
//! - One-line setup for creating a mixed-environment app - provides an API to spawn an app in a mixed-environment (with `axum` e.g.)
//!
//! [`bevy`]: https://bevyengine.org/
//! [`bevy::app::App`]: https://docs.rs/bevy/latest/bevy/app/struct.App.html
//! [`bevy::ecs::event::Event`]: https://docs.rs/bevy/latest/bevy/ecs/event/trait.Event.html

pub mod par_events;
pub mod defer_delete;
pub mod app_ext;
pub mod schedules;
pub mod logging;
pub mod auxiliary_index;
pub mod tick_deferred_commands;
pub mod conns;
pub mod event_wrapper;
pub mod app;
pub mod target_map;
pub mod timeout_map;

pub mod prelude {
	pub use crate::{
		app_ext::*, auxiliary_index::*, defer_delete::*, event_wrapper::*, logging::*, par_events::*, schedules::*, tick_deferred_commands::*, conns::*, app::*, target_map::*,
		timeout_map::*,
	};
}
