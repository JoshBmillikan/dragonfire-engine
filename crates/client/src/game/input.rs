use std::fs::File;
use std::io::Read;
use ahash::AHashMap;
use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use winit::event::{AxisId, ButtonId, DeviceEvent, DeviceId, ElementState, VirtualKeyCode};
use engine::filesystem::DIRS;

#[derive(Debug)]
pub struct InputManager {
    input_bindings: AHashMap<String, InputAction>,
    input_events: AHashMap<InputAction, InputValue>,
}

#[derive(Debug, Serialize, Deserialize, Hash, Copy, Clone, Eq, PartialEq)]
pub enum InputAction {
    Axis(AxisId),
    Button(ButtonId),
    Key(VirtualKeyCode),
}

#[derive(Debug)]
enum InputValue {
    Axis(f64),
    Button(ElementState),
}

impl InputManager {
    pub fn new() -> Result<Self> {
        let cfg = DIRS.project.config_dir();
        let bindings = if let Ok(file) = std::fs::read_to_string(cfg.join("keybindings.toml")) {
            toml::from_str(file.as_str())?
        } else {
            default_bindings()
        };
        
        Ok(InputManager {
            input_bindings: bindings,
            input_events: Default::default()
        })
    }

    pub(super) fn handle_input(&mut self, event: DeviceEvent, device_id: DeviceId) {
        match event {
            DeviceEvent::Added => {
                info!("Device {device_id:?} connected");
            }
            DeviceEvent::Removed => {
                info!("Device {device_id:?} disconnected");
            }
            DeviceEvent::Motion { axis, value } => {
                self.input_events
                    .insert(InputAction::Axis(axis), InputValue::Axis(value));
            }
            DeviceEvent::Button { button, state } => {
                info!("Pressed {button:?}");
                self.input_events
                    .insert(InputAction::Button(button), InputValue::Button(state));
            }
            DeviceEvent::Key(input) => {
                if let Some(code) = input.virtual_keycode {
                    self.input_events
                        .insert(InputAction::Key(code), InputValue::Button(input.state));
                }
            }
            _ => {}
        }
    }

    pub(super) fn clear_events(&mut self) {
        self.input_events.clear();
    }
}

/// Macro to conveniently write the default input mappings
/// Based on the map lit crate, but modified for our specific use case
macro_rules! input_map {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(input_map!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { input_map!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = input_map!(@count $($key),*);
            let mut _map = AHashMap::with_capacity(_cap);
            $(
                let _ = _map.insert($key.into(), $value);
            )*
            _map
        }
    };
}

fn default_bindings() -> AHashMap<String, InputAction> {
        input_map! {
            "forward" => InputAction::Key(VirtualKeyCode::W),
            "left" => InputAction::Key(VirtualKeyCode::A),
            "right" => InputAction::Key(VirtualKeyCode::D),
            "back" => InputAction::Key(VirtualKeyCode::S),
            "jump" => InputAction::Key(VirtualKeyCode::Space),
        }
}
