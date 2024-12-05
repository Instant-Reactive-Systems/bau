//! Parallel events for [`bevy`].
//!
//! The new type [`ParEvents`] is almost fully analogous to [`bevy`]'s [`Events`] type. The main difference is that
//! [`ParEvents`] can be written to from multiple systems in parallel, whereas [`Events`] can only be written to from
//! one system at a time.
//!
//! [`bevy`]: https://bevyengine.org/

use std::{
	cell::UnsafeCell,
	marker::PhantomData,
	sync::atomic::{AtomicUsize, Ordering},
	vec::IntoIter,
};

use bevy::{ecs::system::SystemParam, prelude::*};

/// Plugin type for registering [`ParEvents`] types.
pub struct ParEventsPlugin<E: Event> {
	_marker: PhantomData<E>,
}

impl<E: Event> Default for ParEventsPlugin<E> {
	fn default() -> Self {
		Self { _marker: Default::default() }
	}
}

impl<E: Event> std::fmt::Debug for ParEventsPlugin<E> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "ParEventsPlugin<{}>", std::any::type_name::<E>())
	}
}

impl<E: Event> Clone for ParEventsPlugin<E> {
	fn clone(&self) -> Self {
		Self { _marker: Default::default() }
	}
}

impl<E: Event> Copy for ParEventsPlugin<E> {}

impl<E: Event> PartialEq for ParEventsPlugin<E> {
	fn eq(&self, other: &Self) -> bool {
		std::ptr::eq(self, other)
	}
}

impl<E: Event> Eq for ParEventsPlugin<E> {}

impl<E: Event> std::hash::Hash for ParEventsPlugin<E> {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		std::ptr::hash(self, state)
	}
}

impl<E: Event> Plugin for ParEventsPlugin<E> {
	fn build(&self, app: &mut App) {
		if app.world().contains_resource::<ParEvents<E>>() {
			return;
		}

		app.init_resource::<ParEvents<E>>();
		app.add_systems(bevy::app::First, event_update_system::<E>);
	}

	fn is_unique(&self) -> bool {
		false
	}
}

/// Implements [`Sync`] for manually provable safe [`UnsafeCell`] usage.
#[derive(Debug, Default)]
pub struct SafeUnsafeCell<T>(pub UnsafeCell<T>);
unsafe impl<T> Sync for SafeUnsafeCell<T> {}

impl<T> std::ops::Deref for SafeUnsafeCell<T> {
	type Target = UnsafeCell<T>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<T> std::ops::DerefMut for SafeUnsafeCell<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

/// A unique identifier for an event.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParEventId<E: Event> {
	id: usize,
	_marker: PhantomData<E>,
}

impl<E: Event> ParEventId<E> {
	pub fn new(id: usize) -> Self {
		Self { id, _marker: Default::default() }
	}
}

impl<E: Event> Copy for ParEventId<E> {}
impl<E: Event> Clone for ParEventId<E> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<E: Event> std::fmt::Display for ParEventId<E> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		<Self as std::fmt::Debug>::fmt(self, f)
	}
}

impl<E: Event> std::fmt::Debug for ParEventId<E> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "event<{}>#{}", std::any::type_name::<E>().split("::").last().unwrap(), self.id)
	}
}

/// An ID and an event pair.
#[derive(Debug)]
struct ParEventInstance<E: Event> {
	pub event_id: ParEventId<E>,
	pub event: E,
}

/// A parallel event storage.
///
/// # Safety
/// This type is only safe to use in parallel with [`ParEventReader`]s and [`ParEventWriter`]s.
/// It must not be read and written to at the same time, but it may be written to in parallel.
///
/// # Example
/// For an example of how to use this type, it is almost fully analogous to [`bevy`]'s
/// [`Events`] type.
///
/// [`bevy`]: https://bevyengine.org/
/// [`Events`]: https://docs.rs/bevy/latest/bevy/ecs/struct.Events.html
#[derive(Resource, Debug)]
pub struct ParEvents<E: Event> {
	events_a: SafeUnsafeCell<Vec<UnsafeCell<Vec<ParEventInstance<E>>>>>,
	events_b: SafeUnsafeCell<Vec<UnsafeCell<Vec<ParEventInstance<E>>>>>,
	event_count: AtomicUsize,
}

