//! Utility for automatically setting up a protocol-agnostic communication from the outside.

use std::{collections::HashMap, net::SocketAddr};
use bevy::{ecs::prelude::*, prelude::*};
use deref_derive::{Deref, DerefMut};
use tokio::sync::mpsc::{Receiver, Sender};

use crate::{auxiliary_index::AuxIndex, par_events::ParEventReader, DuplexChannel};

/// Wraps the `[wire::UserId]` into a component.
#[derive(Component, Debug, Clone, Copy, Deref, DerefMut)]
pub struct UserId(pub wire::UserId);

/// Wraps the `[wire::SessionId]` into a component.
#[derive(Component, Debug, Clone, Copy, Deref, DerefMut)]
pub struct SessionId(pub wire::SessionId);

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

// A map used to map sessions to their respective entities.
pub type SessionToEntityMap = AuxIndex<wire::SessionId, SessionId>;

/// An map used to track user sessions.
#[derive(Resource, Debug, Default, Clone)]
pub struct UserSessionsMap(HashMap<wire::UserId, Vec<wire::SessionId>>);

impl UserSessionsMap {
	/// Creates a new instance of the map.
	pub fn new() -> Self {
		Self::default()
	}

	/// Registers itself as a resource.
	pub fn register(self, app: &mut App) {
		app.insert_resource(self);
	}

	/// Returns a reference to the session id for the given target.
	pub fn get(&self, id: &wire::UserId) -> Option<&Vec<wire::SessionId>> {
		self.0.get(id)
	}

	/// Returns a mutable reference to the session ids for the given target.
	pub fn get_mut(&mut self, id: &wire::UserId) -> Option<&mut Vec<wire::SessionId>> {
		self.0.get_mut(id)
	}

	/// Inserts a new user session to the map.
	///
	/// # Returns
	/// The now-current-number of sessions.
	pub fn insert(&mut self, user_id: wire::UserId, session_id: wire::SessionId) -> usize {
		let num_sessions = if let Some(sessions) = self.0.get_mut(&user_id) {
			sessions.push(session_id);
			sessions.len()
		} else {
			self.0.insert(user_id, vec![session_id]);
			1
		};

		num_sessions
	}

	/// Removes a user session from the map.
	///
	/// # Returns
	/// The now-current-number of sessions.
	pub fn remove(&mut self, user_id: wire::UserId, session_id: wire::SessionId) -> usize {
		let num_sessions = if let Some(sessions) = self.0.get_mut(&user_id) {
			let len = sessions.len();
			if len <= 1 {
				self.0.remove(&user_id);
			} else {
				sessions.retain(|session| session != &session_id);
			}

			len - 1
		} else {
			0
		};

		num_sessions
	}
}

/// Registers the connection bridge to the `bevy::app::App`.
pub fn register_conns_bridge<TReq, TRes, TErr>(app: &mut App, bridge: ConnsBridge<TReq, TRes, TErr>)
where
	TReq: Clone + std::fmt::Debug + serde::de::DeserializeOwned + Send + Sync + 'static,
	TRes: Clone + std::fmt::Debug + serde::Serialize + Send + Sync + 'static,
	TErr: Clone + std::fmt::Debug + serde::Serialize + Send + Sync + 'static,
{
	SessionToEntityMap::new().register(app);
	UserSessionsMap::new().register(app);
	app.insert_resource(bridge);

	app.add_systems(bevy::app::First, accept_connections::<TReq, TRes, TErr>);
	app.add_systems(
		crate::schedules::Input,
		receive_messages::<TReq, TRes, TErr>.after(accept_connections::<TReq, TRes, TErr>),
	);
	app.add_systems(
		crate::schedules::Output,
		send_messages::<TReq, TRes, TErr>.after(receive_messages::<TReq, TRes, TErr>),
	);
}

