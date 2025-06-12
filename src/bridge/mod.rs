use tokio::sync::mpsc::{Receiver, Sender};
use bevy::prelude::*;

use crate::{DuplexChannel, event_wrapper::Event};

/// A bridge between the `bevy` and the external system.
#[derive(Resource)]
pub struct Bridge<TReq, TRes> {
	/// Used for receiving new connections from the server.
	pub channel: DuplexChannel<TRes, TReq>,
}

/// Represents the receiving end of the connection.
#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MsgRead<TReq>(pub Receiver<TReq>);

/// Represents the write end of the connection.
#[derive(Resource, Debug, Deref, DerefMut)]
pub struct MsgWrite<TRes>(pub Sender<TRes>);

/// Registers a bridge to the `bevy::app::App`.
pub fn register_bridge<TReq, TRes>(app: &mut App, bridge: Bridge<TReq, TRes>)
where
	TReq: std::fmt::Debug + Send + Sync + 'static,
	TRes: Clone + std::fmt::Debug + Send + Sync + 'static,
{
	app.insert_resource(MsgRead(bridge.channel.rx)).insert_resource(MsgWrite(bridge.channel.tx));
	app.add_event::<Event<TReq>>().add_event::<Event<TRes>>();

	app.add_systems(bevy::app::First, recv_msgs::<TReq>);
	app.add_systems(bevy::app::Last, send_msgs::<TRes>);
}

/// Receives messages from the external system.
fn recv_msgs<TReq>(mut req_writer: EventWriter<crate::event_wrapper::Event<TReq>>, mut msg_reader: ResMut<MsgRead<TReq>>)
where
	TReq: std::fmt::Debug + Send + Sync + 'static,
{
	let span = tracing::trace_span!("recv_msgs");
	let _guard = span.enter();
	loop {
		let msg = msg_reader.try_recv();
		log::debug!("msg: {msg:?}");
		match msg {
			Ok(msg) => {
				log::debug!("received a message, sending through...");
				req_writer.send(crate::event_wrapper::Event::new(msg));
			},
			Err(err) => match err {
				tokio::sync::mpsc::error::TryRecvError::Empty => break,
				tokio::sync::mpsc::error::TryRecvError::Disconnected => {
					log::warn!("external part disconnected");
				},
			},
		}
	}
}

/// Sends messages to the external system.
fn send_msgs<TRes>(mut res_reader: EventReader<crate::event_wrapper::Event<TRes>>, msg_writer: ResMut<MsgWrite<TRes>>)
where
	TRes: std::fmt::Debug + Clone + Send + Sync + 'static,
{
	let span = tracing::trace_span!("send_msgs");
	let _guard = span.enter();

	for res in res_reader.read() {
		let res = res.clone().into_inner();
		log::debug!("res: {res:?}");
		if let Err(err) = msg_writer.blocking_send(res) {
			log::error!("reader closed during sending message: {}", err);
			// TODO: Reader closed during sending of event, this should be handled next tick by receive
			// messages, is it?
		}
	}
}