impl<E: Event> Default for ParEvents<E> {
	fn default() -> Self {
		let this = Self {
			events_a: Default::default(),
			events_b: Default::default(),
			event_count: Default::default(),
		};

		unsafe { this.add_slot() }; // slot 0 reserved for default outside system access
		this
	}
}

impl<E: Event> ParEvents<E> {
	/// “Sends” an event by writing it to the current event buffer. [`ParEventReader`]s can then read the event.
	///
	/// # Safety
	/// This method is only safe if a reader and writer are not active in parallel.
	pub unsafe fn send(&self, slot_index: usize, event: E) {
		let event_id = ParEventId::<E> {
			id: self.event_count.fetch_add(1, Ordering::AcqRel),
			_marker: Default::default(),
		};

		let event_instance = ParEventInstance { event_id, event };
		self.get_events_b_slot_mut(slot_index).push(event_instance);
	}

	/// Sends the default value of the event. Useful when the event is an empty struct.
	///
	/// # Safety
	/// This method is only safe if a reader and writer are not active in parallel.
	pub unsafe fn send_default(&self, slot_index: usize)
	where
		E: Default,
	{
		self.send(slot_index, Default::default());
	}

	/// Gets a new [`ManualEventReader`]. This will include all events already in the event buffers.
	pub fn get_reader(&self) -> ParManualEventReader<E> {
		ParManualEventReader::default()
	}

	/// Gets a new [`ManualEventReader`]. This will ignore all events already in the event buffers. It will read all
	/// future events.
	pub fn get_reader_current(&self) -> ParManualEventReader<E> {
		ParManualEventReader {
			last_event_count: self.event_count.load(Ordering::Acquire),
			..Default::default()
		}
	}

	/// Swaps the event buffers and clears the oldest event buffer. In general, this should be called once per
	/// frame/update.
	///
	/// If you need access to the events that were removed, consider using [`ParEvents::update_drain`].
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	pub unsafe fn update(&self) {
		let _ = self.update_drain().for_each(drop); // consume the iterator because lazy iterators
	}

	/// Swaps the event buffers and drains the oldest event buffer, returning an iterator of all events that were
	/// removed. In general, this should be called once per frame/update.
	///
	/// If you do not need to take ownership of the removed events, use [`ParEvents::update`] instead.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	#[must_use = "If you do not need the returned events, call .update() instead."]
	pub unsafe fn update_drain(&self) -> impl Iterator<Item = E> + '_ {
		std::mem::swap(self.get_events_a_mut(), self.get_events_b_mut());

		let iter = self
			.get_events_b_mut()
			.iter_mut()
			.map(|events| (*events.get()).drain(..))
			.flatten()
			.map(|x| x.event);

