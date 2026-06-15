use std::fs::File;
use std::io::Write;

use glow::HasContext;

use crate::bitmap_font::BitmapFont;
use crate::Config;

const GAMEPAD_BUTTONS: &[(&str, &str)] = &[
    ("Cross", "south"),
    ("Circle", "east"),
    ("Square", "west"),
    ("Triangle", "north"),
    ("L", "left-trigger"),
    ("R", "right-trigger"),
    ("Select", "select"),
    ("Start", "start"),
    ("DPad Up", "dpad-up"),
    ("DPad Down", "dpad-down"),
    ("DPad Left", "dpad-left"),
    ("DPad Right", "dpad-right"),
];

const KEYBOARD_KEYS: &[(&str, u32)] = &[
    ("A", 65), ("B", 66), ("C", 67), ("D", 68), ("E", 69),
    ("F", 70), ("G", 71), ("H", 72), ("I", 73), ("J", 74),
    ("K", 75), ("L", 76), ("M", 77), ("N", 78), ("O", 79),
    ("P", 80), ("Q", 81), ("R", 82), ("S", 83), ("T", 84),
    ("U", 85), ("V", 86), ("W", 87), ("X", 88), ("Y", 89), ("Z", 90),
    ("0", 48), ("1", 49), ("2", 50), ("3", 51), ("4", 52),
    ("5", 53), ("6", 54), ("7", 55), ("8", 56), ("9", 57),
    ("F1", 112), ("F2", 113), ("F3", 114), ("F4", 115),
    ("F5", 116), ("F6", 117), ("F7", 118), ("F8", 119),
    ("F9", 120), ("F10", 121), ("F11", 122), ("F12", 123),
    ("Up Arrow", 38), ("Down Arrow", 40), ("Left Arrow", 37), ("Right Arrow", 39),
    ("Shift", 16), ("Control", 17), ("Alt", 18),
    ("Enter", 13), ("Escape", 27), ("Tab", 9), ("Space", 32), ("Backspace", 8),
    ("Comma", 188), ("Period", 190), ("Slash", 191), ("Semicolon", 186),
    ("Minus", 189), ("Equals", 187), ("Backtick", 192),
    ("[", 219), ("]", 221), ("\\", 220), ("'", 222),
    ("Numpad 0", 96), ("Numpad 1", 97), ("Numpad 2", 98), ("Numpad 3", 99),
    ("Numpad 4", 100), ("Numpad 5", 101), ("Numpad 6", 102),
    ("Numpad 7", 103), ("Numpad 8", 104), ("Numpad 9", 105),
    ("Num *", 106), ("Num +", 107), ("Num -", 109), ("Num .", 110), ("Num /", 111),
    ("Page Up", 33), ("Page Down", 34), ("Home", 35), ("End", 36),
    ("Insert", 45), ("Delete", 46),
    ("Caps Lock", 20), ("Num Lock", 144), ("Scroll Lock", 145),
    ("Pause", 19),
];

const HEADER_INDEX: usize = 2;
const GAMEPAD_START: usize = 3;
const CONFIG_ITEM_COUNT: usize = 15;
const MAX_VISIBLE: usize = 26;

fn key_display_name(code: u32) -> String {
    if code == 0 {
        return "Not Set".to_string();
    }
    for (name, kc) in KEYBOARD_KEYS {
        if *kc == code {
            return name.to_string();
        }
    }
    format!("#{}", code)
}

fn letterbox_display(val: Option<&String>) -> &str {
    match val.map(|s| s.as_str()) {
        Some("on") => "On",
        Some("off") => "Off",
        Some("fullscreen") => "Fullscreen",
        _ => "On",
    }
}

fn next_letterbox(val: Option<&String>) -> String {
    match val.map(|s| s.as_str()) {
        Some("on") => "off",
        Some("off") => "fullscreen",
        _ => "on",
    }
    .to_string()
}

pub enum ConfigAction {
    BackToMenu,
}

