use std::path::PathBuf;
use std::sync::Arc;

use nalgebra::{Isometry3, Matrix4};
use uom::si::f64::Time;

use engine::ecs::{IntoIter, View, World};
use rendering::{Camera, Material, Mesh, RenderingEngine};
use crate::CONFIG;

pub struct Game<R: RenderingEngine> {
    world: World,
    camera: Camera,
    pub rendering_engine: Box<R>,
}

impl<R: RenderingEngine> Game<R> {
    pub fn new(mut rendering_engine: Box<R>) -> Game<R> {
        let camera = Camera::new(&CONFIG.read().graphics);
        let path = PathBuf::from("./model.obj");
        let mesh = rendering_engine.load_model(&path).unwrap();
        let material = rendering_engine.load_material().unwrap();
        let mut world = World::new();
        let _entity = world.add_entity((mesh, material, Isometry3::<f32>::default()));
        Game {
            world,
            camera,
            rendering_engine,
        }
    }

    pub fn tick(&mut self, delta: Time) {
        self.rendering_engine.begin_rendering(&self.camera.view, &self.camera.projection);

        self.world
            .run(
                |mesh: View<Arc<Mesh>>,
                 material: View<Arc<Material>>,
                 transform: View<Isometry3<f32>>| {
                    for (mesh, material, transform) in (&mesh, &material, &transform).iter() {
                        self.rendering_engine
                            .render(mesh, material, transform.to_homogeneous());
                    }
                },
            )
            .expect("Rendering failed");

        self.rendering_engine.end_rendering();
    }
}