		iter
	}

	/// Removes all events.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	pub unsafe fn clear(&self) {
		self.get_events_a_mut().iter_mut().for_each(|events| (*events.get()).clear());
		self.get_events_b_mut().iter_mut().for_each(|events| (*events.get()).clear());
	}

	/// Creates a draining iterator that removes all events.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	pub unsafe fn drain(&self) -> impl Iterator<Item = E> + '_ {
		let iter_a = self.get_events_a_mut().iter_mut().map(|events| (*events.get()).drain(..)).flatten();
		let iter_b = self.get_events_b_mut().iter_mut().map(|events| (*events.get()).drain(..)).flatten();
		let mut drained = iter_a.chain(iter_b).collect::<Vec<_>>();
		drained.sort_by(|a, b| a.event_id.id.cmp(&b.event_id.id));

		drained.into_iter().map(|x| x.event)
	}

	/// Extends a collection with the contents of an iterator.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	pub unsafe fn extend(&self, slot_index: usize, iter: impl IntoIterator<Item = E>) {
		let events = iter.into_iter().map(|event| {
			let event_id = ParEventId::new(self.event_count.fetch_add(1, Ordering::AcqRel));

			ParEventInstance { event_id, event }
		});

		self.get_events_b_slot_mut(slot_index).extend(events);
	}

	/// Adds a new event slot. This is useful for when you need to send events from multiple systems.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	pub unsafe fn add_slot(&self) -> usize {
		let slot_index = self.get_events_a().len();
		self.get_events_a_mut().push(Default::default());
		self.get_events_b_mut().push(Default::default());
		slot_index
	}

	/// Returns the number of events currently stored in the event buffer.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	#[inline]
	pub unsafe fn len(&self) -> usize {
		let len_a = self.get_events_a().iter().map(|events| (*events.get()).len()).sum::<usize>();
		let len_b = self.get_events_b().iter().map(|events| (*events.get()).len()).sum::<usize>();
		len_a + len_b
	}

	/// Returns true if there are no events currently stored in the event buffer.
	///
	/// # Safety
	/// This method is only safe to call in an exclusive system or when manually ticking.
	#[inline]
	pub unsafe fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Returns all A event slots.
	unsafe fn get_events_a(&self) -> &Vec<UnsafeCell<Vec<ParEventInstance<E>>>> {
		&*self.events_a.get()
	}

	/// Returns all A event slots.
	unsafe fn get_events_a_mut(&self) -> &mut Vec<UnsafeCell<Vec<ParEventInstance<E>>>> {
		&mut *self.events_a.get()
	}

	/// Returns all B event slots.
	unsafe fn get_events_b(&self) -> &Vec<UnsafeCell<Vec<ParEventInstance<E>>>> {
		&*self.events_b.get()
	}

	/// Returns all B event slots.
	unsafe fn get_events_b_mut(&self) -> &mut Vec<UnsafeCell<Vec<ParEventInstance<E>>>> {
		&mut *self.events_b.get()
	}

	/// Returns a specific B event slot by slot index.
	unsafe fn get_events_b_slot_mut(&self, slot_index: usize) -> &mut Vec<ParEventInstance<E>> {
		&mut *self.get_events_b().get(slot_index).expect("a slot with that index should have existed").get()
	}
}

/// Reads events of type `E` in order and tracks which events have already been read.
#[derive(SystemParam)]
pub struct ParEventReader<'w, 's, E: Event> {
	reader: Local<'s, ParManualEventReader<E>>,
	events: Res<'w, ParEvents<E>>,
}

impl<'w, 's, E: Event> ParEventReader<'w, 's, E> {
	/// Iterates over the events this [`ParEventReader`] has not seen yet.
	/// This updates the [`ParEventReader`]’s event counter, which means subsequent event reads will not include events
	/// that happened before now.
	pub fn read(&mut self) -> ParEventIterator<'_, E> {
		self.reader.read(&self.events)
	}

	/// Like [`read`], except also returning the [`ParEventId`] of the events.
	pub fn read_with_id(&mut self) -> ParEventIteratorWithId<'_, E> {
		self.reader.read_with_id(&self.events)
	}

	/// Consumes all available events.
	///
	/// This means these events will not appear in calls to [`ParEventReader::read()`] or
	/// [`ParEventReader::read_with_id()`] and [`ParEventReader::is_empty()`] will return true.
	///
	/// For usage, see [`ParEventReader::is_empty()`].
	pub fn clear(&mut self) {
		self.reader.clear(&self.events)
	}

	/// Determines the number of events available to be read from this [`ParEventReader`] without consuming any.
	pub fn len(&self) -> usize {
		self.reader.len(&self.events)
	}

	/// Returns true if there are no events available to read.
	///
	/// # Example
	///
	/// The following example shows a useful pattern where some behavior is triggered if new events are available.
	/// [`ParEventReader::clear()`] is used so the same events don’t re-trigger the behavior the next time the system
	/// runs.
	///
	/// ```
	/// # use bevy::prelude::*;
	/// # use bau::prelude::*;
	/// #[derive(Event)]
	/// struct CollisionEvent;
	///
	/// fn play_collision_sound(mut events: ParEventReader<CollisionEvent>) {
	/// 	if !events.is_empty() {
	/// 		events.clear();
	/// 		// Play a sound
	/// 	}
	/// }
	/// ```
	pub fn is_empty(&self) -> bool {
		self.reader.is_empty(&self.events)
	}
}

/// Sends events of type `T`.
pub struct ParEventWriter<'w, E: Event> {
	slot_index: usize,
	events: &'w ParEvents<E>,
}

