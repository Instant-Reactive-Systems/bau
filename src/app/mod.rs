//! Wrapper around `[bevy::app::App]` offering additional functionality such as
//! external shutdown signalling and communication bridging.

use std::time::Duration;
use bevy::prelude::*;
use tokio::sync::{mpsc, oneshot};

/// A typedef around a [`mpsc::Receiver`] receiving new connections.
pub type ConnReceiver<TReq, TRes, TErr> = mpsc::Receiver<crate::conns::Conn<TReq, TRes, TErr>>;

/// A typedef around a [`oneshot::Receiver`] receiving shutdown signal.
pub type EngineShutdownReceiver = oneshot::Receiver<()>;

/// A [`bevy`] app engine with external shutdown signalling.
pub struct App {
	app: bevy::app::App,
}

impl App {
	/// Creates a new [`Engine`].
	pub fn new() -> Self {
		Self::default()
	}

	/// Inserts a bridge between the external system and the engine.
	pub fn with_bridge<TReq, TRes, TErr>(mut self, bridge: crate::conns::Bridge<TReq, TRes, TErr>) -> Self
	where
		TReq: Clone + std::fmt::Debug + bevy::ecs::event::Event + serde::de::DeserializeOwned,
		TRes: Clone + std::fmt::Debug + bevy::ecs::event::Event + serde::Serialize,
		TErr: Clone + std::fmt::Debug + bevy::ecs::event::Event + serde::Serialize + From<wire::NetworkError>,
	{
		crate::conns::register_conns_bridge(&mut self.app, bridge);
		self
	}

	/// Enables the engine to be shutdown from the outside via a oneshot signal.
	pub fn with_external_shutdown(mut self, rx: EngineShutdownReceiver) -> Self {
		self.app.insert_resource(ShutdownReceiver(rx));
		self.app.add_systems(bevy::app::First, process_exit_message);
		self
	}

	/// Adds a [`bevy::app::Plugin`] to the engine.
	pub fn with_plugin(mut self, plugin: impl bevy::app::Plugin) -> Self {
		self.app.add_plugins(plugin);
		self
	}

	/// Runs the app in the current thread.
	pub fn run(mut self) -> Self {
		// TODO: add custom schedule planning given the current activity
		self.app.add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)));
		self.app.run();
		self
	}

	/// Runs the app in a dedicated thread.
	pub fn run_in_thread(self) -> std::thread::JoinHandle<Self> {
		std::thread::spawn(move || self.run())
	}
}

impl Default for App {
	fn default() -> Self {
		Self { app: bevy::app::App::new() }
	}
}

/// Shutdown signal resource.
#[derive(Resource)]
struct ShutdownReceiver(EngineShutdownReceiver);

/// System that handles shutdown when bridge channel is closed or shutdown signal is received.
fn process_exit_message(mut rx: ResMut<ShutdownReceiver>, mut exit: EventWriter<bevy::app::AppExit>) {
	match rx.0.try_recv() {
		Ok(..) => {
			log::info!("shutting down engine gracefully");
			exit.send(bevy::app::AppExit);
		},
		Err(err) => match err {
			oneshot::error::TryRecvError::Empty => return,
			oneshot::error::TryRecvError::Closed => {
				log::info!("shutting down engine abruptly");
				exit.send(bevy::app::AppExit);
			},
		},
	}
}
