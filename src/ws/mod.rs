//! Utility for automatically setting up WebSocket communication.

use std::collections::HashMap;

use bevy::{ecs::prelude::*, prelude::*, tasks::*};
use deref_derive::{Deref, DerefMut};
#[allow(unused_imports)]
use futures_util::{
	stream::{SplitSink, SplitStream},
	FutureExt, SinkExt, StreamExt,
};
use axum::extract::ws::{Message as WsMessage, WebSocket};

#[allow(unused_imports)]
use crate::{
	auxiliary_index::AuxIndex,
	par_events::{ParEventReader, ParEventWriter},
};

#[cfg(all(feature = "json", feature = "msgpack"))]
compile_error!("features `json` and `msgpack` are mutually exclusive");

/// Register ws subsystem.
pub fn register_ws_subsystem<'de, ReqE, ResE, ErrE>(app: &mut App) -> WsConns
where
	ReqE: bevy::ecs::event::Event + serde::de::DeserializeOwned,
	ResE: bevy::ecs::event::Event + serde::Serialize,
	ErrE: bevy::ecs::event::Event + serde::Serialize + From<wire::NetworkError>,
{
	let wsconns = WsConns::default();
	SessionToEntityMap::new().register(app);
	UserSessionsMap::new().register(app);
	app.insert_resource(wsconns.clone());

	app.add_systems(bevy::app::First, acceptor);
	#[cfg(feature = "json")]
	{
		app.add_systems(crate::schedules::Input, listener_json::<ReqE, ErrE>);
		app.add_systems(crate::schedules::Output, sender_json::<ResE, ErrE>);
	}
	#[cfg(feature = "msgpack")]
	{
		app.add_systems(crate::schedules::Input, listener_msgpack::<ReqE, ErrE>);
		app.add_systems(crate::schedules::Output, sender_msgpack::<ResE, ErrE>);
	}

	wsconns
}

/// A websocket connection associated with a user.
pub struct WsConn {
	pub user_id: wire::UserId,
	pub reader: SplitStream<WebSocket>,
	pub writer: SplitSink<WebSocket, WsMessage>,
}

/// A safe shared collection of ws connections.
#[derive(Default, Resource, Clone)]
pub struct WsConns {
	conns: std::sync::Arc<std::sync::Mutex<Vec<WsConn>>>,
}

impl WsConns {
	/// Locks the collection and drains the contents.
	pub fn take(&self) -> Vec<WsConn> {
		self.conns.lock().expect("lock should not be poisoned").drain(..).collect()
	}
}

/// An auxiliary index used to map a [`wire::SessionId`] to an [`Entity`].
pub type SessionToEntityMap = AuxIndex<wire::SessionId, SessionId>;

/// An auxiliary index used to track user sessions.
#[derive(Resource, Debug, Default, Clone)]
pub struct UserSessionsMap(HashMap<wire::UserId, Vec<wire::SessionId>>);

impl UserSessionsMap {
	/// Creates a new instance of the map.
	pub fn new() -> Self {
		Self::default()
	}

	/// Registers the [`TargetToGameMap`] as a resource.
	pub fn register(self, app: &mut App) {
		app.insert_resource(self);
	}

	/// Returns a reference to the game id for the given target.
	pub fn get(&self, id: &wire::UserId) -> Option<&Vec<wire::SessionId>> {
		self.0.get(id)
	}

	/// Returns a mutable reference to the game id for the given target.
	pub fn get_mut(&mut self, id: &wire::UserId) -> Option<&mut Vec<wire::SessionId>> {
		self.0.get_mut(id)
	}

	/// Inserts a new target to the map.
	pub fn insert(&mut self, user_id: wire::UserId, session_id: wire::SessionId) {
		self.0.insert(user_id, vec![session_id]);
	}

	/// Removes a target from the map.
	pub fn remove(&mut self, id: &wire::UserId) {
		self.0.remove(id);
	}
}

#[derive(Component, Debug, Clone, Copy, Deref, DerefMut)]
pub struct UserId(pub wire::UserId);

