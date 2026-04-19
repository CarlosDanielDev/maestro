/// Library facade — exposes only self-contained modules for benchmarks.
pub mod icon_mode;
pub mod icons;
pub mod turboquant;

#[path = "util"]
pub mod util {
    pub mod formatting;
    pub use formatting::*;
}

#[path = "session"]
pub mod session {
    pub mod parser;
    pub mod transition;
    pub mod types;
}
