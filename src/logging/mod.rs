//! Logging utilities.

use bevy::prelude::Event;

use crate::par_events::ParEventReader;

/// Logs all [`wire::Error<E>`]s.
///
/// # Safety
/// Must be placed in a schedule that does not run in parallel with writers given that writing
/// and reading currently is not safe (because of [`bevy`] related [issues](https://github.com/bevyengine/bevy/pull/7119)).
///
/// [`wire::Error<E>`]: https://github.com/Instant-Reactive-Systems/wire/blob/master/src/error.rs#L12
/// [`bevy`]: https://bevyengine.org/
pub fn log_errors<E: Event + std::fmt::Debug>(mut err_reader: ParEventReader<wire::Error<E>>) {
	for err in err_reader.read() {
		log::error!("{:?}", err);
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
pub fn log_responses<E: Event + std::fmt::Debug>(mut reader: ParEventReader<wire::Res<E>>) {
	for t in reader.read() {
		log::info!("{:?}", t);
	}
}
