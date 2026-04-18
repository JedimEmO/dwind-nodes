#[macro_use]
extern crate dwind_macros;

pub mod graph_callbacks;
pub mod graph_signals;
pub mod theme;
pub mod viewport_view;

pub(crate) mod connection_view;
pub(crate) mod context_menu;
pub(crate) mod event_bridge;
pub(crate) mod frame_view;
pub(crate) mod minimap_view;
pub(crate) mod node_view;
pub(crate) mod search_menu;

// Re-exports for ergonomic imports
pub use graph_callbacks::GraphCallbacks;
pub use graph_signals::{CustomNodeBodyFn, GraphSignals, PortWidgetFn};
pub use theme::Theme;
pub use viewport_view::render_graph_editor;
