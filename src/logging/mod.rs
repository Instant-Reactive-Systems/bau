//! Logging utilities.

use crate::par_events::ParEventReader;

use std::ops::Deref;

/// Logs all [`wire::Error<E>`]s.
///
/// # Safety
/// Must be placed in a schedule that does not run in parallel with writers given that writing
/// and reading currently is not safe (because of [`bevy`] related [issues](https://github.com/bevyengine/bevy/pull/7119)).
///
/// [`wire::Error<E>`]: https://github.com/Instant-Reactive-Systems/wire/blob/master/src/error.rs#L12
/// [`bevy`]: https://bevyengine.org/
pub fn log_errors<E: Send + Sync + std::fmt::Debug + 'static>(mut err_reader: ParEventReader<crate::event_wrapper::Event<wire::Error<E>>>) {
	for err in err_reader.read() {
		log::error!("{:?}", err.deref());
	}
}

/// Logs all [`wire::Res<E>`]s.
///
/// # Safety
/// Must be placed in a schedule that does not run in parallel with writers given that writing
/// and reading currently is not safe (because of [`bevy`] related [issues](https://github.com/bevyengine/bevy/pull/7119)).
///
/// [`wire::Res<E>`]: https://github.com/Instant-Reactive-Systems/wire/blob/master/src/res.rs#L8
/// [`bevy`]: https://bevyengine.org/
pub fn log_responses<E: Send + Sync + std::fmt::Debug + 'static>(mut reader: ParEventReader<crate::event_wrapper::Event<wire::Res<E>>>) {
	for t in reader.read() {
		log::info!("{:?}", t.deref());
	}
}