pub struct ConfigMenu {
    pub swf_name: String,
    pub config: Config,
    base_path: String,
    pub selected: usize,
    scroll_offset: usize,
    pub selecting_key: Option<usize>,
    key_selected: usize,
    key_scroll_offset: usize,
}

impl ConfigMenu {
    pub fn new(swf_name: String, config: Config, base_path: &str) -> Self {
        ConfigMenu {
            swf_name,
            config,
            base_path: base_path.to_string(),
            selected: 0,
            scroll_offset: 0,
            selecting_key: None,
            key_selected: 0,
            key_scroll_offset: 0,
        }
    }

    pub fn save(&self) {
        let config_dir = format!("{}/config", self.base_path);
        let _ = std::fs::create_dir_all(&config_dir);
        let config_file = format!("{}/{}.ron", config_dir, self.swf_name);
        match ron::ser::to_string(&self.config) {
            Ok(ron_str) => {
                match File::create(&config_file) {
                    Ok(mut f) => {
                        let _ = f.write_all(ron_str.as_bytes());
                        println!("Saved config to {}", config_file);
                    }
                    Err(e) => println!("Failed to write config {}: {}", config_file, e),
                }
            }
            Err(e) => println!("Failed to serialize config: {}", e),
        }
    }

    pub fn handle_button(
        &mut self,
        is_down: bool,
        button: sdl2::controller::Button,
    ) -> Option<ConfigAction> {
        if !is_down {
            return None;
        }

        if let Some(btn_idx) = self.selecting_key {
            return self.handle_key_picker(button, btn_idx);
        }

        match button {
            sdl2::controller::Button::DPadUp => {
                if self.selected > 0 {
                    self.selected -= 1;
                    if self.selected == HEADER_INDEX {
                        self.selected = self.selected.saturating_sub(1);
                    }
                } else {
                    self.selected = CONFIG_ITEM_COUNT - 1;
                }
                self.clamp_scroll();
            }
            sdl2::controller::Button::DPadDown => {
                self.selected += 1;
                if self.selected == HEADER_INDEX {
                    self.selected += 1;
                }
                if self.selected >= CONFIG_ITEM_COUNT {
                    self.selected = 0;
                }
                self.clamp_scroll();
            }
            sdl2::controller::Button::DPadLeft => {
                self.selected = self.selected.saturating_sub(10);
                if self.selected == HEADER_INDEX {
                    self.selected = self.selected.saturating_sub(1);
                }
                self.clamp_scroll();
            }
            sdl2::controller::Button::DPadRight => {
                self.selected = (self.selected + 10).min(CONFIG_ITEM_COUNT - 1);
                if self.selected == HEADER_INDEX {
                    self.selected = GAMEPAD_START;
                }
                self.clamp_scroll();
            }
            sdl2::controller::Button::A => match self.selected {
                0 => {
                    self.save();
                    return Some(ConfigAction::BackToMenu);
                }
                1 => {
                    self.config.letterbox = Some(next_letterbox(self.config.letterbox.as_ref()));
                }
                _ if self.selected >= GAMEPAD_START => {
                    let btn_idx = self.selected - GAMEPAD_START;
                    if btn_idx < GAMEPAD_BUTTONS.len() {
                        self.selecting_key = Some(btn_idx);
                        self.key_selected = 0;
                        self.key_scroll_offset = 0;
                    }
                }
                _ => {}
            },
            sdl2::controller::Button::Y => {
                self.config.gamepad_config.clear();
                self.config.letterbox = Some("on".to_string());
            }
            _ => {}
        }
        None
    }

    fn handle_key_picker(
        &mut self,
        button: sdl2::controller::Button,
        btn_idx: usize,
    ) -> Option<ConfigAction> {
        match button {
            sdl2::controller::Button::DPadUp => {
                if self.key_selected > 0 {
                    self.key_selected -= 1;
                } else {
                    self.key_selected = KEYBOARD_KEYS.len();
                }
                self.clamp_key_scroll();
            }
            sdl2::controller::Button::DPadDown => {
                self.key_selected = (self.key_selected + 1) % (KEYBOARD_KEYS.len() + 1);
                self.clamp_key_scroll();
            }
            sdl2::controller::Button::A => {
                let (_, key_name) = GAMEPAD_BUTTONS[btn_idx];
                if self.key_selected < KEYBOARD_KEYS.len() {
                    let (_, kc) = KEYBOARD_KEYS[self.key_selected];
                    self.config.gamepad_config.insert(key_name.to_string(), kc);
                } else {
                    self.config.gamepad_config.remove(key_name);
                }
                self.selecting_key = None;
            }
            sdl2::controller::Button::B => {
                self.selecting_key = None;
            }
            _ => {}
        }
        None
    }

