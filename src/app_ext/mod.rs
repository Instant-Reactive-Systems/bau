//! Extension trait to [`bevy::app::App`].
//!
//! [`bevy::app::App`]: https://docs.rs/bevy/latest/bevy/app/struct.App.html

use std::{
	cmp::PartialEq,
	fmt::Debug,
	sync::atomic::{AtomicU64, Ordering},
};

use bevy::{
	ecs::{event::ManualEventReader, prelude::*, query::WorldQuery, schedule::ScheduleLabel},
	prelude::{Deref, DerefMut},
};

use crate::par_events::{ParEvents, ParManualEventReader};

/// A helper trait for enforcing bounds on assert helpers.
pub trait AssertHelper: bevy::prelude::Event + Clone + Debug + PartialEq {}

impl<T: bevy::prelude::Event + Clone + Debug + PartialEq> AssertHelper for T {}

/// Extends the `App` trait with additional utility methods.
pub trait AppExt {
	/// Sends an action from the specified target to the world.
	fn send_action<A: Event>(&mut self, target: impl Into<wire::Target>, action: A) -> u64;
	/// Observes all events of the specified type.
	fn observe_events<E: Event + Clone>(&mut self) -> Vec<E>;
	/// Observes all par events of the specified type.
	fn observe_par_events<E: Event + Clone>(&mut self) -> Vec<E>;
	/// Adds a custom schedule after the specified schedule.
	fn add_schedule_after(&mut self, schedule: impl ScheduleLabel + Clone, after: impl ScheduleLabel);
	/// Adds systems to a set to the app.
	fn add_systems_to_set<M>(&mut self, set: impl SystemSet, systems: impl IntoSystemConfigs<M>);
	/// Asserts that the tick returns the specified [`wire::Res`].
	fn assert_ok<Event: AssertHelper, Err: AssertHelper>(&mut self, expected: impl Into<Vec<wire::Res<Event>>>);
	/// Asserts that the tick returns the specified [`wire::Error`].
	fn assert_err<Event: AssertHelper, Err: AssertHelper>(&mut self, expected: impl Into<Vec<wire::Error<Err>>>);
	/// Asserts that the tick returns no [`wire::Error`].
	fn assert_no_err<Event: AssertHelper, Err: AssertHelper>(&mut self);
	/// Updates a tick and asserts that the tick returns no [`wire::Error`].
	fn update_no_err<Event: AssertHelper, Err: AssertHelper>(&mut self);
	/// Inspects the queried state of the world.
	fn inspect_state<Q: WorldQuery>(&mut self, f: impl FnMut(<<Q as WorldQuery>::ReadOnly as WorldQuery>::Item<'_>));
	/// Inspects the resource of the world.
	fn inspect_res<R: Resource>(&mut self, f: impl FnMut(&R));
}

impl AppExt for bevy::app::App {
	fn send_action<A: Event>(&mut self, target: impl Into<wire::Target>, action: A) -> u64 {
		static CORRELATION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
		let correlation_id = CORRELATION_ID_COUNTER.fetch_add(1, Ordering::AcqRel);

		self.world.send_event(wire::Req::<A>::new(target.into(), action, correlation_id));

		correlation_id
	}

	fn observe_events<E: Event + Clone>(&mut self) -> Vec<E> {
		self.world.init_resource::<Observer<E>>();

		let events_res = self.world.resource::<Events<E>>();
		// SAFETY: Used only in testing purposes where systems are controlled via manual ticks.
		let mut observer = unsafe {
			self.world
				.as_unsafe_world_cell_readonly()
				.get_resource_mut::<Observer<E>>()
				.expect("Observer resource not initialized")
		};
		observer.read(&events_res).cloned().collect()
	}

	fn observe_par_events<E: Event + Clone>(&mut self) -> Vec<E> {
		self.world.init_resource::<ParObserver<E>>();

		let events_res = self.world.resource::<ParEvents<E>>();
		// SAFETY: Used only in testing purposes where systems are controlled via manual ticks.
		let mut observer = unsafe {
			self.world
				.as_unsafe_world_cell_readonly()
				.get_resource_mut::<ParObserver<E>>()
				.expect("Observer resource not initialized")
		};
		observer.read(&events_res).cloned().collect()
	}

	fn add_schedule_after(&mut self, schedule: impl ScheduleLabel + Clone, after: impl ScheduleLabel) {
		self.init_schedule(schedule.clone());
		let mut main_schedule = self.world.resource_mut::<bevy::app::MainScheduleOrder>();
		main_schedule.insert_after(after, schedule);
	}

	fn add_systems_to_set<M>(&mut self, set: impl SystemSet, systems: impl IntoSystemConfigs<M>) {
		self.add_systems(bevy::app::Update, systems.in_set(set));
	}

	fn assert_ok<Event: AssertHelper, Err: AssertHelper>(&mut self, expected: impl Into<Vec<wire::Res<Event>>>) {
		let expected: Vec<wire::Res<Event>> = expected.into();
		let got = self.observe_par_events::<wire::Res<Event>>();
		let errs = self.observe_par_events::<wire::Error<Err>>();
		if !errs.is_empty() {
			dbg!(&errs);
			panic!("assertion failed, see above");
		}

		assert_eq!(got, expected);
	}

	fn assert_err<Event: AssertHelper, Err: AssertHelper>(&mut self, expected: impl Into<Vec<wire::Error<Err>>>) {
		let expected: Vec<wire::Error<Err>> = expected.into();
		let got = self.observe_par_events::<wire::Error<Err>>();
		let events = self.observe_par_events::<wire::Res<Event>>();
		if !events.is_empty() {
			dbg!(&events);
			panic!("assertion failed, see above");
		}

		assert_eq!(got, expected);
	}

	fn assert_no_err<Event: AssertHelper, Err: AssertHelper>(&mut self) {
		self.observe_par_events::<wire::Res<Event>>(); // read events to clear them
		let errs = self.observe_par_events::<wire::Error<Err>>();
		if !errs.is_empty() {
			dbg!(&errs);
			panic!("assertion failed, see above");
		}
	}

	fn update_no_err<Event: AssertHelper, Err: AssertHelper>(&mut self) {
		self.update();
		self.assert_no_err::<Event, Err>();
	}

	fn inspect_state<Q: WorldQuery>(&mut self, f: impl FnMut(<<Q as WorldQuery>::ReadOnly as WorldQuery>::Item<'_>)) {
		self.world.query::<Q>().iter(&self.world).for_each(f);
	}

	fn inspect_res<R: Resource>(&mut self, mut f: impl FnMut(&R)) {
		f(self.world.get_resource::<R>().expect("resource not found"));
	}
}

/// A wrapper type for [`ManualEventReader`] that implements [`Resource`]
/// used for observing an [`Event`].
///
/// [`ManualEventReader`]: https://docs.rs/bevy/latest/bevy/ecs/event/struct.ManualEventReader.html
/// [`Resource`]: https://docs.rs/bevy/latest/bevy/ecs/prelude/trait.Resource.html
/// [`Event`]: https://docs.rs/bevy/latest/bevy/ecs/event/trait.Event.html
#[derive(Resource, Debug, Deref, DerefMut)]
struct Observer<E: Event>(ManualEventReader<E>);

impl<E: Event> Default for Observer<E> {
	fn default() -> Self {
		Self(ManualEventReader::default())
	}
}

/// A wrapper type for [`ParManualEventReader`] that implements [`Resource`]
/// used for observing a parallel [`Event`].
///
/// [`ParManualEventReader`]: https://docs.rs/bau/latest/bau/par_events/struct.ParManualEventReader.html
/// [`Resource`]: https://docs.rs/bevy/latest/bevy/ecs/prelude/trait.Resource.html
/// [`Event`]: https://docs.rs/bevy/latest/bevy/ecs/event/trait.Event.html
#[derive(Resource, Debug, Deref, DerefMut)]
struct ParObserver<E: Event>(ParManualEventReader<E>);

impl<E: Event> Default for ParObserver<E> {
	fn default() -> Self {
		Self(ParManualEventReader::default())
	}
}
