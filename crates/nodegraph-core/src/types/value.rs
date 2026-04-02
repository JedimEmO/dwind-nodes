use serde::{Deserialize, Serialize};

/// Runtime value for socket types. Used as default values on input ports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Float(f64),
    Int(i64),
    Bool(bool),
    Vector([f64; 3]),
    Color([f64; 4]),
    String(String),
    None,
}

impl Default for Value {
    fn default() -> Self {
        Value::None
    }
}
