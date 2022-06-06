use std::path::PathBuf;
use std::sync::Arc;
use nalgebra::{Matrix4, Perspective3, Transform3};
use uom::si::f64::Time;
use rendering::{Material, Mesh, RenderingEngine};

pub struct Game <R: RenderingEngine> {
    pub rendering_engine: Box<R>,
    mesh: Arc<Mesh>,
    material: Arc<Material>
}

impl<R: RenderingEngine> Game<R> {
    pub fn new(mut rendering_engine: Box<R>) -> Game<R> {
        let path = PathBuf::from("./model.obj");
        let mesh = rendering_engine.load_model(&path).unwrap();
        let material = rendering_engine.load_material().unwrap();
        Game {
            rendering_engine,
            mesh,
            material
        }
    }

    pub fn tick(&mut self, delta: Time) {
        let perspective = Perspective3::new(1920./1080., 45., 0.1, 100.);
        let view = Matrix4::identity();
        self.rendering_engine.begin_rendering(&view, &perspective);

        self.rendering_engine.render(&self.mesh, &self.material, Matrix4::default());

        self.rendering_engine.end_rendering();
    }
}