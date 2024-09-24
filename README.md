# bau
A collection of utilities for [`bevy`] apps.

Each module is treated as it's own utility component.

## Features
- Parallel events - write to an event channel from multiple systems in parallel
- Deferred deletion - schedule entities for deletion from inside a system without invalidating the world for the
  duration of the tick
- Custom schedules - provides a set of common schedules for a common [`bevy`] app structure
- Error and response logging helper systems
- Auxiliary index - a map of entities to a custom ID type, used for fast lookup of entities by an arbitrary ID
- Tick deferred commands - schedule commands to run at the end of the tick, after all systems have run
- App utility extensions - adds useful methods to a [`bevy::app::App`] used for testing and debugging
- [`bevy::ecs::event::Event`] wrapper for all types so that they can be sent via the event pipeline in [`bevy`]

[`bevy`]: https://bevyengine.org/
[`bevy::app::App`]: https://docs.rs/bevy/latest/bevy/app/struct.App.html
[`bevy::ecs::event::Event`]: https://docs.rs/bevy/latest/bevy/ecs/event/trait.Event.html

# Adding as a dependency

```toml
[dependencies]
bau = { git = "ssh://git@github.com/Instant-Reactive-Systems/bau.git" }
```
