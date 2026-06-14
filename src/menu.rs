use std::path::Path;

use glow::HasContext;

use crate::bitmap_font::BitmapFont;

const SWF_DIR: &str = "swf";
const MAX_VISIBLE: usize = 26;
const STUB_MSG: &str = "Config editing not yet implemented";

pub enum MenuAction {
    Launch(String),
    Exit,
}

pub struct MenuState {
    pub files: Vec<String>,
    pub selected: usize,
    pub scroll_offset: usize,
    stub_timer: Option<u128>,
}

impl MenuState {
    pub fn new(base_path: &str) -> Self {
        let swf_dir = Path::new(base_path).join(SWF_DIR);
        let files = Self::list_swf_files(&swf_dir);
        MenuState {
            files,
            selected: 0,
            scroll_offset: 0,
            stub_timer: None,
        }
    }

    fn list_swf_files(dir: &Path) -> Vec<String> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "swf") {
                    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                        files.push(name.to_string());
                    }
                }
            }
        }
        files.sort();
        files
    }

    pub fn refresh(&mut self, base_path: &str) {
        let swf_dir = Path::new(base_path).join(SWF_DIR);
        self.files = Self::list_swf_files(&swf_dir);
        self.selected = 0;
        self.scroll_offset = 0;
        self.stub_timer = None;
    }

    pub fn handle_button(&mut self, is_down: bool, button: sdl2::controller::Button) -> Option<MenuAction> {
        if !is_down {
            return None;
        }
        if self.stub_timer.is_some() {
            self.stub_timer = None;
            return None;
        }
        match button {
            sdl2::controller::Button::DPadUp => {
                if self.selected > 0 {
                    self.selected -= 1;
                } else {
                    self.selected = self.files.len().saturating_sub(1);
                }
                self.clamp_scroll();
            }
            sdl2::controller::Button::DPadDown => {
                if self.files.is_empty() {
                    return None;
                }
                self.selected = (self.selected + 1) % self.files.len();
                self.clamp_scroll();
            }
            sdl2::controller::Button::DPadLeft => {
                if self.files.is_empty() {
                    return None;
                }
                self.selected = self.selected.saturating_sub(10);
                self.clamp_scroll();
            }
            sdl2::controller::Button::DPadRight => {
                if self.files.is_empty() {
                    return None;
                }
                self.selected = (self.selected + 10).min(self.files.len() - 1);
                self.clamp_scroll();
            }
            sdl2::controller::Button::A => {
                if !self.files.is_empty() {
                    return Some(MenuAction::Launch(self.files[self.selected].clone()));
                }
            }
            sdl2::controller::Button::B => {
                return Some(MenuAction::Exit);
            }
            sdl2::controller::Button::Y => {
                if !self.files.is_empty() {
                    self.stub_timer = Some(0);
                }
            }
            _ => {}
        }
        None
    }

    pub fn handle_axis_motion(&mut self, axis: sdl2::controller::Axis, value: i32) {
        let deadzone = 16000;
        match axis {
            sdl2::controller::Axis::LeftY if value < -deadzone && !self.files.is_empty() => {
                if self.selected > 0 {
                    self.selected -= 1;
                } else {
                    self.selected = self.files.len() - 1;
                }
                self.clamp_scroll();
            }
            sdl2::controller::Axis::LeftY if value > deadzone && !self.files.is_empty() => {
                self.selected = (self.selected + 1) % self.files.len();
                self.clamp_scroll();
            }
            _ => {}
        }
    }

    fn clamp_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + MAX_VISIBLE {
            self.scroll_offset = self.selected - MAX_VISIBLE + 1;
        }
    }

    pub fn update_stub_timer(&mut self, dt_ms: u128) {
        if let Some(t) = self.stub_timer.as_mut() {
            *t += dt_ms;
            if *t >= 2000 {
                self.stub_timer = None;
            }
        }
    }

    pub fn render(&self, gl: &glow::Context, font: &BitmapFont, screen_w: f32, screen_h: f32) {
        unsafe {
            gl.clear_color(0.08, 0.08, 0.12, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);

            let title = "Ruffle for PS Vita";
            let (tw, _) = font.measure(title);
            font.render_text(
                gl,
                title,
                (screen_w - tw as f32) / 2.0,
                8.0,
                (0.7, 0.7, 0.9, 1.0),
                screen_w,
                screen_h,
            );

            if self.files.is_empty() {
                let msg = "No SWF files found in ux0:data/ruffle/swf/";
                let (mw, _) = font.measure(msg);
                font.render_text(
                    gl,
                    msg,
                    (screen_w - mw as f32) / 2.0,
                    screen_h / 2.0 - 16.0,
                    (0.8, 0.3, 0.3, 1.0),
                    screen_w,
                    screen_h,
                );
            } else {
                let start_y = 34.0;
                let row_h = font.glyph_height() as f32 + 2.0;
                let end = (self.scroll_offset + MAX_VISIBLE).min(self.files.len());
                for i in self.scroll_offset..end {
                    let y = start_y + (i - self.scroll_offset) as f32 * row_h;
                    let display = &self.files[i];
                    let ext = ".swf";
                    let label = format!("{}{}", display, ext);
                    if i == self.selected {
                        font.render_text(gl, ">", 16.0, y, (1.0, 0.8, 0.2, 1.0), screen_w, screen_h);
                        font.render_text(gl, &label, 28.0, y, (1.0, 0.8, 0.2, 1.0), screen_w, screen_h);
                    } else {
                        font.render_text(gl, &label, 28.0, y, (0.8, 0.8, 0.8, 1.0), screen_w, screen_h);
                    }
                }
            }

            if let Some(_) = self.stub_timer {
                let (sw, _) = font.measure(STUB_MSG);
                font.render_text(
                    gl,
                    STUB_MSG,
                    (screen_w - sw as f32) / 2.0,
                    screen_h - 64.0,
                    (1.0, 0.7, 0.2, 1.0),
                    screen_w,
                    screen_h,
                );
            }

            if !self.files.is_empty() && self.stub_timer.is_none() {
                let bar_y = screen_h - 28.0;
                font.render_text(
                    gl,
                    "Cross: Launch Game",
                    12.0,
                    bar_y,
                    (0.6, 0.6, 0.8, 1.0),
                    screen_w,
                    screen_h,
                );
                let hint2 = "Triangle: Edit Config  |  Circle: Exit";
                let (h2w, _) = font.measure(hint2);
                font.render_text(
                    gl,
                    hint2,
                    screen_w - h2w as f32 - 12.0,
                    bar_y,
                    (0.6, 0.6, 0.8, 1.0),
                    screen_w,
                    screen_h,
                );
            }
        }
    }
}