    pub fn handle_axis_motion(&mut self, axis: sdl2::controller::Axis, value: i32) {
        let deadzone = 16000;
        if self.selecting_key.is_some() {
            match axis {
                sdl2::controller::Axis::LeftY if value < -deadzone => {
                    if self.key_selected > 0 {
                        self.key_selected -= 1;
                    } else {
                        self.key_selected = KEYBOARD_KEYS.len();
                    }
                    self.clamp_key_scroll();
                }
                sdl2::controller::Axis::LeftY if value > deadzone => {
                    self.key_selected = (self.key_selected + 1) % (KEYBOARD_KEYS.len() + 1);
                    self.clamp_key_scroll();
                }
                _ => {}
            }
        } else {
            match axis {
                sdl2::controller::Axis::LeftY if value < -deadzone => {
                    if self.selected > 0 {
                        self.selected -= 1;
                        if self.selected == HEADER_INDEX {
                            self.selected = self.selected.saturating_sub(1);
                        }
                    } else {
                        self.selected = CONFIG_ITEM_COUNT - 1;
                    }
                    self.clamp_scroll();
                }
                sdl2::controller::Axis::LeftY if value > deadzone => {
                    self.selected += 1;
                    if self.selected == HEADER_INDEX {
                        self.selected += 1;
                    }
                    if self.selected >= CONFIG_ITEM_COUNT {
                        self.selected = 0;
                    }
                    self.clamp_scroll();
                }
                _ => {}
            }
        }
    }

