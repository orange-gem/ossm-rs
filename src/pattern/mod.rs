mod deeper;
mod halfhalf;
mod simple;
mod stopngo;
mod teasingpounding;

use core::fmt::Write;

use deeper::Deeper;
use defmt::error;
use halfhalf::HalfHalf;
use heapless::String;
use simple::Simple;
use stopngo::StopNGo;
use teasingpounding::TeasingPounding;

use crate::utils::saturate_range;

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
    fn get_name(&self) -> &'static str;

    /// Reset the pattern to its initial state
    fn reset(&mut self);

    /// Get the next position for the pattern with the given input
    /// Will be called when the move to the previously given position is complete
    fn next_move(&mut self, input: &PatternInput) -> PatternMove;
}

pub struct PatternExecutor {
    patterns: [Option<AvailablePatterns>; NUM_PATTERNS],
    current_pattern: usize,
}

const NUM_PATTERNS: usize = 7;

#[enum_dispatch::enum_dispatch]
pub enum AvailablePatterns {
    Simple,
    TeasingPounding,
    HalfHalf,
    Deeper,
    StopNGo,
}

impl PatternExecutor {
    pub fn new() -> Self {
        let patterns = [
            Some(Simple::new().into()),
            Some(TeasingPounding::new().into()),
            None,
            Some(HalfHalf::new().into()),
            Some(Deeper::new().into()),
            Some(StopNGo::new().into()),
            None,
        ];

        Self {
            patterns,
            current_pattern: 0,
        }
    }

    pub fn set_pattern(&mut self, pattern_index: u32) {
        let mut selected_pattern = pattern_index as usize;

        if let Some(new_pattern) = self.patterns.get(selected_pattern) {
            if new_pattern.is_none() {
                error!(
                    "Pattern at index {} not implemented. Switching to the simple pattern",
                    pattern_index
                );
                selected_pattern = 0;
            }
        } else {
            error!(
                "Unknown pattern index {}. Switching to the simple pattern",
                pattern_index
            );
            selected_pattern = 0;
        };

        self.current_pattern = selected_pattern;
    }

    /// Returns all patterns as json
    pub fn get_all_patterns_json(&mut self) -> String<256> {
        let mut output: String<256> = String::new();
        output.write_char('[').ok();
        for (i, pattern) in self.patterns.iter().enumerate() {
            if let Some(pattern) = pattern {
                let name = pattern.get_name();
                if write!(output, r#"{{"name":"{}","idx":{}}},"#, name, i).is_err() {
                    error!("Overflow. Returning unfinished string");
                    break;
                }
            }
        }
        // Remove the last comma
        output.pop();

        if output.write_char(']').is_err() {
            error!("Overflow. Returning unfinished string");
        }

        output
    }
}

impl Pattern for PatternExecutor {
    fn get_name(&self) -> &'static str {
        "Pattern Executor"
    }

    fn reset(&mut self) {
        let pattern = self.patterns[self.current_pattern]
            .as_mut()
            .expect("Checked in set_pattern");

        pattern.reset();
    }

    fn next_move(&mut self, input: &PatternInput) -> PatternMove {
        let pattern = self.patterns[self.current_pattern]
            .as_mut()
            .expect("Checked in set_pattern");

        let mut next_move = pattern.next_move(input);
        // Verify that all constraints have been met and saturate if not
        next_move.position = saturate_range(next_move.position, 0.0, input.depth);
        next_move.velocity = saturate_range(next_move.velocity, 0.0, input.velocity);

        next_move
    }
}
