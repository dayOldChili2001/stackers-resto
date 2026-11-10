//! Deterministic Tecmo Stackers rules, reimplemented from the disassembly.
//!
//! No I/O and no game assets: `Game::step` advances one 60 Hz frame from a pad state.
//! The rules and the addresses they come from are written up in `docs/gameplay.md`.

pub mod bgm;
pub mod camera;
pub mod cpu;
pub mod ending;
pub mod field;
pub mod front;
pub mod game;
pub mod garbage;
pub mod mascot;
pub mod over;
pub mod pause;
pub mod rng;
pub mod roulette;
pub mod set_match;
pub mod tables;
pub mod trial;
pub mod versus;
pub mod vs_cpu;

pub use camera::Camera;
pub use field::{Cell, Field, COLS, ROWS, TALL_ROWS, TILE};
pub use front::Front;
pub use game::{pad, ps, se, CutIn, Game, Hud, Rules};
pub use mascot::Mascot;
pub use over::{GameOver, Outcome};
pub use pause::Pause;
pub use trial::TimeTrial;
pub use versus::Versus;
