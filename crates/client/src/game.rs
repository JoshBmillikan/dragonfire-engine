use std::ops::Add;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use log::info;
use nalgebra::{Isometry3, Point3, Quaternion, Unit, UnitQuaternion, Vector3};
use uom::si::f64::Time;
use uom::si::time::second;
use winit::event::{Event, WindowEvent};
use winit::event_loop::ControlFlow;
use winit::window::Window;

use engine::ecs::{IntoIter, UniqueView, View, ViewMut, World};
use rendering::{Camera, Material, Mesh, RenderingEngine};

use crate::CONFIG;

pub struct Game<R: RenderingEngine> {
    world: World,
    camera: Camera,
    rendering_engine: Box<R>,
    time: Instant,
    window: Window,
}

impl<R: RenderingEngine> Game<R> {
    pub fn new(mut rendering_engine: Box<R>, window: Window) -> Game<R> {
        let mut camera = Camera::new(&CONFIG.read().graphics);
        let path = PathBuf::from("./model.obj");
        let mesh = rendering_engine.load_model(&path).unwrap();
        let material = rendering_engine.load_material().unwrap();
        let mut world = World::new();
        let mut iso = Isometry3::<f32>::default();
        iso.translation.x += 2.;
        iso.translation.z += -6.;
        let mut iso2 = iso.clone();
        iso2.translation.x -= 4.;
        let eye = Point3::new(0.0, 0.0, 0.0);
        let up = Vector3::new(0., 1., 0.);
        let mut target = iso.translation.vector;
        target.x = 0.;
        let target = Point3::from(target);
        camera.view = Isometry3::look_at_rh(&eye, &target, &up);
        let _entity = world.add_entity((mesh.clone(), material.clone(), iso));
        let _ = world.add_entity((mesh, material, iso2));
        Game {
            world,
            camera,
            rendering_engine,
            time: Instant::now(),
            window,
        }
    }

    pub fn main_loop(&mut self, event: Event<()>, control_flow: &mut ControlFlow) {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if self.window.id() == window_id => *control_flow = ControlFlow::Exit,

            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                window_id,
            } if self.window.id() == window_id => self.rendering_engine.resize(size.width, size.height),

            Event::DeviceEvent { .. } => {}
            Event::UserEvent(_) => {}
            Event::Suspended => {}
            Event::Resumed => {}

            Event::MainEventsCleared => {
                let now = Instant::now();
                let delta = Time::new::<second>((now - self.time).as_secs_f64());
                self.tick(delta);
                self.time = now;
            }

            Event::LoopDestroyed => {
                info!("Shutting down");
                self.rendering_engine.wait();
                self.world.clear();
            }
            _ => {}
        }
    }

    fn tick(&mut self, delta: Time) {
        self.world.add_unique(delta).unwrap();
        self.world.run(rotate).unwrap();

        self.rendering_engine.begin_rendering(&self.camera.view.to_homogeneous(), &self.camera.projection);

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
        self.world.remove_unique::<Time>().unwrap();
    }
}

fn rotate(mut iso: ViewMut<Isometry3<f32>>, time: UniqueView<Time>) {
    for mut transform in (&mut iso).iter() {
        let (r, p, y) = transform.rotation.euler_angles();
        let q =  UnitQuaternion::from_euler_angles(r, p + 1.,y);
        let r = transform.rotation;
        transform.rotation = r.slerp(&q, time.value as f32 / 60.);
    }
}