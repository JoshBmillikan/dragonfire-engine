use std::error::Error;

use fern::colors::{Color, ColoredLevelConfig};
use log::{info, LevelFilter};
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

use engine::filesystem::DIRS;
use rendering::create_rendering_engine;

use crate::config::CONFIG;
use crate::game::Game;

mod config;
mod game;

pub fn start() -> ! {
    init_logging().expect("Failed to initialize logging");
    info!("Starting");
    let event_loop = EventLoop::new();
    let window = create_window(&event_loop).expect("Failed to create window");
    let rendering_engine = create_rendering_engine(&window, &CONFIG.read().graphics);
    let mut game = Game::new(rendering_engine, window);
    info!("Initialization finished");

    event_loop.run(move |event, _, control_flow| game.main_loop(event, control_flow));
}

fn create_window<T>(events: &EventLoop<T>) -> Result<Window, Box<dyn Error>> {
    let settings = &CONFIG.read().graphics;
    Ok(WindowBuilder::new()
        .with_inner_size(LogicalSize {
            width: settings.resolution[0],
            height: settings.resolution[1],
        })
        .with_title(std::option_env!("APP_NAME").unwrap_or("dragonfire engine"))
        .build(events)?)
    // todo more window options
}

fn init_logging() -> Result<(), fern::InitError> {
    let cfg = CONFIG.read();
    let level = match cfg.log_level.as_str() {
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
        info!("Unknown log level option \"{}\"", cfg.log_level);
    }
    Ok(())
}
