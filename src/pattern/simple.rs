use super::{Pattern, PatternInput, PatternMove};

pub struct Simple {
    out_stroke: bool,
}

impl Simple {
    pub fn new() -> Self {
        Self { out_stroke: true }
    }
}

impl Pattern for Simple {
    fn next_move(&mut self, input: &PatternInput) -> PatternMove {
        let new_move = if self.out_stroke {
            PatternMove::new(input.velocity, input.depth)
        } else {
            let in_stroke_depth = input.depth - input.motion_length;
            PatternMove::new(input.velocity, in_stroke_depth)
        };
        self.out_stroke = !self.out_stroke;

        new_move
    }
}
