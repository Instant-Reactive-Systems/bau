//! [`bevy::ecs::event::Event`] wrapper for all types so that they can be sent via the event pipeline in [`bevy`].
//!
//! [`bevy::ecs::event::Event`]: https://docs.rs/bevy/latest/bevy/ecs/event/trait.Event.html

pub struct Event<T: Send + Sync + 'static> {
	inner: T,
}

impl<T: Send + Sync + 'static> bevy::ecs::event::Event for Event<T> {}

impl<T: Send + Sync + 'static> Event<T> {
	pub fn into_inner(self) -> T {
		self.inner
	}
}

impl<T: Send + Sync + 'static> std::ops::Deref for Event<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<T: Send + Sync + 'static> std::ops::DerefMut for Event<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.inner
	}
}

impl<T: Clone + Send + Sync + 'static> Clone for Event<T> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone() }
	}
}

impl<T: Copy + Send + Sync + 'static> Copy for Event<T> {}

impl<T: std::fmt::Debug + Send + Sync + 'static> std::fmt::Debug for Event<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.inner.fmt(f)
	}
}

impl<T: std::fmt::Display + Send + Sync + 'static> std::fmt::Display for Event<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.inner.fmt(f)
	}
}

impl<T: PartialEq + Send + Sync + 'static> PartialEq for Event<T> {
	fn eq(&self, other: &Self) -> bool {
		self.inner.eq(other)
	}
}

impl<T: Eq + Send + Sync + 'static> Eq for Event<T> {}

impl<T: std::hash::Hash + Send + Sync + 'static> std::hash::Hash for Event<T> {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.inner.hash(state)
	}
}
