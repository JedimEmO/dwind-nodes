use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortOwner(pub EntityId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortSocketType(pub SocketType);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortIndex(pub u32);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortLabel(pub String);
