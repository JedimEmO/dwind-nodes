use crate::store::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameLabel(pub String);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameColor(pub [u8; 3]);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameMembers(pub Vec<EntityId>);
