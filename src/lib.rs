/// Library facade — exposes only self-contained modules for benchmarks.
#[path = "session"]
pub mod session {
    pub mod parser;
    pub mod transition;
    pub mod types;
}
