use serde::{Deserialize, Serialize};
use crate::store::EntityId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupMembers(pub Vec<EntityId>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupInputPorts(pub Vec<EntityId>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupOutputPorts(pub Vec<EntityId>);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubgraphRoot(pub EntityId);