/// A message received from the external system.
#[derive(Debug, Clone, PartialEq)]
pub enum ExternalReq<TReq> {
	/// A user action was sent.
	UserAction(TReq),
	/// The user disconnected.
	Disconnected,
	/// The user authenticated.
	Authenticated(wire::UserId),
	/// The user unauthenticated.
	Unauthenticated,
}

/// A connection associated with a user.
#[derive(Debug)]
pub struct Conn<TReq, TRes, TErr> {
	/// The user that initiated the connection.
	pub user_id: wire::UserId,
	/// The user's socket address.
	pub user_socket_address: SocketAddr,
	/// The channel that communicates to the outside.
	pub channel: DuplexChannel<Result<wire::TimestampedEvent<TRes>, TErr>, ExternalReq<TReq>>,
}

/// A bridge between the `bevy` and the external system.
#[derive(Resource)]
pub struct ConnsBridge<TReq, TRes, TErr> {
	/// Used for receiving new connections from the server.
	pub new_conns: Receiver<Conn<TReq, TRes, TErr>>,
}

/// Represents the receiving end of the connection.
#[derive(Component, Debug, Deref, DerefMut)]
pub struct ConnRead<TReq>(pub Receiver<ExternalReq<TReq>>);

/// Represents the write end of the connection.
#[derive(Component, Debug, Deref, DerefMut)]
pub struct ConnWrite<TRes, TErr>(pub Sender<Result<wire::TimestampedEvent<TRes>, TErr>>);

/// Accepts user connections from the external system.
fn accept_connections<TReq, TRes, TErr>(
	mut commands: Commands,
	mut bridge: ResMut<ConnsBridge<TReq, TRes, TErr>>,
	mut user_sessions_map: ResMut<UserSessionsMap>,
	mut conn_writer: EventWriter<crate::event_wrapper::Event<wire::Connected<wire::Undetermined>>>,
	mut first_conn_writer: EventWriter<crate::event_wrapper::Event<wire::FirstConnected<wire::Undetermined>>>,
	mut exit: EventWriter<bevy::app::AppExit>,
) where
	TReq: Send + Sync + 'static,
	TRes: Send + Sync + 'static,
	TErr: Send + Sync + 'static,
{
	loop {
		let new_conn = match bridge.new_conns.try_recv() {
			Ok(conn) => conn,
			Err(err) => match err {
				tokio::sync::mpsc::error::TryRecvError::Empty => return, // no new connections
				tokio::sync::mpsc::error::TryRecvError::Disconnected => {
					log::error!("bridge channel closed, shutting down");
					exit.send(bevy::app::AppExit::Success);
					return;
				},
			},
		};

		// reason we manually iterate and insert is because we need the entity index
		// (for the session id) otherwise we would just use `commands.spawn_batch()`
		let mut entity = commands.spawn_empty();
		let session_id = entity.id().index();
		let Conn { user_id, user_socket_address, channel } = new_conn;

		let span = tracing::trace_span!(
			"accept_connections",
			user_id = user_id.hyphenated().to_string(),
			session_id = session_id.to_string(),
			addr = user_socket_address.to_string(),
		);
		let _guard = span.enter();

		let bundle = (SessionId(session_id), UserId(user_id), ConnRead(channel.rx), ConnWrite(channel.tx));
		entity.insert(bundle);

		// track how many sessions the user has active (in order to report status updates about his connection)
		if let Some(sessions) = user_sessions_map.get_mut(&user_id) {
			sessions.push(session_id);
			log::trace!("user now has {} sessions active", sessions.len());
			conn_writer.send(crate::event_wrapper::Event::new(wire::Connected::new(user_id, session_id)));
		} else {
			log::trace!("user just hopped on");
			user_sessions_map.insert(user_id, session_id);
			first_conn_writer.send(crate::event_wrapper::Event::new(wire::FirstConnected::new(user_id, session_id)));
		}
	}
}

