//! Extension trait to [`bevy::app::App`].
//!
//! [`bevy::app::App`]: https://docs.rs/bevy/latest/bevy/app/struct.App.html

use std::{cmp::PartialEq, fmt::Debug};

use bevy::ecs::{
	prelude::*,
	query::{QueryData, QueryFilter},
	schedule::ScheduleLabel,
};

use crate::{par_events::ParEvents, event_wrapper::Event};

/// A helper trait for enforcing bounds on assert helpers.
pub trait AssertHelper: Send + Sync + Clone + Debug + PartialEq + 'static {}

impl<T: Send + Sync + Clone + Debug + PartialEq + 'static> AssertHelper for T {}

/// Extends the `App` trait with additional utility methods.
pub trait AppExt {
	/// Adds a custom schedule after the specified schedule.
	fn add_schedule_after(&mut self, schedule: impl ScheduleLabel + Clone, after: impl ScheduleLabel);

	/// Adds systems to a set to the app.
	fn add_systems_to_set<M>(&mut self, set: impl SystemSet, systems: impl IntoSystemConfigs<M>);

	/// Returns all events that were queued in the last two ticks.
	fn events<E: Send + Sync + Clone + 'static>(&self) -> Vec<E>;

	/// Returns all events that were queued in the last two ticks.
	fn par_events<E: Send + Sync + Clone + 'static>(&self) -> Vec<E>;

	/// Returns the specified resource.
	fn res<R: Resource + Clone>(&self) -> R;

	/// Returns the specified component.
	fn component<C: Component + Clone>(&self) -> C;

	/// Checks if the query matches.
	fn query_matches<Q: QueryData, F: QueryFilter>(&self) -> bool;

	/// Sends an action from the specified target to the world.
	fn send_action<A: Send + Sync + 'static>(&mut self, target: impl Into<wire::Target>, action: A) -> wire::CorrelationId;

	/// Sends an event to the world.
	fn send_event<E: Send + Sync + 'static>(&mut self, event: E);
}

impl AppExt for bevy::app::App {
	fn add_schedule_after(&mut self, schedule: impl ScheduleLabel + Clone, after: impl ScheduleLabel) {
		self.init_schedule(schedule.clone());
		let mut main_schedule = self.world_mut().resource_mut::<bevy::app::MainScheduleOrder>();
		main_schedule.insert_after(after, schedule);
	}

	fn add_systems_to_set<M>(&mut self, set: impl SystemSet, systems: impl IntoSystemConfigs<M>) {
		self.add_systems(bevy::app::Update, systems.in_set(set));
	}

	fn events<E: Send + Sync + Clone + 'static>(&self) -> Vec<E> {
		let events = self.world().resource::<Events<Event<E>>>();
		let mut cursor = events.get_cursor();
		cursor.read(&events).cloned().map(Event::into_inner).collect()
	}

	fn par_events<E: Send + Sync + Clone + 'static>(&self) -> Vec<E> {
		let events = self.world().resource::<ParEvents<Event<E>>>();
		let mut reader = events.get_reader();
		reader.read(&events).cloned().map(Event::into_inner).collect()
	}

	fn res<R: Resource + Clone>(&self) -> R {
		self.world().resource::<R>().clone()
	}

	fn component<C: Component + Clone>(&self) -> C {
		// SAFETY: Holds the world mutably for a short while, then clones the specified component.
		let world = unsafe { self.world().as_unsafe_world_cell_readonly().world_mut() };
		let mut query = world.query::<&C>();
		query.single(&world).clone()
	}

	fn query_matches<Q: QueryData, F: QueryFilter>(&self) -> bool {
		// SAFETY: Holds the world mutably for a short while, then clones the specified component.
		let world = unsafe { self.world().as_unsafe_world_cell_readonly().world_mut() };
		let mut query = world.query_filtered::<Q, F>();
		let query_item = query.get_single(&world);
		query_item.is_ok()
	}

	fn send_action<A: Send + Sync + 'static>(&mut self, target: impl Into<wire::Target>, action: A) -> wire::CorrelationId {
		let corrid = wire::CorrelationId::new_v4();
		self.world_mut()
			.send_event(crate::event_wrapper::Event::new(wire::Req::<A>::new(target.into(), action, corrid)));
		corrid
	}

	fn send_event<E: Send + Sync + 'static>(&mut self, event: E) {
		self.world_mut().send_event(Event::new(event));
	}
}