#[derive(Component, Debug, Clone, Copy, Deref, DerefMut)]
pub struct SessionId(pub wire::SessionId);

#[derive(Component, Debug, Deref, DerefMut)]
pub struct WsRead(pub SplitStream<WebSocket>);

#[derive(Component, Debug, Deref, DerefMut)]
pub struct WsWrite(pub SplitSink<WebSocket, WsMessage>);

impl From<UserId> for wire::UserId {
	fn from(UserId(id): UserId) -> Self {
		id
	}
}

impl From<SessionId> for wire::SessionId {
	fn from(SessionId(id): SessionId) -> Self {
		id
	}
}

/// Sends events (in json format) to connected sessions.
#[cfg(feature = "json")]
pub fn sender_json<ResE, ErrE>(
	mut res_reader: ParEventReader<crate::event_wrapper::Event<wire::Res<ResE>>>,
	mut err_reader: ParEventReader<crate::event_wrapper::Event<wire::Error<ErrE>>>,
	user_sessions_map: Res<UserSessionsMap>,
	session_to_entity_map: Res<SessionToEntityMap>,
	mut query: Query<&mut WsWrite>,
) where
	ResE: bevy::ecs::event::Event + serde::Serialize,
	ErrE: bevy::ecs::event::Event + serde::Serialize,
{
	for res in res_reader.read() {
		let msg = serde_json::to_vec(&res.event).expect("should not fail ever");
		let msg = WsMessage::Binary(msg);
		sender_impl(&res.targets, msg, &user_sessions_map, &session_to_entity_map, &mut query);
	}

	for err in err_reader.read() {
		let msg = serde_json::to_vec(&err.error).expect("should not fail ever");
		let msg = WsMessage::Binary(msg);
		sender_impl(&err.to.into(), msg, &user_sessions_map, &session_to_entity_map, &mut query);
	}
}

/// Sends events (in msgpack format) to connected sessions.
#[cfg(feature = "msgpack")]
pub fn sender_msgpack<ResE, ErrE>(
	mut res_reader: ParEventReader<crate::event_wrapper::Event<wire::Res<ResE>>>,
	mut err_reader: ParEventReader<crate::event_wrapper::Event<wire::Error<ErrE>>>,
	user_sessions_map: Res<UserSessionsMap>,
	session_to_entity_map: Res<SessionToEntityMap>,
	mut query: Query<&mut WsWrite>,
) where
	ResE: bevy::ecs::event::Event + serde::Serialize,
	ErrE: bevy::ecs::event::Event + serde::Serialize,
{
	for res in res_reader.read() {
		let msg = rmp_serde::to_vec(&res.event).expect("should not fail ever");
		let msg = WsMessage::Binary(msg);
		sender_impl(&res.targets, msg, &user_sessions_map, &session_to_entity_map, &mut query);
	}

	for err in err_reader.read() {
		let msg = rmp_serde::to_vec(&err.error).expect("should not fail ever");
		let msg = WsMessage::Binary(msg);
		sender_impl(&err.to.into(), msg, &user_sessions_map, &session_to_entity_map, &mut query);
	}
}

