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
pub struct NodeSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NodeTypeId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MuteState(pub bool);
