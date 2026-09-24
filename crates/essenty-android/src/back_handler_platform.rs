#[path = "back_handler/platform.rs"]
mod platform;

pub use platform::{AndroidBackError, AndroidBackHandler, AndroidBackStrategy};
