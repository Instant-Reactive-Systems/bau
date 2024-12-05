//! Tick-deferred commands.
//!
//! A workaround for [`CommandQueue`] not having the `append()` method.
//!
//! [`CommandQueue`]: https://docs.rs/bevy/latest/bevy/ecs/system/struct.CommandQueue.html

use bevy::{
	ecs::{
		component::Tick,
		system::{ReadOnlySystemParam, SystemMeta, SystemParam},
		world::{CommandQueue, unsafe_world_cell::UnsafeWorldCell},
	},
	prelude::*,
};

// todo: rework after bevy 0.13 adds `append()` to `CommandQueue`

/// Command storage for tick-deferred commands.
///
/// A workaround for [`CommandQueue`] not having the `append()` method.
///
/// [`CommandQueue`]: https://docs.rs/bevy/latest/bevy/ecs/system/struct.CommandQueue.html
#[derive(Resource, Default, Deref, DerefMut)]
pub struct TickDeferredCommandStorage(Vec<CommandQueue>);

impl TickDeferredCommandStorage {
	/// Registers the resource and systems for tick-deferred commands.
	pub fn register(self, app: &mut App) {
		if app.world().contains_resource::<TickDeferredCommandStorage>() {
			return;
		}
		app.insert_resource(self);
		app.add_systems(bevy::app::Last, (apply_deferred, apply_tick_deferred_commands));
	}
}

/// Applies deferred commands.
fn apply_tick_deferred_commands(world: &mut World) {
	world.resource_scope(|world, mut command_storage: Mut<TickDeferredCommandStorage>| {
		for mut queue in command_storage.drain(..) {
			queue.apply(world);
		}
	});
}

/// A system parameter that provides concurrent access to the [`TickDeferredCommandQueue`].
///
/// # Example
/// ```
/// # use bevy::prelude::*;
/// # use bau::prelude::*;
/// fn example_system(mut commands: TickDeferredCommands) {
/// 	commands.add(|_world: &mut World| {
/// 		println!("Hello!");
/// 	});
/// }
/// ```
#[derive(Deref, DerefMut)]
pub struct TickDeferredCommands<'w, 's> {
	commands: Commands<'w, 's>,
}

// SAFETY: Only local state is accessed.
unsafe impl ReadOnlySystemParam for TickDeferredCommands<'_, '_> {}

// SAFETY: Only local state is accessed.
unsafe impl SystemParam for TickDeferredCommands<'_, '_> {
	type Item<'w, 's> = TickDeferredCommands<'w, 's>;
	type State = CommandQueue;

	fn init_state(_world: &mut World, _system_meta: &mut SystemMeta) -> Self::State {
		default()
	}

	fn apply(state: &mut Self::State, _system_meta: &SystemMeta, world: &mut World) {
		let queue = std::mem::take(state);

		world.resource_mut::<TickDeferredCommandStorage>().push(queue);
	}

	unsafe fn get_param<'w, 's>(state: &'s mut Self::State, _system_meta: &SystemMeta, world: UnsafeWorldCell<'w>, _change_tick: Tick) -> Self::Item<'w, 's> {
		TickDeferredCommands {
			commands: Commands::new_from_entities(state, world.entities()),
		}
	}
}
