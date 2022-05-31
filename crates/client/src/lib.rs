use std::error::Error;
use std::time::Instant;
use winit::dpi::LogicalSize;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};
use engine::uom::si::f64::Time;
use engine::uom::si::time::second;
use rendering::create_rendering_engine;
use crate::config::CONFIG;
use crate::game::Game;

mod config;
mod game;

pub fn start() -> ! {
    let event_loop = EventLoop::new();
    let window = create_window(&event_loop).expect("Failed to create window");
    let rendering_engine = create_rendering_engine(&window, &CONFIG.graphics);
    let mut game = Game::new(rendering_engine);
    let mut time = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::NewEvents(_) => {}
            Event::WindowEvent { .. } => {}
            Event::DeviceEvent { .. } => {}
            Event::UserEvent(_) => {}
            Event::Suspended => {}
            Event::Resumed => {}
            Event::MainEventsCleared => {
                let now = Instant::now();
                let delta = Time::new::<second>((now - time).as_secs_f64());
                game.tick(delta);
                time = now;
            }
            _ => {}
        }
    });
}

fn create_window(
    events: &EventLoop<()>,
) -> Result<Window, Box<dyn Error>> {
    let settings = &CONFIG.graphics;
    Ok(WindowBuilder::new()
        .with_inner_size(LogicalSize {
            width: settings.resolution[0],
            height: settings.resolution[1],
        })
        .with_title(&settings.title)
        .build(events)?)
    // todo more window options
}