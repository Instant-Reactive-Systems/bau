//! Shared module for reporting authentication changes for non-HTTP systems.

/// Event indicating that a user was authenticated.
#[derive(Debug, Clone, Copy, bevy::ecs::event::Event, serde::Serialize, serde::Deserialize)]
pub struct Authenticated;

/// Event indicating that a user was unauthenticated.
#[derive(Debug, Clone, Copy, bevy::ecs::event::Event, serde::Serialize, serde::Deserialize)]
pub struct Unauthenticated;
