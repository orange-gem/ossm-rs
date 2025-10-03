use crate::utils::scale;

use super::{Pattern, PatternInput, PatternMove, MAX_SENSATION};

pub struct HalfHalf {
    out_stroke: bool,
    half: bool,
}

impl HalfHalf {
    pub fn new() -> Self {
        Self {
            out_stroke: true,
            half: false,
        }
    }
}

impl Pattern for HalfHalf {
    fn next_move(&mut self, input: &PatternInput) -> PatternMove {
        let max_scaling_factor = 5.0;
        let cut_velocity = input.velocity / max_scaling_factor;

        let mut in_stroke_velocity = cut_velocity;
        let mut out_stroke_velocity = cut_velocity;

        let sensation_factor = scale(
            input.sensation.abs(),
            0.0,
            MAX_SENSATION,
            1.0,
            max_scaling_factor,
        );

        // Faster in move
        if input.sensation > 0.0 {
            in_stroke_velocity = cut_velocity * sensation_factor;
        // Faster out move
        } else if input.sensation < 0.0 {
            out_stroke_velocity = cut_velocity * sensation_factor;
        }

        let new_move = if self.out_stroke {
            let out_stroke_depth = if self.half {
                input.depth - input.motion_length / 2.0
            } else {
                input.depth
            };
            self.half = !self.half;
            PatternMove::new(out_stroke_velocity, out_stroke_depth)
        } else {
            let in_stroke_depth = input.depth - input.motion_length;
            PatternMove::new(in_stroke_velocity, in_stroke_depth)
        };
        self.out_stroke = !self.out_stroke;

        new_move
    }
}
