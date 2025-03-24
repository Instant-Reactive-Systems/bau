//! Auxiliary index for target timeouts.
//!
//! Provides a utility that emits expired timeout events if the timeout associated with the target has expired.

use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::ops::DerefMut;
use std::time::{Instant, Duration};

/// An event used to notify when a timeout has expired.
pub struct ExpiredTimeout<M> {
	pub target: wire::Target,
	_phant: std::marker::PhantomData<M>,
}

impl<M> std::fmt::Debug for ExpiredTimeout<M> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct(std::any::type_name::<Self>()).field("target", &self.target).finish()
	}
}

impl<M> Clone for ExpiredTimeout<M> {
	fn clone(&self) -> Self {
		Self {
			target: self.target,
			_phant: Default::default(),
		}
	}
}

impl<M> Copy for ExpiredTimeout<M> {}

impl<M> PartialEq for ExpiredTimeout<M> {
	fn eq(&self, other: &Self) -> bool {
		self.target.eq(&other.target)
	}
}

impl<M> Eq for ExpiredTimeout<M> {}

/// Auxiliary index for target timeouts.
///
/// Holds a sorted queue of target timeouts and dispatches an expiration event
/// if timeout exceeded the limit.
///
/// # Note
/// Does not provide a `register(&mut App)` method since it is meant to be wrapped.
#[derive(Resource)]
pub struct TimeoutMap<M>
where
	M: Send + Sync + 'static,
{
	/// A lookup table of timeout duration, timeout instant, and the player index in the queue.
	timeouts: HashMap<wire::Target, (Duration, Instant, usize)>,
	/// A queue of time-sorted targets.
	///
	/// Separated into sorted duration categories in order to properly handle timeout passes.
	queues: BTreeMap<Duration, Vec<wire::Target>>,
	_phant: std::marker::PhantomData<M>,
}

impl<M> TimeoutMap<M>
where
	M: Send + Sync + 'static,
{
	/// Creates a new instance of the map.
	pub fn new() -> Self {
		Self {
			timeouts: Default::default(),
			queues: Default::default(),
			_phant: Default::default(),
		}
	}

	/// Checks if the given target is in the map.
	pub fn contains(&self, target: &wire::Target) -> bool {
		self.timeouts.contains_key(&Self::transform_target(target))
	}

	/// Inserts a new target timeout to the map.
	pub fn insert(&mut self, target: wire::Target, duration: Duration) {
		let now = Instant::now();
		let target = Self::transform_target(&target);
		let n_in_queue = self.queues.entry(duration).or_default().len();
		self.timeouts.insert(target, (duration, now, n_in_queue));
		self.queues.get_mut(&duration).unwrap().push(target);
	}

	/// Inserts new target timeouts to the map.
	pub fn insert_many(&mut self, targets: impl IntoIterator<Item = wire::Target>, duration: Duration) {
		let now = Instant::now();
		for target in targets {
			let target = Self::transform_target(&target);
			let n_in_queue = self.queues.entry(duration).or_default().len();
			self.timeouts.insert(target, (duration, now, n_in_queue));
			self.queues.get_mut(&duration).unwrap().push(target);
		}
	}

	/// Removes a target from the map.
	pub fn remove(&mut self, target: &wire::Target) {
		let target = Self::transform_target(target);
		if let Some((duration, _, idx)) = self.timeouts.remove(&target) {
			// SAFETY: The `queue` and `timeouts` data are synchronized.
			self.queues.get_mut(&duration).unwrap().remove(idx);
			for target in &self.queues.get(&duration).unwrap()[idx..] {
				let (_, _, idx) = self.timeouts.get_mut(target).unwrap();
				*idx -= 1;
			}
		}
	}

	/// Removes targets from the map.
	pub fn remove_many(&mut self, targets: impl IntoIterator<Item = wire::Target>) {
		// TODO: is there a more efficient way of bulk deletion in this context?
		for target in targets {
			self.remove(&target);
		}
	}

	/// Transforms the target into a general target.
	fn transform_target(target: &wire::Target) -> wire::Target {
		match target {
			wire::Target::Anon(..) => *target,
			wire::Target::Auth(auth_target) => wire::Target::Auth(wire::AuthTarget::All(auth_target.id())),
		}
	}
}

impl<M> TimeoutMap<M>
where
	M: Send + Sync + 'static,
{
	/// Checks if which timeouts are expired and sends the appropriate events.
	pub fn process_timeouts(mut map: ResMut<Self>, mut expired_timeout_writer: EventWriter<crate::event_wrapper::Event<ExpiredTimeout<M>>>, time: Res<Time<Real>>) {
		let now = time.last_update().unwrap_or(Instant::now());
		let Self { timeouts, queues, _phant: _ } = map.deref_mut();
		for queue in queues.values_mut() {
			let first_nonexpired_idx = queue.iter().position(|target| {
				// SAFETY: The `queue` and `timeouts` data are synchronized.
				let (time_limit, instant, _) = timeouts.get(target).unwrap().clone();
				let current_time_span = now.saturating_duration_since(instant);
				current_time_span > time_limit
			});

			if let Some(idx) = first_nonexpired_idx {
				// update the remaining timeouts
				let n_expired = idx;
				for target in &queue[idx..] {
					// SAFETY: The `queue` and `timeouts` data are synchronized.
					let (_, _, idx) = timeouts.get_mut(&target).unwrap();
					*idx -= n_expired;
				}

				// remove the timeouts from the lookup table and publish the events
				for target in queue.drain(..idx) {
					timeouts.remove(&target);
					expired_timeout_writer.send(crate::event_wrapper::Event::new(ExpiredTimeout { target, _phant: Default::default() }));
				}
			}
		}
	}
}

impl<M> std::fmt::Debug for TimeoutMap<M>
where
	M: Send + Sync + 'static,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct(std::any::type_name::<Self>())
			.field("timeouts", &self.timeouts)
			.field("queues", &self.queues)
			.finish()
	}
}

impl<M> Clone for TimeoutMap<M>
where
	M: Send + Sync + 'static,
{
	fn clone(&self) -> Self {
		Self {
			timeouts: self.timeouts.clone(),
			queues: self.queues.clone(),
			_phant: Default::default(),
		}
	}
}

impl<M> PartialEq for TimeoutMap<M>
where
	M: Send + Sync + 'static,
{
	fn eq(&self, other: &Self) -> bool {
		self.timeouts.eq(&other.timeouts) && self.queues.eq(&other.queues)
	}
}

impl<M> Eq for TimeoutMap<M> where M: Send + Sync + 'static {}
