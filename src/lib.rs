/// Library facade — exposes only self-contained modules for benchmarks.
pub mod icon_mode;
pub mod icons;

#[path = "session"]
pub mod session {
    pub mod parser;
    pub mod transition;
    pub mod types;
}
