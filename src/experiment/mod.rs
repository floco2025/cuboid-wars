mod executor;
mod play;
mod playback;
mod player;
mod report;
mod script;
mod session;

pub use play::play_file;
pub use script::run_file;

#[cfg(test)]
#[path = "tests/encounter.rs"]
mod encounter_tests;

#[cfg(test)]
#[path = "tests/fixtures.rs"]
mod fixtures;
#[cfg(test)]
#[path = "tests/movement.rs"]
mod movement_tests;

#[cfg(test)]
#[path = "tests/course.rs"]
mod course_tests;

#[cfg(test)]
#[path = "tests/playback.rs"]
mod playback_tests;
