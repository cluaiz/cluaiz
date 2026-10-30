//! Generation output port. Presentation adapters share the same model/session pipeline.

pub trait Output: Send {
    fn content(&mut self, text: &str);
    fn reasoning(&mut self, text: &str);
    fn tick(&mut self);
    fn finish(&mut self);
}

impl Output for super::print::StreamPrinter {
    fn content(&mut self, text: &str) {
        Self::content(self, text);
    }
    fn reasoning(&mut self, text: &str) {
        Self::reasoning(self, text);
    }
    fn tick(&mut self) {
        Self::tick(self);
    }
    fn finish(&mut self) {
        Self::finish(self);
    }
}