/// Receives messages from the external system.
fn receive_messages<TReq, TRes, TErr>(
	mut commands: Commands,
	mut req_writer: EventWriter<crate::event_wrapper::Event<wire::Req<TReq>>>,
	mut disconn_writer: EventWriter<crate::event_wrapper::Event<wire::Disconnected<wire::Undetermined>>>,
	mut conn_writer: EventWriter<crate::event_wrapper::Event<wire::Connected<wire::Undetermined>>>,
	mut first_conn_writer: EventWriter<crate::event_wrapper::Event<wire::FirstConnected<wire::Undetermined>>>,
	mut user_sessions_map: ResMut<UserSessionsMap>,
	mut query: Query<(Entity, &SessionId, &mut UserId, &mut ConnRead<TReq>)>,
) where
	TReq: std::fmt::Debug + serde::de::DeserializeOwned + Send + Sync + 'static,
	TRes: Send + Sync + 'static,
	TErr: Send + Sync + 'static,
{
	for (entity, session_id, mut user_id, mut rx) in query.iter_mut() {
		match rx.try_recv() {
			Ok(msg) => {
				let span = tracing::trace_span!("receive_messages", user_id = user_id.hyphenated().to_string(), session_id = session_id.to_string());
				let _guard = span.enter();
				let target = wire::Target::new(user_id.0, session_id.0);
				let corrid = wire::CorrelationId::new_v4();

				match msg {
					ExternalReq::UserAction(action) => {
						log::debug!("user requested an action: {action:?}");
						req_writer.send(crate::event_wrapper::Event::new(wire::Req::new(target, action, corrid)));
					},
					ExternalReq::Disconnected => {
						let remaining = user_sessions_map.remove(user_id.0, session_id.0);
						if remaining == 0 {
							disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
							log::debug!("user disconnected, no more remaining sessions");
						} else {
							log::debug!("user disconnected, {} remaining sessions", remaining);
						}

						commands.entity(entity).despawn();
					},
					ExternalReq::Authenticated(new_user_id) => {
						if user_id.0 == new_user_id {
							log::trace!("user authenticated on an already authenticated session, skipping...");
						} else {
							// remove the session from the anonymous sessions
							let remaining = user_sessions_map.remove(user_id.0, session_id.0);

							// insert the session as an authenticated user
							user_id.0 = new_user_id;
							if let Some(sessions) = user_sessions_map.get_mut(&user_id.0) {
								sessions.push(session_id.0);
								log::trace!("user now has {} sessions active", sessions.len());
								conn_writer.send(crate::event_wrapper::Event::new(wire::Connected::new(user_id.0, session_id.0)));
							} else {
								log::trace!("user just hopped on");
								user_sessions_map.insert(user_id.0, session_id.0);
								first_conn_writer.send(crate::event_wrapper::Event::new(wire::FirstConnected::new(user_id.0, session_id.0)));
							}

							log::debug!("user is now authenticated, {remaining} anonymous sessions left");
						}
					},
					ExternalReq::Unauthenticated => {
						// remove the session from the authenticated sessions
						let remaining = user_sessions_map.remove(user_id.0, session_id.0);
						if remaining == 0 {
							disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
						}

						// insert the session as an anonymous user
						user_id.0 = wire::ANON_USER_ID;
						user_sessions_map.insert(user_id.0, session_id.0);
						log::debug!("user is now unauthenticated, {remaining} sessions left");
					},
				};
			},
			Err(err) => {
				match err {
					tokio::sync::mpsc::error::TryRecvError::Empty => {},
					tokio::sync::mpsc::error::TryRecvError::Disconnected => {
						// this branch is for when the server shuts down
						// do not log anything here because for 100+ users, you can assume how useless
						// the logs become

						user_sessions_map.remove(user_id.0, session_id.0);
						disconn_writer.send(crate::event_wrapper::Event::new(wire::Disconnected::new(user_id.0, session_id.0)));
						commands.entity(entity).despawn();
					},
				}
			},
		}
	}
}

