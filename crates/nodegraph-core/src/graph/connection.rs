use serde::{Deserialize, Serialize};
use crate::store::EntityId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectionEndpoints {
    pub source_port: EntityId,
    pub target_port: EntityId,
}
