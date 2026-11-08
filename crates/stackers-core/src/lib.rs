//! Deterministic Tecmo Stackers rules, reimplemented from the disassembly.
//!
//! No I/O and no game assets: `Game::step` advances one 60 Hz frame from a pad state.
//! The rules and the addresses they come from are written up in `docs/gameplay.md`.

pub mod field;
pub mod game;
pub mod over;
pub mod rng;
pub mod tables;

pub use field::{Cell, Field, COLS, ROWS, TILE};
pub use game::{pad, ps, se, Game, Hud};
pub use over::{GameOver, Outcome};
