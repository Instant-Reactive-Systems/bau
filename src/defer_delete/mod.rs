//! Deferred deletion.
//!
//! This module provides a component and system for deferring deletion of entities.

use bevy::prelude::*;

/// Marks an entity for deletion.
#[derive(Component)]
pub struct Deleted;

/// Despawns all defer-deleted entities.
pub fn despawn_defer_deleted_entities(mut commands: Commands, entities: Query<Entity, With<Deleted>>) {
	for entity in entities.iter() {
		commands.entity(entity).despawn();
	}
}
