//! Scroll state of the Output view (#1288, §02.13): it follows the tail
//! until the user scrolls up, and `End` follows again.

/// Where the Output view sits. `top` is the first output row shown when not
/// following; `tail` is the last drawn `top` of the tail, kept so a scroll
/// up starts from what is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputScroll {
    pub follow: bool,
    top: u16,
    tail: u16,
}

impl Default for OutputScroll {
    fn default() -> Self {
        Self {
            follow: true,
            top: 0,
            tail: 0,
        }
    }
}

impl OutputScroll {
    /// One row up. Any upward scroll stops following.
    pub fn up(&mut self) {
        if self.follow {
            self.follow = false;
            self.top = self.tail;
        }
        self.top = self.top.saturating_sub(1);
    }

    /// One row down. Reaching the tail follows again.
    pub fn down(&mut self) {
        if self.follow {
            return;
        }
        self.top = self.top.saturating_add(1);
        if self.top >= self.tail {
            self.follow = true;
        }
    }

    /// `End`: follow the tail again.
    pub fn end(&mut self) {
        self.follow = true;
    }

    /// The first row to draw, given the tail `top` (rows past the screen).
    /// A stopped view stays on its row while new output lands below.
    pub fn offset(&mut self, tail: u16) -> u16 {
        self.tail = tail;
        if self.follow {
            return tail;
        }
        self.top = self.top.min(tail);
        self.top
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_stops_following_and_new_output_does_not_move_the_view() {
        let mut s = OutputScroll::default();
        assert_eq!(s.offset(30), 30);
        s.up();
        assert!(!s.follow);
        assert_eq!(s.offset(30), 29);
        assert_eq!(s.offset(50), 29, "new output lands below, the view stays");
        s.end();
        assert!(s.follow);
        assert_eq!(s.offset(50), 50);
    }

    #[test]
    fn down_to_the_tail_follows_again() {
        let mut s = OutputScroll::default();
        s.offset(5);
        s.up();
        s.down();
        assert!(s.follow);
    }
}
