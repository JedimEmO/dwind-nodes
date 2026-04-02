use serde::{Deserialize, Serialize};
use crate::store::EntityId;
use crate::types::socket_type::SocketType;
use crate::types::value::Value;

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortDefaultValue(pub Value);