// SAFETY: This impl is only safe insofar as two writers cannot exist on the same system.
// It is unsafe insofar as readers and writers can have access to the resource at the same time,
// which is wrong and should be changed once `SystemMeta` adds the required API to make it possible
// to enforce exclusion of simultaneous writers and readers.
unsafe impl<'a, E: Event> SystemParam for ParEventWriter<'a, E> {
	type Item<'w, 's> = ParEventWriter<'w, E>;
	type State = ParManualEventWriter<E>;

	fn init_state(world: &mut World, system_meta: &mut bevy::ecs::system::SystemMeta) -> Self::State {
		let _ = world.init_resource::<ParEvents<E>>();
		let mut par_events = world.resource_mut::<ParEvents<E>>();
		let slot_index = (*par_events.events_a.0.get_mut()).len();
		par_events.events_a.get_mut().push(Default::default());
		par_events.events_b.get_mut().push(Default::default());

		// TODO: this allows mutual access between writers *and* readers, make it so it's
		// exclusionary
		Res::<ParEvents<E>>::init_state(world, system_meta);

		ParManualEventWriter::new(slot_index)
	}

	unsafe fn get_param<'w, 's>(
		state: &'s mut Self::State,
		system_meta: &bevy::ecs::system::SystemMeta,
		world: bevy::ecs::world::unsafe_world_cell::UnsafeWorldCell<'w>,
		_change_tick: bevy::ecs::component::Tick,
	) -> Self::Item<'w, 's> {
		let events = world
			.get_resource::<ParEvents<E>>()
			.unwrap_or_else(|| panic!("Resource requested by {} does not exist: {}", system_meta.name(), std::any::type_name::<E>()));

		ParEventWriter { slot_index: state.slot_index, events }
	}
}

impl<'w, E: Event> ParEventWriter<'w, E> {
	/// Sends an event, which can later be read by [`ParEventReader`]s.
	///
	/// See [`ParEvents`] for details.
	pub fn send(&self, event: E) {
		unsafe { self.events.send(self.slot_index, event) }
	}

	/// Sends a list of events all at once, which can later be read by [`ParEventReader`]s. This is more efficient than
	/// sending each event individually.
	///
	/// See [`ParEvents`] for details.
	pub fn send_batch(&self, events: impl IntoIterator<Item = E>) {
		unsafe { self.events.extend(self.slot_index, events) }
	}

	/// Sends the default value of the event. Useful when the event is an empty struct.
	pub fn send_default(&self)
	where
		E: Default,
	{
		unsafe { self.events.send_default(self.slot_index) }
	}
}

/// Stores the state for a [`ParEventReader`]. Access to the [`ParEvents<E>`] resource is required to read any incoming
/// events.
#[derive(Debug)]
pub struct ParManualEventReader<E: Event> {
	last_event_count: usize,
	_marker: PhantomData<E>,
}

impl<E: Event> Default for ParManualEventReader<E> {
	fn default() -> Self {
		Self {
			last_event_count: 0,
			_marker: Default::default(),
		}
	}
}

impl<E: Event> ParManualEventReader<E> {
	/// See [`ParEventReader::read`].
	pub fn read<'a>(&'a mut self, events: &'a ParEvents<E>) -> ParEventIterator<'a, E> {
		self.read_with_id(events).without_id()
	}

	/// See [`ParEventReader::read_with_id`].
	pub fn read_with_id<'a>(&'a mut self, events: &'a ParEvents<E>) -> ParEventIteratorWithId<'a, E> {
		ParEventIteratorWithId::new(self, events)
	}

	/// See [`ParEventReader::clear`].
	pub fn clear(&mut self, events: &ParEvents<E>) {
		self.last_event_count = events.event_count.load(Ordering::Acquire);
	}

	/// See [`ParEventReader::len`].
	pub fn len(&self, events: &ParEvents<E>) -> usize {
		let iter_a = unsafe { events.get_events_a().iter().map(|events| (*events.get()).iter()).flatten() };
		let iter_b = unsafe { events.get_events_b().iter().map(|events| (*events.get()).iter()).flatten() };
		let mut event_iter = iter_a.chain(iter_b).collect::<Vec<_>>();
		event_iter.sort_by(|a, b| a.event_id.id.cmp(&b.event_id.id));

		// find the oldest event id
		if let Some(oldest_event) = event_iter.first().map(|x| x.event_id) {
			let start_index = self.last_event_count.saturating_sub(oldest_event.id);
			let unread = event_iter.len() - start_index;

			unread
		} else {
			0
		}
	}

