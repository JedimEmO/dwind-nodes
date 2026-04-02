use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SocketType {
    Float,
    Int,
    Bool,
    Vector,
    Color,
    String,
    Shader,
    Geometry,
    Object,
    Image,
    Custom(u32),
}

impl SocketType {
    /// Check if a connection from self (output) to other (input) is valid.
    /// Blender allows some implicit conversions (e.g. Float -> Int, Float -> Bool).
    pub fn is_compatible_with(&self, other: &SocketType) -> bool {
        if self == other {
            return true;
        }
        // Custom types only connect to same custom ID
        if let (SocketType::Custom(a), SocketType::Custom(b)) = (self, other) {
            return a == b;
        }
        // Numeric implicit conversions
        matches!(
            (self, other),
            (SocketType::Float, SocketType::Int)
                | (SocketType::Float, SocketType::Bool)
                | (SocketType::Int, SocketType::Float)
                | (SocketType::Int, SocketType::Bool)
                | (SocketType::Bool, SocketType::Float)
                | (SocketType::Bool, SocketType::Int)
                | (SocketType::Float, SocketType::Color)
                | (SocketType::Color, SocketType::Float)
                | (SocketType::Float, SocketType::Vector)
                | (SocketType::Vector, SocketType::Float)
        )
    }

    /// Default socket color as [r, g, b] — follows Blender's convention.
    pub fn default_color(&self) -> [u8; 3] {
        match self {
            SocketType::Float => [160, 160, 160],    // gray
            SocketType::Int => [73, 154, 73],         // green
            SocketType::Bool => [204, 169, 136],      // tan
            SocketType::Vector => [99, 99, 199],      // purple
            SocketType::Color => [200, 200, 40],      // yellow
            SocketType::String => [111, 178, 204],    // light blue
            SocketType::Shader => [57, 194, 57],      // bright green
            SocketType::Geometry => [0, 208, 172],    // teal
            SocketType::Object => [237, 145, 36],     // orange
            SocketType::Image => [160, 100, 200],     // purple-ish
            SocketType::Custom(_) => [128, 128, 128], // gray
        }
    }
}
