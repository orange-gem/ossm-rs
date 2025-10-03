mod deeper;
mod halfhalf;
mod simple;
mod teasingpounding;
mod stopngo;

use deeper::Deeper;
use defmt::error;
use halfhalf::HalfHalf;
use simple::Simple;
use teasingpounding::TeasingPounding;
use stopngo::StopNGo;

use crate::utils::saturate_range;

#[enum_dispatch::enum_dispatch]
pub enum AvailablePatterns {
    Simple,
    TeasingPounding,
    HalfHalf,
    Deeper,
    StopNGo,
}

pub const MIN_SENSATION: f64 = -100.0;
pub const MAX_SENSATION: f64 = 100.0;

pub struct PatternInput {
    // The maximum depth
    pub depth: f64,
    // The maximum length of the motion
    pub motion_length: f64,
    pub velocity: f64,
    // Sensation from -100 to 100
    pub sensation: f64,
}

#[derive(Default)]
pub struct PatternMove {
    // The maximum velocity for the move
    pub velocity: f64,
    // The position for the move
    pub position: f64,
    // How much to delay after this move
    pub delay_ms: u64,
}

impl PatternMove {
    /// Create a new pattern move
    pub fn new(velocity: f64, position: f64) -> Self {
        Self {
            velocity,
            position,
            delay_ms: 0,
        }
    }

    /// Create a new pattern move that would delay by this much after a pattern is done
    pub fn new_with_delay(velocity: f64, position: f64, delay_ms: u64) -> Self {
        Self {
            velocity,
            position,
            delay_ms,
        }
    }
}

#[enum_dispatch::enum_dispatch(AvailablePatterns)]
pub trait Pattern {
    /// Get the next position for the pattern with the given input
    /// Will be called when the move to the previously given position is complete
    fn next_move(&mut self, input: &PatternInput) -> PatternMove;
}

pub struct PatternExecutor {
    current_pattern: AvailablePatterns,
}

impl PatternExecutor {
    pub fn new() -> Self {
        Self {
            current_pattern: Simple::new().into(),
        }
    }

    pub fn set_pattern(&mut self, pattern_index: u32) {
        let new_pattern: AvailablePatterns = match pattern_index {
            0 => Simple::new().into(),
            1 => TeasingPounding::new().into(),
            3 => HalfHalf::new().into(),
            4 => Deeper::new().into(),
            5 => StopNGo::new().into(),
            _ => {
                error!(
                    "Unknown pattern index {}. Switching to the simple pattern",
                    pattern_index
                );
                Simple::new().into()
            }
        };

        self.current_pattern = new_pattern;
    }
}

impl Pattern for PatternExecutor {
    fn next_move(&mut self, input: &PatternInput) -> PatternMove {
        let mut next_move = self.current_pattern.next_move(input);
        // Verify that all constraints have been met and saturate if not
        next_move.position = saturate_range(next_move.position, 0.0, input.depth);
        next_move.velocity = saturate_range(next_move.velocity, 0.0, input.velocity);

        next_move
    }
}