	/// See [`ParEventReader::is_empty`].
	pub fn is_empty(&self, events: &ParEvents<E>) -> bool {
		self.len(events) == 0
	}
}

/// An iterator that yields any unread events from an [`ParEventReader`] or [`ParManualEventReader`].
pub struct ParEventIterator<'a, E: Event> {
	iter: ParEventIteratorWithId<'a, E>,
}

impl<'a, E: Event> Iterator for ParEventIterator<'a, E> {
	type Item = &'a E;

	fn next(&mut self) -> Option<Self::Item> {
		self.iter.next().map(|(event, _)| event)
	}

	fn nth(&mut self, n: usize) -> Option<Self::Item> {
		self.iter.nth(n).map(|(event, _)| event)
	}

	fn last(self) -> Option<Self::Item>
	where
		Self: Sized,
	{
		self.iter.last().map(|(event, _)| event)
	}

	fn count(self) -> usize
	where
		Self: Sized,
	{
		self.iter.count()
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.iter.size_hint()
	}
}

impl<'a, E: Event> ExactSizeIterator for ParEventIterator<'a, E> {
	fn len(&self) -> usize {
		self.iter.len()
	}
}

/// An iterator that yields any unread events (and their IDs) from an [`ParEventReader`] or [`ParManualEventReader`].
pub struct ParEventIteratorWithId<'a, E: Event> {
	reader: &'a mut ParManualEventReader<E>,
	event_iter: IntoIter<&'a ParEventInstance<E>>,
	unread: usize,
}

impl<'a, E: Event> ParEventIteratorWithId<'a, E> {
	/// Creates a new iterator that yields any events that have not yet been seen by reader.
	pub fn new(reader: &'a mut ParManualEventReader<E>, events: &'a ParEvents<E>) -> Self {
		let iter_a = unsafe { events.get_events_a().iter().map(|events| (*events.get()).iter()).flatten() };
		let iter_b = unsafe { events.get_events_b().iter().map(|events| (*events.get()).iter()).flatten() };
		let mut event_iter = iter_a.chain(iter_b).collect::<Vec<_>>();
		event_iter.sort_by(|a, b| a.event_id.id.cmp(&b.event_id.id));

		// find the oldest event id
		if let Some(oldest_event) = event_iter.first().map(|x| x.event_id) {
			let start_index = reader.last_event_count.saturating_sub(oldest_event.id);
			let unread = event_iter.len() - start_index;
			let event_iter = event_iter.drain(start_index..).collect::<Vec<_>>().into_iter();
			if reader.last_event_count < oldest_event.id {
				reader.last_event_count = oldest_event.id;
			}

			Self { reader, event_iter, unread }
		} else {
			reader.last_event_count = events.event_count.load(Ordering::Acquire);

			Self {
				reader,
				event_iter: Vec::new().into_iter(),
				unread: 0,
			}
		}
	}

	/// Iterate over only the events.
	pub fn without_id(self) -> ParEventIterator<'a, E> {
		ParEventIterator { iter: self }
	}
}

