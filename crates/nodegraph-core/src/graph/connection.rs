use crate::store::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConnectionEndpoints {
    pub source_port: EntityId,
    pub target_port: EntityId,
}