/// Sends messages from the game engine to the server bridge server side
fn send_messages<TReq, TRes, TErr>(
	mut res_reader: ParEventReader<crate::event_wrapper::Event<wire::Res<TRes>>>,
	mut err_reader: ParEventReader<crate::event_wrapper::Event<wire::Error<TErr>>>,
	user_sessions_map: Res<UserSessionsMap>,
	session_to_entity_map: Res<SessionToEntityMap>,
	mut query: Query<&mut ConnWrite<TRes, TErr>>,
) where
	TReq: Clone + Send + Sync + 'static,
	TRes: std::fmt::Debug + Clone + serde::Serialize + Send + Sync + 'static,
	TErr: std::fmt::Debug + Clone + serde::Serialize + Send + Sync + 'static,
{
	for msg in res_reader.read() {
		send_message::<TReq, TRes, TErr>(Ok(msg.clone().into_inner()), &user_sessions_map, &session_to_entity_map, &mut query);
	}

	for msg in err_reader.read() {
		send_message::<TReq, TRes, TErr>(Err(msg.clone().into_inner()), &user_sessions_map, &session_to_entity_map, &mut query);
	}
}

/// Sends a single message to the external system.
fn send_message<TReq, TRes, TErr>(
	msg: Result<wire::Res<TRes>, wire::Error<TErr>>,
	user_sessions_map: &Res<UserSessionsMap>,
	session_to_entity_map: &Res<SessionToEntityMap>,
	query: &mut Query<&mut ConnWrite<TRes, TErr>>,
) where
	TReq: Clone + Send + Sync + 'static,
	TRes: std::fmt::Debug + Clone + serde::Serialize + Send + Sync + 'static,
	TErr: std::fmt::Debug + Clone + serde::Serialize + Send + Sync + 'static,
{
	let (msg, targets) = match msg {
		Ok(msg) => {
			let wire::Res { targets, event } = msg;
			(Ok(event), targets)
		},
		Err(err) => {
			let wire::Error { to, error, corrid: _ } = err;
			(Err(error), to.into())
		},
	};
	let span = tracing::trace_span!("send_message", targets = format!("{targets:?}"));
	let _guard = span.enter();
	log::debug!("sending a response: {msg:?}");

	match &targets {
		wire::Targets::All => {
			for writer in query.iter_mut() {
				if let Err(err) = writer.blocking_send(msg.clone()) {
					log::error!("reader closed during sending message: {}", err);
					// TODO: Reader closed during sending of event, this should be handled next tick by receive
					// messages, is it?
				}
			}
		},
		wire::Targets::Few(targets) => {
			for target in targets.iter() {
				match target {
					wire::Target::Auth(auth_target) => match auth_target {
						wire::AuthTarget::All(user_id) => {
							let Some(sessions) = user_sessions_map.get(user_id) else {
								// we don't care if the session phased out by this point, just skip it
								continue;
							};

							for session_id in sessions.iter() {
								let entity = session_to_entity_map.get_by_left(session_id).expect("should exist here");
								let writer = query.get(*entity).expect("should exist here");

								if let Err(err) = writer.blocking_send(msg.clone()) {
									log::debug!("reader closed: {}", err);
								}
							}
						},
						wire::AuthTarget::Specific(_user_id, session_id) => {
							let Some(entity) = session_to_entity_map.get_by_left(session_id) else {
								// we don't care if the session phased out by this point, just skip it
								continue;
							};
							let writer = query.get(*entity).expect("should exist here");

							if let Err(err) = writer.blocking_send(msg.clone()) {
								log::debug!("reader closed: {}", err);
							}
						},
					},
					wire::Target::Anon(session_id) => {
						let Some(entity) = session_to_entity_map.get_by_left(session_id) else {
							// we don't care if the session phased out by this point, just skip it
							continue;
						};
						let writer = query.get(*entity).expect("should exist here");

						// TODO: What happens if channel is full?
						if let Err(err) = writer.blocking_send(msg.clone()) {
							log::debug!("reader closed: {}", err);
						}
					},
				}
			}
		},
	}
}