impl<'a, E: Event> Iterator for ParEventIteratorWithId<'a, E> {
	type Item = (&'a E, ParEventId<E>);

	fn next(&mut self) -> Option<Self::Item> {
		match self.event_iter.next().map(|x| (&x.event, x.event_id)) {
			Some(item) => {
				self.reader.last_event_count += 1;
				self.unread -= 1;
				Some(item)
			},
			None => None,
		}
	}

	fn nth(&mut self, n: usize) -> Option<Self::Item> {
		if let Some(ParEventInstance { event_id, event }) = self.event_iter.nth(n) {
			self.reader.last_event_count += n + 1;
			self.unread -= n + 1;
			Some((event, *event_id))
		} else {
			self.reader.last_event_count += self.unread;
			self.unread = 0;
			None
		}
	}

	fn last(self) -> Option<Self::Item>
	where
		Self: Sized,
	{
		let ParEventInstance { event_id, event } = self.event_iter.last()?;
		self.reader.last_event_count += self.unread;
		Some((event, *event_id))
	}

	fn count(self) -> usize
	where
		Self: Sized,
	{
		self.reader.last_event_count += self.unread;
		self.unread
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.event_iter.size_hint()
	}
}

impl<'a, E: Event> ExactSizeIterator for ParEventIteratorWithId<'a, E> {
	fn len(&self) -> usize {
		self.unread
	}
}

/// Stores the state for a [`ParEventWriter`]. Access to the [`ParEvents<E>`] resource is required to read any incoming
/// events.
#[derive(Debug)]
pub struct ParManualEventWriter<E: Event> {
	slot_index: usize,
	_marker: PhantomData<E>,
}

impl<E: Event> ParManualEventWriter<E> {
	/// Creates a new [`ParManualEventWriter`] with the given slot index.
	fn new(slot_index: usize) -> Self {
		Self { slot_index, _marker: Default::default() }
	}
}

/// A system that calls [`ParEvents::update`].
pub fn event_update_system<E: Event>(par_events: ResMut<ParEvents<E>>) {
	unsafe { par_events.update() };
}

/// A run condition that checks if the event’s [`event_update_system`] needs to run or not.
pub fn event_update_condition<E: Event>(events: Res<ParEvents<E>>) -> bool {
	unsafe { events.is_empty() }
}

// almost all tests are mimicked from [bevy's tests](https://github.com/bevyengine/bevy/blob/main/crates/bevy_ecs/src/event.rs#L816-L1213) for Events
#[cfg(test)]
mod tests {
	use std::collections::HashMap;

	use bevy::ecs::prelude::*;

	use super::*;

	#[derive(Event, Copy, Clone, PartialEq, Eq, Debug)]
	struct TestEvent {
		i: usize,
	}

	fn get_events<E: Event + Clone>(events: &ParEvents<E>, reader: &mut ParManualEventReader<E>) -> Vec<E> {
		reader.read(events).cloned().collect::<Vec<E>>()
	}

	#[test]
	fn test_events() {
		let events = ParEvents::<TestEvent>::default();
		let event_0 = TestEvent { i: 0 };
		let event_1 = TestEvent { i: 1 };
		let event_2 = TestEvent { i: 2 };

		let slot_index = unsafe { events.add_slot() };

		// this reader will miss event_0 and event_1 because it wont read them over the course of
		// two updates
		let mut reader_missed = events.get_reader();

		let mut reader_a = events.get_reader();

		unsafe { events.send(slot_index, event_0) };

		assert_eq!(get_events(&events, &mut reader_a), vec![event_0], "reader_a created before event receives event");
		assert_eq!(
			get_events(&events, &mut reader_a),
			vec![],
			"second iteration of reader_a created before event results in zero events"
		);

		let mut reader_b = events.get_reader();

		assert_eq!(get_events(&events, &mut reader_b), vec![event_0], "reader_b created after event receives event");
		assert_eq!(
			get_events(&events, &mut reader_b),
			vec![],
			"second iteration of reader_b created after event results in zero events"
		);

		unsafe { events.send(slot_index, event_1) };

		let mut reader_c = events.get_reader();

		assert_eq!(
			get_events(&events, &mut reader_c),
			vec![event_0, event_1],
			"reader_c created after two events receives both events"
		);
		assert_eq!(
			get_events(&events, &mut reader_c),
			vec![],
			"second iteration of reader_c created after two event results in zero events"
		);

		assert_eq!(get_events(&events, &mut reader_a), vec![event_1], "reader_a receives next unread event");

		unsafe { events.update() };

		let mut reader_d = events.get_reader();

		unsafe { events.send(slot_index, event_2) };

		assert_eq!(get_events(&events, &mut reader_a), vec![event_2], "reader_a receives event created after update");
		assert_eq!(
			get_events(&events, &mut reader_b),
			vec![event_1, event_2],
			"reader_b receives events created before and after update"
		);
		assert_eq!(
			get_events(&events, &mut reader_d),
			vec![event_0, event_1, event_2],
			"reader_d receives all events created before and after update"
		);

		unsafe { events.update() };

		assert_eq!(
			get_events(&events, &mut reader_missed),
			vec![event_2],
			"reader_missed missed events unread after two update() calls"
		);
	}

