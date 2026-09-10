//! The user interface: a reusable toolbar and status bar, the node map with its
//! editor sidebar, and the layout that arranges them.

pub mod editor;
pub mod icon;
pub mod layout;
pub mod node_map;
pub mod status_bar;
pub mod theme;
pub mod toolbar;

pub use editor::{Editor, EditorAction, Lsp};
pub use node_map::NodeMap;
