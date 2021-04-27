use std::collections::HashSet;
use winit::event::*;

#[derive(Clone, Debug)]
pub enum Drag {
    None,
    Start { x0: u32, y0: u32 },
    Dragging { x0: u32, y0: u32, x1: u32, y1: u32 },
    End { x0: u32, y0: u32, x1: u32, y1: u32 },
}

#[derive(Clone, Debug)]
pub struct InputState {
    pub key_pressed: HashSet<VirtualKeyCode>,
    pub mouse_pressed: HashSet<MouseButton>,
    pub key_trigger: HashSet<VirtualKeyCode>,
    pub mouse_trigger: HashSet<MouseButton>,
    pub key_release: HashSet<VirtualKeyCode>,
    pub mouse_release: HashSet<MouseButton>,
    pub drag: Drag,
    pub last_scroll: f32,
    pub cursor_pos: (u32, u32),
    pub cursor_offset: (i32, i32),
}

impl InputState {
    pub fn new() -> Self {
        InputState {
            key_pressed: HashSet::new(),
            mouse_pressed: HashSet::new(),
            key_trigger: HashSet::new(),
            mouse_trigger: HashSet::new(),
            key_release: HashSet::new(),
            mouse_release: HashSet::new(),
            last_scroll: 0.0,
            cursor_pos: (0, 0),
            cursor_offset: (0, 0),
            drag: Drag::None,
        }
    }

    pub fn update(&mut self, evt: &WindowEvent) {
        match evt {
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: Some(vkc),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                self.key_pressed.insert(vkc.clone());
                self.key_trigger.insert(vkc.clone());
            }
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: Some(vkc),
                        state: ElementState::Released,
                        ..
                    },
                ..
            } => {
                self.key_pressed.remove(vkc);
                self.key_release.insert(vkc.clone());
            }

            WindowEvent::MouseWheel {
                delta: MouseScrollDelta::LineDelta(_, dy),
                ..
            } => {
                self.last_scroll = *dy;
            }

            WindowEvent::CursorMoved { position, .. } => {
                let (old_x, old_y) = self.cursor_pos;

                self.cursor_offset = (
                    position.x as i32 - old_x as i32,
                    position.y as i32 - old_y as i32,
                );
                self.cursor_pos = (position.x as u32, position.y as u32);
                match self.drag {
                    Drag::Start { x0, y0 } | Drag::Dragging { x0, y0, .. } => {
                        self.drag = Drag::Dragging {
                            x0,
                            y0,
                            x1: self.cursor_pos.0 as u32,
                            y1: self.cursor_pos.1 as u32,
                        };
                    }
                    _ => {}
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if let &ElementState::Pressed = state {
                    self.mouse_pressed.insert(*button);
                    self.mouse_trigger.insert(*button);

                    if let MouseButton::Left = button {
                        self.drag = Drag::Start {
                            x0: self.cursor_pos.0 as u32,
                            y0: self.cursor_pos.1 as u32,
                        }
                    };
                } else {
                    self.mouse_pressed.remove(button);
                    self.mouse_release.insert(*button);
                    if let MouseButton::Left = button {
                        match self.drag {
                            Drag::Dragging { x0, y0, .. } => {
                                self.drag = Drag::End {
                                    x0,
                                    y0,
                                    x1: self.cursor_pos.0 as u32,
                                    y1: self.cursor_pos.1 as u32,
                                };
                            }
                            _ => {
                                self.drag = Drag::None;
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }

    pub fn clear(&mut self) {
        self.key_trigger.clear();
        self.mouse_trigger.clear();
        self.mouse_release.clear();
        self.key_release.clear();
        if let Drag::End { .. } = self.drag {
            self.drag = Drag::None;
        }
        self.last_scroll = 0.0;
        self.cursor_offset = (0, 0);
    }
}
