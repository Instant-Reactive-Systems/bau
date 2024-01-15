//! Auxiliary index for entities.
//! 
//! Maps entities to a custom ID type, used for fast lookup of entities by an arbitrary ID.

use std::{hash::Hash, marker::PhantomData};

use bevy::prelude::*;
use bimap::BiHashMap;

use crate::defer_delete::*;

/// A bimap from the left type to the entity.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct AuxIndex<L, Q>(BiHashMap<L, Entity>, PhantomData<Q>)
where
	L: Hash + PartialEq + Eq + Send + Sync + From<Q> + 'static,
	Q: Component + Clone + Copy;

impl<L, Q> Default for AuxIndex<L, Q>
where
	L: Hash + PartialEq + Eq + Send + Sync + From<Q> + 'static,
	Q: Component + Clone + Copy,
{
	fn default() -> Self {
		Self(Default::default(), PhantomData)
	}
}

impl<L, Q> AuxIndex<L, Q>
where
	L: Hash + PartialEq + Eq + Send + Sync + From<Q> + 'static,
	Q: Component + Clone + Copy,
{
	/// Creates a new instance of the [`AuxIndex`].
	pub fn new() -> Self {
		Self::default()
	}

	/// Registers the [`AuxIndex`] as a resource and adds the necessary systems.
	pub fn register(self, app: &mut App) {
		app.insert_resource(self);
		app.add_systems(crate::schedules::PostInput, (Self::on_add, Self::on_remove));
	}

	/// Updates the map on add.
	fn on_add(mut map: ResMut<Self>, query: Query<(Entity, &Q), Added<Q>>) {
		for (entity, q) in query.iter() {
			map.0.insert(L::from(*q), entity);
		}
	}

	/// Updates the map on delete.
	fn on_remove(mut map: ResMut<Self>, query: Query<&Q, With<Deleted>>) {
		for q in query.iter() {
			map.0.remove_by_left(&L::from(*q));
		}
	}

	/// Returns a reference to the entity.
	pub fn get_by_left(&self, k: &L) -> Option<&Entity> {
		self.0.get_by_left(k)
	}

	/// Returns a reference to the left side.
	pub fn get_by_right(&self, k: &Entity) -> Option<&L> {
		self.0.get_by_right(k)
	}
}
