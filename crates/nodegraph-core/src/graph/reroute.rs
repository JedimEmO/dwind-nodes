use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReroutePosition {
    pub x: f64,
    pub y: f64,
}

/// Marker component for reroute nodes
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IsReroute;
