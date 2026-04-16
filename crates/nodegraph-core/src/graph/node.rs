use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeHeader {
    pub title: String,
    pub color: [u8; 3],
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeTypeId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MuteState(pub bool);

/// Extra height (in pixels) reserved for custom body content below port rows.
/// When absent, falls back to `PORT_HEIGHT` if a custom body callback exists.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomBodyHeight(pub f64);