    fn clamp_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + MAX_VISIBLE {
            self.scroll_offset = self.selected - MAX_VISIBLE + 1;
        }
    }

    fn clamp_key_scroll(&mut self) {
        if self.key_selected < self.key_scroll_offset {
            self.key_scroll_offset = self.key_selected;
        } else if self.key_selected >= self.key_scroll_offset + MAX_VISIBLE {
            self.key_scroll_offset = self.key_selected - MAX_VISIBLE + 1;
        }
    }

    pub fn render(&self, gl: &glow::Context, font: &BitmapFont, screen_w: f32, screen_h: f32) {
        unsafe {
            gl.clear_color(0.08, 0.08, 0.12, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);

            if let Some(btn_idx) = self.selecting_key {
                self.render_key_picker(gl, font, screen_w, screen_h, btn_idx);
            } else {
                self.render_main(gl, font, screen_w, screen_h);
            }
        }
    }

    fn render_main(&self, gl: &glow::Context, font: &BitmapFont, screen_w: f32, screen_h: f32) {
        let title = format!("Config: {}", self.swf_name);
        let (tw, _) = font.measure(&title);
        font.render_text(
            gl,
            &title,
            (screen_w - tw as f32) / 2.0,
            8.0,
            (0.7, 0.7, 0.9, 1.0),
            screen_w,
            screen_h,
        );

        let start_y = 34.0;
        let row_h = font.glyph_height() as f32 + 2.0;

        let end = (self.scroll_offset + MAX_VISIBLE).min(CONFIG_ITEM_COUNT);
        for i in self.scroll_offset..end {
            let y = start_y + (i - self.scroll_offset) as f32 * row_h;
            let is_selected = i == self.selected;

            let (label, color) = match i {
                0 => (
                    "Save and Return".to_string(),
                    if is_selected {
                        (1.0, 0.8, 0.2, 1.0)
                    } else {
                        (0.6, 0.9, 0.6, 1.0)
                    },
                ),
                1 => (
                    format!("Letterbox: {}", letterbox_display(self.config.letterbox.as_ref())),
                    if is_selected {
                        (1.0, 0.8, 0.2, 1.0)
                    } else {
                        (0.8, 0.8, 0.8, 1.0)
                    },
                ),
                2 => ("    Gamepad Buttons".to_string(), (0.5, 0.5, 0.6, 1.0)),
                _ => {
                    let btn_idx = i - GAMEPAD_START;
                    if btn_idx < GAMEPAD_BUTTONS.len() {
                        let (display_name, key_name) = GAMEPAD_BUTTONS[btn_idx];
                        let current = self.config.gamepad_config.get(key_name).copied().unwrap_or(0);
                        (
                            format!("{}: [{}]", display_name, key_display_name(current)),
                            if is_selected {
                                (1.0, 0.8, 0.2, 1.0)
                            } else {
                                (0.8, 0.8, 0.8, 1.0)
                            },
                        )
                    } else {
                        ("".to_string(), (0.8, 0.8, 0.8, 1.0))
                    }
                }
            };

            if i == HEADER_INDEX {
                font.render_text(gl, &label, 16.0, y, color, screen_w, screen_h);
            } else if is_selected {
                font.render_text(gl, ">", 16.0, y, (1.0, 0.8, 0.2, 1.0), screen_w, screen_h);
                font.render_text(gl, &label, 28.0, y, color, screen_w, screen_h);
            } else {
                font.render_text(gl, &label, 28.0, y, color, screen_w, screen_h);
            }
        }

        let bar_y = screen_h - 28.0;
        let hint = "Cross: Select  |  Triangle: Defaults";
        let (hw, _) = font.measure(hint);
        font.render_text(
            gl,
            hint,
            (screen_w - hw as f32) / 2.0,
            bar_y,
            (0.6, 0.6, 0.8, 1.0),
            screen_w,
            screen_h,
        );
    }

    fn render_key_picker(
        &self,
        gl: &glow::Context,
        font: &BitmapFont,
        screen_w: f32,
        screen_h: f32,
        btn_idx: usize,
    ) {
        let (display_name, _) = GAMEPAD_BUTTONS[btn_idx];
        let title = format!("Key for {}:", display_name);
        let (tw, _) = font.measure(&title);
        font.render_text(
            gl,
            &title,
            (screen_w - tw as f32) / 2.0,
            8.0,
            (0.7, 0.7, 0.9, 1.0),
            screen_w,
            screen_h,
        );

        let start_y = 34.0;
        let row_h = font.glyph_height() as f32 + 2.0;
        let total_items = KEYBOARD_KEYS.len() + 1;

        let end = (self.key_scroll_offset + MAX_VISIBLE).min(total_items);
        for i in self.key_scroll_offset..end {
            let y = start_y + (i - self.key_scroll_offset) as f32 * row_h;
            let is_selected = i == self.key_selected;

            let (label, color) = if i < KEYBOARD_KEYS.len() {
                let (name, _) = KEYBOARD_KEYS[i];
                (
                    name.to_string(),
                    if is_selected {
                        (1.0, 0.8, 0.2, 1.0)
                    } else {
                        (0.8, 0.8, 0.8, 1.0)
                    },
                )
            } else {
                (
                    "[Clear Binding]".to_string(),
                    if is_selected {
                        (1.0, 0.4, 0.4, 1.0)
                    } else {
                        (0.6, 0.6, 0.8, 1.0)
                    },
                )
            };

            if is_selected {
                font.render_text(gl, ">", 16.0, y, (1.0, 0.8, 0.2, 1.0), screen_w, screen_h);
            }
            font.render_text(gl, &label, 28.0, y, color, screen_w, screen_h);
        }

        let bar_y = screen_h - 28.0;
        let hint = "Cross: Assign  |  Circle: Cancel";
        let (hw, _) = font.measure(hint);
        font.render_text(
            gl,
            hint,
            (screen_w - hw as f32) / 2.0,
            bar_y,
            (0.6, 0.6, 0.8, 1.0),
            screen_w,
            screen_h,
        );
    }
}
