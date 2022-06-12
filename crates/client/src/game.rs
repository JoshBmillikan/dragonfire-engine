use std::path::PathBuf;
use std::sync::Arc;

use nalgebra::{Isometry3, Matrix4, Perspective3, Vector3};
use uom::si::f64::Time;
use engine::ecs::{IntoIter, View, World};

use rendering::{Material, Mesh, RenderingEngine};

pub struct Game<R: RenderingEngine> {
    world: World,
    pub rendering_engine: Box<R>,
}

impl<R: RenderingEngine> Game<R> {
    pub fn new(mut rendering_engine: Box<R>) -> Game<R> {
        let path = PathBuf::from("./model.obj");
        let mesh = rendering_engine.load_model(&path).unwrap();
        let material = rendering_engine.load_material().unwrap();
        let mut world = World::new();
        let _entity = world.add_entity((mesh, material, Matrix4::<f32>::default()));
        Game {
            world,
            rendering_engine,
        }
    }

    pub fn tick(&mut self, delta: Time) {
        let perspective = Perspective3::new(1920. / 1080., 45., 0.1, 100.);
        let mut view = Matrix4::identity();
        let vec = Vector3::new(0.,0., 2.);
        view.append_translation_mut(&vec);
        self.rendering_engine.begin_rendering(&view, &perspective);

        self.world.run(|mesh: View<Arc<Mesh>>, material: View<Arc<Material>>, transform: View<Isometry3<f32>>| {
            for (mesh, material, transform) in (&mesh, &material, &transform).iter() {
                self.rendering_engine.render(mesh, material, transform.to_homogeneous());
            }
        }).expect("Rendering failed");

        self.rendering_engine.end_rendering();
    }

}
