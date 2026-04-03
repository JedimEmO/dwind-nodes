use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubgraphRoot(pub crate::store::EntityId);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupIOLabel(pub String);
