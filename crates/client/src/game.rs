use engine::uom::si::f64::Time;
use rendering::RenderingEngine;

pub struct Game <R: RenderingEngine> {
    pub rendering_engine: Box<R>,
}

impl<R: RenderingEngine> Game<R> {
    pub fn new(rendering_engine: Box<R>) -> Game<R> {
        Game {
            rendering_engine
        }
    }

    pub fn tick(&mut self, delta: Time) {

    }
}