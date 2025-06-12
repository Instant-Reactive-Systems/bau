use tokio::sync::mpsc::{Receiver, Sender};
use bevy::prelude::*;

use crate::DuplexChannel;

/// A bridge between the `bevy` and the external system.
#[derive(Resource)]
pub struct Bridge<TReq, TRes, TErr> {
	/// Used for receiving new connections from the server.
	pub channel: DuplexChannel<Result<wire::TimestampedEvent<TRes>, TErr>, TReq>,
}

/// Represents the receiving end of the connection.
#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MsgRead<TReq>(pub Receiver<TReq>);

/// Represents the write end of the connection.
#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MsgWrite<TRes, TErr>(pub Sender<Result<wire::TimestampedEvent<TRes>, TErr>>);

/// Registers a bridge to the `bevy::app::App`.
pub fn register_bridge<TReq, TRes, TErr>(app: &mut App, bridge: Bridge<TReq, TRes, TErr>)
where
	TReq: Clone + std::fmt::Debug + serde::de::DeserializeOwned + Send + Sync + 'static,
	TRes: Clone + std::fmt::Debug + serde::Serialize + Send + Sync + 'static,
	TErr: Clone + std::fmt::Debug + serde::Serialize + Send + Sync + 'static,
{
	app.insert_resource(MsgRead(bridge.channel.rx));
	app.insert_resource(MsgWrite(bridge.channel.tx));

	app.add_systems(bevy::app::First, recv_msgs::<TReq, TRes, TErr>);
	app.add_systems(bevy::app::Last, send_msgs::<TReq, TRes, TErr>);
}

/// Receives messages from the external system.
fn recv_msgs<TReq, TRes, TErr>(mut req_writer: EventWriter<crate::event_wrapper::Event<TReq>>, mut msg_reader: ResMut<MsgRead<TReq>>)
where
	TReq: std::fmt::Debug + serde::de::DeserializeOwned + Send + Sync + 'static,
	TRes: Send + Sync + 'static,
	TErr: Send + Sync + 'static,
{
	let span = tracing::trace_span!("recv_msgs");
	let _guard = span.enter();
	loop {
		match msg_reader.try_recv() {
			Ok(msg) => {
				log::debug!("received a message, sending through...");
				req_writer.send(crate::event_wrapper::Event::new(msg));
			},
			Err(err) => match err {
				tokio::sync::mpsc::error::TryRecvError::Empty => {},
				tokio::sync::mpsc::error::TryRecvError::Disconnected => {
					log::warn!("external part disconnected");
				},
			},
		}
	}
}

/// Sends messages to the external system.
fn send_msgs<TReq, TRes, TErr>(
	mut res_reader: EventReader<crate::event_wrapper::Event<TRes>>,
	mut err_reader: EventReader<crate::event_wrapper::Event<TErr>>,
	msg_writer: ResMut<MsgWrite<TRes, TErr>>,
) where
	TReq: std::fmt::Debug + serde::de::DeserializeOwned + Send + Sync + 'static,
	TRes: Clone + Send + Sync + 'static,
	TErr: Clone + Send + Sync + 'static,
{
	let span = tracing::trace_span!("send_msgs");
	let _guard = span.enter();

	for res in res_reader.read() {
		if let Err(err) = msg_writer.blocking_send(Ok(wire::TimestampedEvent::new(res.clone().into_inner()))) {
			log::error!("reader closed during sending message: {}", err);
			// TODO: Reader closed during sending of event, this should be handled next tick by receive
			// messages, is it?
		}
	}

	for err in err_reader.read() {
		if let Err(err) = msg_writer.blocking_send(Err(err.clone().into_inner())) {
			log::error!("reader closed during sending error: {}", err);
			// TODO: Reader closed during sending of event, this should be handled next tick by receive
			// messages, is it?
		}
	}
}