	#[derive(Event, PartialEq, Eq, Debug)]
	struct E(usize);

	fn events_clear_and_read_impl(clear_func: impl FnOnce(&ParEvents<E>)) {
		let events = ParEvents::<E>::default();
		let slot_index = unsafe { events.add_slot() };

		let mut reader = events.get_reader();

		assert!(reader.read(&events).next().is_none());

		unsafe { events.send(slot_index, E(0)) };
		assert_eq!(*reader.read(&events).next().unwrap(), E(0));
		assert_eq!(reader.read(&events).next(), None);

		unsafe { events.send(slot_index, E(1)) };
		clear_func(&events);
		assert!(reader.read(&events).next().is_none());

		unsafe { events.send(slot_index, E(2)) };
		unsafe { events.update() };
		unsafe { events.send(slot_index, E(3)) };

		assert!(reader.read(&events).eq([E(2), E(3)].iter()));
	}

	#[test]
	fn test_events_clear_and_read() {
		events_clear_and_read_impl(|events| unsafe { events.clear() });
	}

	#[test]
	fn test_events_drain_and_read() {
		events_clear_and_read_impl(|events| {
			assert!(unsafe { events.drain().eq(vec![E(0), E(1)].into_iter()) });
		});
	}

	#[test]
	fn test_events_extend_impl() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		let mut reader = events.get_reader();

		unsafe { events.extend(slot_index, vec![TestEvent { i: 0 }, TestEvent { i: 1 }]) };
		assert!(reader.read(&events).eq([TestEvent { i: 0 }, TestEvent { i: 1 }].iter()));
	}

	#[test]
	fn test_events_empty() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		assert!(unsafe { events.is_empty() });

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		assert!(!unsafe { events.is_empty() });

		unsafe { events.update() };
		assert!(!unsafe { events.is_empty() });

