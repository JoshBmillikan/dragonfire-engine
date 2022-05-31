use std::error::Error;
use std::time::Instant;

use fern::colors::{Color, ColoredLevelConfig};
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use engine::filesystem::DIRS;
use engine::log::{info, LevelFilter};
use engine::uom::si::f64::Time;
use engine::uom::si::time::second;
use rendering::{create_rendering_engine, RenderingEngine};

use crate::config::CONFIG;
use crate::game::Game;

mod config;
mod game;

pub fn start() -> ! {
    init_logging().expect("Failed to initialize logging");
    info!("Starting");
    let event_loop = EventLoop::new();
    let window = create_window(&event_loop).expect("Failed to create window");
    let rendering_engine = create_rendering_engine(&window, &CONFIG.graphics);
    let mut game = Game::new(rendering_engine);
    let mut time = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window.id() == window_id => *control_flow = ControlFlow::Exit,

            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                window_id,
            } if window.id() == window_id => game.rendering_engine.resize(size.width, size.height),

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
            Event::LoopDestroyed => info!("Shutting down"),
            _ => {}
        }
    });
}

fn create_window(events: &EventLoop<()>) -> Result<Window, Box<dyn Error>> {
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

fn init_logging() -> Result<(), fern::InitError> {
    let level = match CONFIG.log_level.as_str() {
        "trace" => Some(LevelFilter::Trace),
        "debug" => Some(LevelFilter::Debug),
        "info" => Some(LevelFilter::Info),
        "warn" => Some(LevelFilter::Warn),
        "error" => Some(LevelFilter::Error),
        "" => Some(LevelFilter::Info),
        _ => None,
    };
    let colors = ColoredLevelConfig::new()
        .info(Color::Green)
        .warn(Color::Yellow)
        .error(Color::Red)
        .debug(Color::White)
        .trace(Color::Black);

    let path = DIRS.project.data_local_dir().join("log.txt");
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(level.unwrap_or(LevelFilter::Info))
        .chain(std::io::stdout())
        .chain(fern::log_file(&path)?)
        .apply()?;
    if level.is_none() {
        info!("Unknown log level option \"{}\"", CONFIG.log_level);
    }
    Ok(())
}
