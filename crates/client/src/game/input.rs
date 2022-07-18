use ahash::AHashMap;
use anyhow::Result;
use engine::filesystem::DIRS;
use log::info;
use multimap::{MultiMap, multimap};
use serde::{Deserialize, Serialize};
use winit::event::{AxisId, ButtonId, DeviceEvent, DeviceId, ElementState, VirtualKeyCode};

#[derive(Debug)]
pub struct InputManager {
    input_bindings: MultiMap<String, InputBinding>,
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
enum InputBinding {
    Axis {
        id: AxisId,
        scale: f64,
    },
    Button {
        id: ButtonId,
        state: ElementState,
    },
    Key {
        id: VirtualKeyCode,
        state: ElementState,
    }
}

impl InputManager {
    pub fn new() -> Result<Self> {
        let cfg = DIRS.project.config_dir();
        let bindings = if let Ok(file) = std::fs::read_to_string(cfg.join("keybindings.yaml")) {
            serde_yaml::from_str(file.as_str())?
        } else {
            default_bindings()
        };
        info!("Bindings:\n{}", serde_yaml::to_string(&bindings).unwrap());

        Ok(InputManager {
            input_bindings: bindings,
            input_events: Default::default(),
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


fn default_bindings() -> MultiMap<String, InputBinding> {
    multimap! {
        "forward".into() => InputBinding::Key {
            id: VirtualKeyCode::W,
            state: ElementState::Pressed
        }
    }
}
