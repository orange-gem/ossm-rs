use defmt::info;

use crate::{
    pattern::{MAX_SENSATION, MIN_SENSATION},
    utils::scale,
};

use super::{Pattern, PatternInput, PatternMove};

const MIN_STEPS: f64 = 2.0;
const MAX_STEPS: f64 = 22.0;

pub struct Deeper {
    out_stroke: bool,
    num_steps: usize,
    current_step: usize,
    previous_sensation: f64,
}

impl Deeper {
    pub fn new() -> Self {
        Self {
            out_stroke: true,
            num_steps: scale(0.0, MIN_SENSATION, MAX_SENSATION, MIN_STEPS, MAX_STEPS) as usize,
            current_step: 1,
            previous_sensation: 0.0,
        }
    }
}

impl Pattern for Deeper {
    fn next_move(&mut self, input: &PatternInput) -> PatternMove {
        if input.sensation != self.previous_sensation {
            self.num_steps = scale(
                input.sensation,
                MIN_SENSATION,
                MAX_SENSATION,
                MIN_STEPS,
                MAX_STEPS,
            ) as usize;
            info!("Using {} steps", self.num_steps);
            // Reset every time sensation changes
            self.current_step = 1;
            self.previous_sensation = input.sensation;
        }
        let in_stroke_depth = input.depth - input.motion_length;

        let new_move = if self.out_stroke {
            let increment = input.motion_length / self.num_steps as f64;
            if self.current_step > self.num_steps {
                self.current_step = 1;
            }
            let out_stroke_depth = in_stroke_depth + increment * self.current_step as f64;
            self.current_step += 1;
            PatternMove::new(input.velocity, out_stroke_depth)
        } else {
            PatternMove::new(input.velocity, in_stroke_depth)
        };
        self.out_stroke = !self.out_stroke;

        new_move
    }
}