#[allow(unused)]
fn sender_impl(
	targets: &wire::Targets,
	msg: WsMessage,
	user_sessions_map: &Res<UserSessionsMap>,
	session_to_entity_map: &Res<SessionToEntityMap>,
	query: &mut Query<&mut WsWrite>,
) {
	// TODO: figure out a better solution than `block_on`
	match &targets {
		wire::Targets::All => {
			for mut writer in query.iter_mut() {
				if let Err(err) = block_on(writer.send(msg.clone())) {
					log::debug!("socket error occurred: {}", err);
				}
			}
		},
		wire::Targets::Few(targets) => {
			for target in targets.iter() {
				match target {
					wire::Target::Auth(auth_target) => match auth_target {
						wire::AuthTarget::All(user_id) => {
							let Some(sessions) = user_sessions_map.get(&user_id) else {
								// we don't care if the session phased out by this point, just skip it
								return;
							};
							for session_id in sessions.iter() {
								let entity = session_to_entity_map.get_by_left(&session_id).expect("should exist here");
								let mut writer = query.get_mut(*entity).expect("should exist here");
								if let Err(err) = block_on(writer.send(msg.clone())) {
									log::debug!("socket error occurred: {}", err);
								}
							}
						},
						wire::AuthTarget::Specific(_user_id, session_id) => {
							let Some(entity) = session_to_entity_map.get_by_left(&session_id) else {
								// we don't care if the session phased out by this point, just skip it
								return;
							};
							let mut writer = query.get_mut(*entity).expect("should exist here");
							if let Err(err) = block_on(writer.send(msg.clone())) {
								log::debug!("socket error occurred: {}", err);
							}
						},
					},
					wire::Target::Anon(session_id) => {
						let Some(entity) = session_to_entity_map.get_by_left(&session_id) else {
							// we don't care if the session phased out by this point, just skip it
							return;
						};
						let mut writer = query.get_mut(*entity).expect("should exist here");
						if let Err(err) = block_on(writer.send(msg.clone())) {
							log::debug!("socket error occurred: {}", err);
						}
					},
				}
			}
		},
	}
}

/// Accepts new websockets from outside of bevy.
pub fn acceptor(
	mut commands: Commands,
	new_conns: Res<WsConns>,
	mut user_sessions_map: ResMut<UserSessionsMap>,
	mut conn_writer: EventWriter<crate::event_wrapper::Event<wire::Connected<wire::Undetermined>>>,
	mut first_conn_writer: EventWriter<crate::event_wrapper::Event<wire::FirstConnected<wire::Undetermined>>>,
) {
	let new_conns = new_conns.take(); // blocks, but should be brief

	// reason we manually iterate and insert is because we need the entity index
	// (for the session id) otherwise we would just use `commands.spawn_batch()`
	for new_conn in new_conns.into_iter() {
		let mut entity = commands.spawn_empty();
		let session_id = entity.id().index();
		let WsConn { user_id, reader, writer } = new_conn;
		let bundle = (SessionId(session_id), UserId(user_id), WsRead(reader), WsWrite(writer));
		entity.insert(bundle);

		// track how many sessions the user has active (in order to report status updates about his connection)
		if let Some(sessions) = user_sessions_map.get_mut(&user_id) {
			sessions.push(session_id);
			conn_writer.send(crate::event_wrapper::Event::new(wire::Connected::new(user_id, session_id)));
		} else {
			user_sessions_map.insert(user_id, session_id);
			first_conn_writer.send(crate::event_wrapper::Event::new(wire::FirstConnected::new(user_id, session_id)));
		}
	}
}

