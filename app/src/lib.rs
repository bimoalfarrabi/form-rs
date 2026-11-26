//! Form native editor — library surface used by the binary and headless tools.

pub mod app;
pub mod logo;
pub mod project;
pub mod settings;

pub use app::{install_fonts, FormApp};
