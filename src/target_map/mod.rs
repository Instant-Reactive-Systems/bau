//! Auxiliary index for targets.
//!
//! Maps targets to an arbitrary type, used to connect a target to arbitrary data.

use bevy::prelude::*;
use std::collections::HashMap;

/// An event used to notify when a new target has joined the data.
#[derive(Clone)]
pub struct TargetJoined<T> {
	pub target: wire::Target,
	pub value: T,
}

impl<T> TargetJoined<T> {
	pub fn new(target: wire::Target, value: T) -> Self {
		Self { target, value }
	}
}

/// An event used to notify when a target has left the data.
#[derive(Clone)]
pub struct TargetLeft<T> {
	pub target: wire::Target,
	_phantom: std::marker::PhantomData<T>,
}

impl<T> TargetLeft<T> {
	pub fn new(target: wire::Target) -> Self {
		Self { target, _phantom: Default::default() }
	}
}

/// Auxiliary index for targets.
///
/// Maps targets to an arbitrary type, used to connect a target to arbitrary data.
///
/// # Note
/// Connects all target sessions of a particular user to the data.
#[derive(Resource)]
pub struct TargetMap<T>(HashMap<wire::Target, T>)
where
	T: Clone + Send + Sync + 'static;

impl<T> TargetMap<T>
where
	T: Clone + Send + Sync + 'static,
{
	/// Creates a new instance of the map.
	pub fn new() -> Self {
		Self(Default::default())
	}

	/// Registers itself as a resource.
	pub fn register(self, app: &mut App) {
		app.insert_resource(self);
		app.add_event::<crate::event_wrapper::Event<TargetJoined<T>>>();
		app.add_event::<crate::event_wrapper::Event<TargetLeft<T>>>();
		app.add_systems(crate::schedules::PostInput, Self::on_target_change);
	}

	/// Checks if the given target is in the map.
	pub fn contains(&self, target: &wire::Target) -> bool {
		self.0.contains_key(&Self::transform_target(target))
	}

	/// Returns a reference to the value for the given target.
	pub fn get(&self, target: &wire::Target) -> Option<&T> {
		self.0.get(&Self::transform_target(target))
	}

	/// Returns a mutable reference to the value for the given target.
	pub fn get_mut(&mut self, target: &wire::Target) -> Option<&mut T> {
		self.0.get_mut(&Self::transform_target(target))
	}

	/// Inserts a new target to the map.
	pub fn insert(&mut self, target: &wire::Target, value: T) {
		self.0.insert(Self::transform_target(target), value);
	}

	/// Removes a target from the map.
	pub fn remove(&mut self, target: &wire::Target) {
		self.0.remove(&Self::transform_target(target));
	}

	/// Transforms the target into a general target.
	fn transform_target(target: &wire::Target) -> wire::Target {
		match target {
			wire::Target::Anon(..) => *target,
			wire::Target::Auth(auth_target) => wire::Target::Auth(wire::AuthTarget::All(auth_target.id())),
		}
	}
}

impl<T> TargetMap<T>
where
	T: Clone + Send + Sync + 'static,
{
	fn on_target_change(
		mut map: ResMut<Self>,
		mut participant_added_reader: EventReader<crate::event_wrapper::Event<TargetJoined<T>>>,
		mut participant_left_reader: EventReader<crate::event_wrapper::Event<TargetLeft<T>>>,
	) {
		for event in participant_added_reader.read() {
			let TargetJoined { target, value } = event.clone().into_inner();
			map.insert(&target, value);
		}

		for event in participant_left_reader.read() {
			let TargetLeft { target, .. } = event.clone().into_inner();
			map.remove(&target);
		}
	}
}

impl<T> std::fmt::Debug for TargetMap<T>
where
	T: std::fmt::Debug + Clone + Send + Sync + 'static,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct(std::any::type_name::<Self>()).field("targets", &self.0).finish()
	}
}

impl<T> PartialEq for TargetMap<T>
where
	T: PartialEq + Clone + Send + Sync + 'static,
{
	fn eq(&self, other: &Self) -> bool {
		self.0.eq(&other.0)
	}
}

impl<T> Eq for TargetMap<T> where T: Eq + Clone + Send + Sync + 'static {}