/// Listens to incoming messages (in json format) from WebSockets.
#[cfg(feature = "json")]
pub fn listener_json<ReqE, ErrE>(
	mut commands: Commands,
	mut req_writer: EventWriter<crate::event_wrapper::Event<wire::Req<ReqE>>>,
	mut disconn_writer: EventWriter<crate::event_wrapper::Event<wire::Disconnected<wire::Undetermined>>>,
	err_writer: ParEventWriter<crate::event_wrapper::Event<wire::Error<ErrE>>>,
	mut user_sessions_map: ResMut<UserSessionsMap>,
	mut query: Query<(Entity, &SessionId, &UserId, &mut WsRead)>,
) where
	ReqE: bevy::ecs::event::Event + serde::de::DeserializeOwned,
	ErrE: bevy::ecs::event::Event + From<wire::NetworkError>,
{
	for (entity, session_id, user_id, mut reader) in query.iter_mut() {
		// read until there are no more left in the stream currently
		while let Some(msg) = reader.next().now_or_never().flatten() {
			let corrid = wire::CorrelationId::new_v4();
			let target = wire::Target::new(user_id.0, session_id.0);

			// handle stream error
			let msg = match msg {
				Ok(msg) => msg,
				Err(err) => {
					err_writer.send(crate::event_wrapper::Event::new(wire::Error::new(
						target,
						wire::NetworkError::SocketError(err.to_string()),
						corrid,
					)));

					if let Some(sessions) = user_sessions_map.get_mut(&target.id()) {
						// delete the user from the sessions map if this is his last session
						if sessions.len() == 1 {
							user_sessions_map.remove(&target.id());
							commands.entity(entity).despawn();
							disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
							break;
						}
					}
					continue;
				},
			};

			// handle message
			match msg {
				WsMessage::Binary(bytes) => {
					let action = match serde_json::from_slice::<ReqE>(&bytes) {
						Ok(action) => action,
						Err(..) => {
							err_writer.send(crate::event_wrapper::Event::new(wire::Error::new(
								target,
								wire::NetworkError::InvalidMessage,
								corrid,
							)));
							continue;
						},
					};

					req_writer.send(crate::event_wrapper::Event::new(wire::Req::new(target, action, corrid)));
				},
				WsMessage::Close(..) => {
					if let Some(sessions) = user_sessions_map.get_mut(&target.id()) {
						// delete the user from the sessions map if this is his last session
						if sessions.len() == 1 {
							user_sessions_map.remove(&target.id());
							commands.entity(entity).despawn();
							disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
							break;
						}
					}
				},
				_ => {
					err_writer.send(crate::event_wrapper::Event::new(wire::Error::new(
						target,
						wire::NetworkError::InvalidMessage,
						corrid,
					)));
					continue;
				},
			}
		}
	}
}
/// Listens to incoming messages (in msgpack format) from WebSockets.
#[cfg(feature = "msgpack")]
pub fn listener_msgpack<ReqE, ErrE>(
	mut commands: Commands,
	mut req_writer: EventWriter<crate::event_wrapper::Event<wire::Req<ReqE>>>,
	mut disconn_writer: EventWriter<crate::event_wrapper::Event<wire::Disconnected<wire::Undetermined>>>,
	err_writer: ParEventWriter<crate::event_wrapper::Event<wire::Error<ErrE>>>,
	mut user_sessions_map: ResMut<UserSessionsMap>,
	mut query: Query<(Entity, &SessionId, &UserId, &mut WsRead)>,
) where
	ReqE: bevy::ecs::event::Event + serde::de::DeserializeOwned,
	ErrE: bevy::ecs::event::Event + From<wire::NetworkError>,
{
	for (entity, session_id, user_id, mut reader) in query.iter_mut() {
		// read until there are no more left in the stream currently
		while let Some(msg) = reader.next().now_or_never().flatten() {
			let corrid = wire::CorrelationId::new_v4();
			let target = wire::Target::new(user_id.0, session_id.0);

			// handle stream error
			let msg = match msg {
				Ok(msg) => msg,
				Err(err) => {
					err_writer.send(crate::event_wrapper::Event::new(wire::Error::new(
						target,
						wire::NetworkError::SocketError(err.to_string()),
						corrid,
					)));

					if let Some(sessions) = user_sessions_map.get_mut(&target.id()) {
						// delete the user from the sessions map if this is his last session
						if sessions.len() == 1 {
							user_sessions_map.remove(&target.id());
							commands.entity(entity).despawn();
							disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
							break;
						}
					}
					continue;
				},
			};

			// handle message
			match msg {
				WsMessage::Binary(bytes) => {
					let action = match rmp_serde::from_slice::<ReqE>(&bytes) {
						Ok(action) => action,
						Err(..) => {
							err_writer.send(crate::event_wrapper::Event::new(wire::Error::new(
								target,
								wire::NetworkError::InvalidMessage,
								corrid,
							)));
							continue;
						},
					};

					req_writer.send(crate::event_wrapper::Event::new(wire::Req::new(target, action, corrid)));
				},
				WsMessage::Close(..) => {
					if let Some(sessions) = user_sessions_map.get_mut(&target.id()) {
						// delete the user from the sessions map if this is his last session
						if sessions.len() == 1 {
							user_sessions_map.remove(&target.id());
							commands.entity(entity).despawn();
							disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
							break;
						}
					}
				},
				_ => {
					err_writer.send(crate::event_wrapper::Event::new(wire::Error::new(
						target,
						wire::NetworkError::InvalidMessage,
						corrid,
					)));
					continue;
				},
			}
		}
	}
}