		// events are only empty after the second call to update
		// due to double buffering.
		unsafe { events.update() };
		assert!(unsafe { events.is_empty() });
	}

	#[test]
	fn test_event_reader_len_empty() {
		let events = ParEvents::<TestEvent>::default();
		assert_eq!(events.get_reader().len(&events), 0);
		assert!(events.get_reader().is_empty(&events));
	}

	#[test]
	fn test_event_reader_len_filled() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		assert_eq!(events.get_reader().len(&events), 1);
		assert!(!events.get_reader().is_empty(&events));
	}

	#[test]
	fn test_event_iter_len_updated() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		unsafe { events.send(slot_index, TestEvent { i: 1 }) };
		unsafe { events.send(slot_index, TestEvent { i: 2 }) };
		let mut reader = events.get_reader();
		let mut iter = reader.read(&events);
		assert_eq!(iter.len(), 3);
		iter.next();
		assert_eq!(iter.len(), 2);
		iter.next();
		assert_eq!(iter.len(), 1);
		iter.next();
		assert_eq!(iter.len(), 0);
	}

	#[test]
	fn test_event_reader_len_current() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		let reader = events.get_reader_current();
		assert!(reader.is_empty(&events));
		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		assert_eq!(reader.len(&events), 1);
		assert!(!reader.is_empty(&events));
	}

	#[test]
	fn test_event_reader_len_update() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		let reader = events.get_reader();
		assert_eq!(reader.len(&events), 2);
		unsafe { events.update() };
		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		assert_eq!(reader.len(&events), 3);
		unsafe { events.update() };
		assert_eq!(reader.len(&events), 1);
		unsafe { events.update() };
		assert!(reader.is_empty(&events));
	}

	#[test]
	fn test_event_reader_clear() {
		let mut world = World::new();
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		world.insert_resource(events);

		let mut reader = IntoSystem::into_system(|mut events: ParEventReader<TestEvent>| -> bool {
			if !events.is_empty() {
				events.clear();
				false
			} else {
				true
			}
		});
		reader.initialize(&mut world);

		let is_empty = reader.run((), &mut world);
		assert!(!is_empty, "ParEventReader should not be empty");
		let is_empty = reader.run((), &mut world);
		assert!(is_empty, "ParEventReader should be empty");
	}

	#[test]
	fn test_update_drain() {
		let events = ParEvents::<TestEvent>::default();
		let slot_index = unsafe { events.add_slot() };

		let mut reader = events.get_reader();

		unsafe { events.send(slot_index, TestEvent { i: 0 }) };
		unsafe { events.send(slot_index, TestEvent { i: 1 }) };
		assert_eq!(reader.read(&events).count(), 2);

		let mut old_events = Vec::from_iter(unsafe { events.update_drain() });
		assert!(old_events.is_empty());

		unsafe { events.send(slot_index, TestEvent { i: 2 }) };
		assert_eq!(reader.read(&events).count(), 1);

		old_events.extend(unsafe { events.update_drain() });
		assert_eq!(old_events.len(), 2);

		old_events.extend(unsafe { events.update_drain() });
		assert_eq!(old_events, &[TestEvent { i: 0 }, TestEvent { i: 1 }, TestEvent { i: 2 }]);
	}

	#[test]
	fn test_parallel() {
		use std::sync::Arc;
		let events = Arc::new(ParEvents::<TestEvent>::default());
		let slot_index_a = unsafe { events.add_slot() };
		let slot_index_b = unsafe { events.add_slot() };

		let events_a = events.clone();
		let join_handle = std::thread::spawn(move || {
			for i in 0..100 {
				unsafe { events_a.send(slot_index_a, TestEvent { i }) };
			}
		});

		for i in 0..100 {
			unsafe { events.send(slot_index_b, TestEvent { i }) };
		}

		join_handle.join().unwrap();

		// test results
		let mut values: HashMap<usize, usize> = HashMap::new();
		let mut reader = events.get_reader();
		reader.read(&events).for_each(|event| {
			*values.entry(event.i).or_insert(0) += 1;
		});

		for (k, v) in values {
			assert_eq!(v, 2, "event '{}' was not sent twice", k);
		}
	}

	// #[allow(clippy::iter_nth_zero)]
	// #[test]
	// fn test_event_iter_nth() {
	// 	use bevy_ecs::prelude::*;

	// 	let mut world = World::new();
	// 	world.init_resource::<ParEvents<TestEvent>>();
	// 	let slot_index = world.resource_mut::<ParEvents<TestEvent>>().add_slot();

	// 	world.send_event(TestEvent { i: 0 });
	// 	world.send_event(TestEvent { i: 1 });
	// 	world.send_event(TestEvent { i: 2 });
	// 	world.send_event(TestEvent { i: 3 });
	// 	world.send_event(TestEvent { i: 4 });

	// 	let mut schedule = Schedule::default();
	// 	schedule.add_systems(|mut events: ParEventReader<TestEvent>| {
	// 		let mut iter = events.read();

	// 		assert_eq!(iter.next(), Some(&TestEvent { i: 0 }));
	// 		assert_eq!(iter.nth(2), Some(&TestEvent { i: 3 }));
	// 		assert_eq!(iter.nth(1), None);

	// 		assert!(events.is_empty());
	// 	});
	// 	schedule.run(&mut world);
	// }

	// #[test]
	// fn test_event_iter_last() {
	// 	use bevy_ecs::prelude::*;

	// 	let mut world = World::new();
	// 	world.init_resource::<ParEvents<TestEvent>>();
	// 	let slot_index = world.resource_mut::<ParEvents<TestEvent>>().add_slot();

	// 	let mut reader = IntoSystem::into_system(|mut events: ParEventReader<TestEvent>| ->
	// Option<TestEvent> { events.read().last().copied() }); 	reader.initialize(&mut world);

	// 	let last = reader.run((), &mut world);
	// 	assert!(last.is_none(), "ParEventReader should be empty");

	// 	world.send_event(TestEvent { i: 0 });
	// 	let last = reader.run((), &mut world);
	// 	assert_eq!(last, Some(TestEvent { i: 0 }));

	// 	world.send_event(TestEvent { i: 1 });
	// 	world.send_event(TestEvent { i: 2 });
	// 	world.send_event(TestEvent { i: 3 });
	// 	let last = reader.run((), &mut world);
	// 	assert_eq!(last, Some(TestEvent { i: 3 }));

	// 	let last = reader.run((), &mut world);
	// 	assert!(last.is_none(), "ParEventReader should be empty");
	// }
}
