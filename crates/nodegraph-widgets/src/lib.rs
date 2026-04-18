#[macro_use]
extern crate dwind_macros;

pub mod bool_input;
pub mod color_input;
pub mod float_input;
pub mod int_input;
pub mod string_input;

pub use bool_input::{bool_input, BoolInputProps, BoolValueWrapper};
pub use color_input::{color_input, ColorInputProps, ColorValueWrapper};
pub use float_input::{float_input, FloatInputProps, FloatValueWrapper};
pub use int_input::{int_input, IntInputProps, IntValueWrapper};
pub use string_input::{string_input, StringInputProps, StringValueWrapper};
