//! Common schedules for an app structure.
//!
//! The full schedule graph after calling [`add_schedules`] is as follows:
//! - [`bevy::app::First`]
//! - [`Input`]
//! - [`PostInput`]
//! - [`Deletion`]
//! - [`Dispatch`]
//! - [`bevy::app::PreUpdate`]
//! - [`bevy::app::Update`]
//! - [`bevy::app::PostUpdate`]
//! - [`PreOutput`]
//! - [`Output`]
//! - [`bevy::app::Last`]
//!
//! Some omitted for brevity (like [`bevy::app::FixedUpdate`]).

use bevy::ecs::schedule::ScheduleLabel;

use crate::app_ext::AppExt;

/// Runs after [`bevy::app::First`], intended for systems that accept input from an external source.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Input;

/// Runs after [`Input`], intended for systems that need to update its auxiliary indexes.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PostInput;

/// Runs after [`PostInput`], intended for systems that need to delete entities that were deferred.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Deletion;

/// Runs after [`Deletion`], intended for systems that need to dispatch events into specific handlers.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Dispatch;

/// Runs after [`bevy::app::PostUpdate`], intended for systems that need to prepare for output.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PreOutput;

/// Runs after [`PreOutput`], intended for systems that need to output data to an external source.
#[derive(ScheduleLabel, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Output;

/// Adds the schedules to the app.
pub fn add_schedules(app: &mut bevy::app::App) {
	app.add_schedule_after(Input, bevy::app::First);
	app.add_schedule_after(PostInput, Input);
	app.add_schedule_after(Deletion, PostInput);
	app.add_schedule_after(Dispatch, Deletion);
	app.add_schedule_after(PreOutput, bevy::app::PostUpdate);
	app.add_schedule_after(Output, PreOutput);
}
