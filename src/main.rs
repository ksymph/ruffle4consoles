#![allow(unused_variables)]
#![allow(dead_code)]

mod backends;
mod bitmap_font;
mod config_menu;
mod font_data;
mod menu;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ron::de::from_reader;
use ron::from_str;

use ruffle_core::backend::navigator::{NullExecutor, NullNavigatorBackend};
use ruffle_core::config::Letterbox;
use ruffle_core::events::{GamepadButton, MouseButton, KeyCode, TextControlCode};
use ruffle_core::limits::ExecutionLimit;
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{PlayerBuilder, PlayerEvent, ViewportDimensions};

use ruffle_render::quality::StageQuality;
use ruffle_render_glow::GlowRenderBackend;

use tracing_subscriber::layer::SubscriberExt;
use serde::{Deserialize, Serialize};
use sdl2::controller::Axis;

use backends::log::ConsoleLogBackend;
use backends::ui::SdlUiBackend;
use backends::audio::SdlAudioBackend;
use backends::storage::DiskStorageBackend;

use glow::HasContext;
use bitmap_font::BitmapFont;
use menu::{MenuAction, MenuState};

#[cfg(target_os = "horizon")]
use core::ffi::c_void;

#[cfg(target_os = "vita")]
type SceGxmMultisampleMode = u32;
#[cfg(target_os = "vita")]
pub const SCE_GXM_MULTISAMPLE_NONE: SceGxmMultisampleMode = 0;
#[cfg(target_os = "vita")]
pub const SCE_GXM_MULTISAMPLE_2X: SceGxmMultisampleMode = 1;
#[cfg(target_os = "vita")]
pub const SCE_GXM_MULTISAMPLE_4X: SceGxmMultisampleMode = 2;

#[cfg(target_os = "vita")]
static VGL_MODE_POSTPONED: u32 = 2;

#[cfg(target_os = "vita")]
#[link(name = "SDL2", kind = "static")]
#[link(name = "vitaGL", kind = "static")]
#[link(name = "stdc++", kind = "static")]
#[link(name = "vitashark", kind = "static")]
#[link(name = "SceShaccCg_stub", kind = "static")]
#[link(name = "mathneon", kind = "static")]
#[link(name = "SceShaccCgExt", kind = "static")]
#[link(name = "taihen_stub", kind = "static")]
#[link(name = "SceKernelDmacMgr_stub", kind = "static")]
#[link(name = "SceIme_stub", kind = "static")]
unsafe extern "C" {
    pub fn vglInitWithCustomThreshold(
        pool_size: i32,
        width: i32,
        height: i32,
        ram_reteshold: i32,
        cdram_threshold: i32,
        phycont_threshold: i32,
        cdlg_threshold: i32,
        msaa: SceGxmMultisampleMode,
    ) -> bool;
    pub fn vglSetSemanticBindingMode(mode: u32);
    pub fn vglSetParamBufferSize(size: u32);
    pub fn vglUseCachedMem(r#use: bool);
    pub fn vglUseTripleBuffering(usage: bool);
}

#[used]
#[unsafe(export_name = "_newlib_heap_size_user")]
pub static _NEWLIB_HEAP_SIZE_USER: u32 = 246 * 1024 * 1024;

#[cfg(target_os = "horizon")]
unsafe extern "C" {
    pub fn randomGet(buf: *mut c_void, len: usize);
    pub fn appletGetDefaultDisplayResolution(width: *mut i32, height: *mut i32) -> u32;
}

#[cfg(target_os = "horizon")]
static _SC_PAGESIZE: i32 = 30;
#[cfg(target_os = "horizon")]
static _SC_HOST_NAME_MAX: u32 = 33;
#[cfg(target_os = "horizon")]
static GRND_RANDOM: u32 = 0x2;

#[cfg(target_os = "horizon")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrandom(buf: *mut c_void, mut buflen: usize, flags: u32) -> isize {
    let maxlen = if flags & GRND_RANDOM != 0 {
        512
    } else {
        0x1FF_FFFF
    };
    buflen = buflen.min(maxlen);
    unsafe {
        randomGet(buf, buflen);
    }
    buflen as isize
}

#[cfg(target_os = "horizon")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysconf(name: i32) -> i64 {
    if name == _SC_PAGESIZE {
        return 4096;
    } else {
        return -1;
    }
}

#[cfg(target_os = "horizon")]
pub fn get_default_display_resolution() -> Result<(u32, u32), u32> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;
    let rc = unsafe { appletGetDefaultDisplayResolution(&mut width, &mut height) };
    if rc == 0 {
        Ok((width as u32, height as u32))
    } else {
        Err(rc)
    }
}

pub struct AxisState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

impl Default for AxisState {
    fn default() -> Self {
        AxisState {
            up: false,
            down: false,
            left: false,
            right: false,
        }
    }
}

#[cfg(target_os = "vita")]
const BASE_PATH: &str = "ux0:data/ruffle";

#[cfg(target_os = "horizon")]
const BASE_PATH: &str = "/switch/ruffle";

#[cfg(not(any(target_os = "horizon", target_os = "vita")))]
const BASE_PATH: &str = "./ruffle";

const DEFAULT_CONFIG: &str = "
Config(
    gamepad_config: {},
    letterbox: Some(\"on\"),
)";

use tracing_subscriber::util::SubscriberInitExt;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub gamepad_config: HashMap<String, u32>,
    pub letterbox: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            gamepad_config: HashMap::new(),
            letterbox: Some("on".to_string()),
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,ruffle_core=info,ruffle_render_glow=info")
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init();
}

fn load_config_for_swf(
    base_path: &str,
    swf_name: &str,
) -> (HashMap<GamepadButton, KeyCode>, Letterbox) {
    let config_dir = format!("{}/config", base_path);
    let _ = std::fs::create_dir_all(&config_dir);
    let config_file = format!("{}/{}.ron", config_dir, swf_name);

    let config: Config = match File::open(&config_file) {
        Ok(f) => match from_reader(f) {
            Ok(c) => c,
            Err(e) => {
                println!("Couldn't parse config file: {} ({})", config_file, e);
                from_str(DEFAULT_CONFIG).unwrap()
            }
        },
        Err(_) => {
            println!("No config for {}, creating defaults", swf_name);
            if let Ok(mut f) = File::create(&config_file) {
                let _ = f.write_all(DEFAULT_CONFIG.as_bytes());
            }
            from_str(DEFAULT_CONFIG).unwrap()
        }
    };

    let mut gamepad_button_mapping: HashMap<GamepadButton, KeyCode> = HashMap::new();
    for (button, key) in config.gamepad_config.into_iter() {
        if let Ok(gb) = GamepadButton::from_str(&button) {
            gamepad_button_mapping.insert(gb, KeyCode::from_code(key));
        }
    }
    let letterbox = Letterbox::from_str(&config.letterbox.unwrap_or("on".to_string()))
        .unwrap_or(Letterbox::On);
    (gamepad_button_mapping, letterbox)
}

pub fn main() {
    unsafe { std::env::set_var("RUST_BACKTRACE", "1"); }
    init_tracing();

    #[cfg(target_os = "vita")]
    {
        unsafe {
            let id = vitasdk_sys::sceKernelGetThreadId();
            vitasdk_sys::sceKernelChangeThreadPriority(id, vitasdk_sys::SCE_KERNEL_PROCESS_PRIORITY_USER_HIGH as _);
            vitasdk_sys::sceKernelChangeThreadCpuAffinityMask(id, vitasdk_sys::SCE_KERNEL_CPU_MASK_USER_0 as _);
        }
    }

    sdl2::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");

    let sdl2_context = sdl2::init().unwrap();
    let sdl2_video = sdl2_context.video().unwrap();
    let sdl2_game_controller = sdl2_context.game_controller().unwrap();
    let sdl2_joystick = sdl2_context.joystick().unwrap();

    #[cfg(target_os = "vita")]
    unsafe {
        vglSetSemanticBindingMode(VGL_MODE_POSTPONED);
        vglUseCachedMem(false);
        vglUseTripleBuffering(false);
        vglSetParamBufferSize(4 * 1024 * 1024);
        vglInitWithCustomThreshold(
            0,
            960,
            544,
            4 * 1024 * 1024,
            0,
            0,
            0,
            SCE_GXM_MULTISAMPLE_NONE,
        );
    }

    let gl_attr = sdl2_video.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::GLES);
    gl_attr.set_context_version(2, 0);
    let _ = sdl2_video.gl_set_swap_interval(0);

    #[cfg(target_os = "vita")]
    let mut dimensions = ViewportDimensions {
        width: 960,
        height: 544,
        scale_factor: 1.0,
    };

    #[cfg(target_os = "horizon")]
    let (display_width, display_height) = get_default_display_resolution().unwrap();

    #[cfg(target_os = "horizon")]
    let mut dimensions = ViewportDimensions {
        width: display_width,
        height: display_height,
        scale_factor: 1.0,
    };

    #[cfg(not(any(target_os = "horizon", target_os = "vita")))]
    let mut dimensions = ViewportDimensions {
        width: 1280,
        height: 720,
        scale_factor: 1.0,
    };

    let sdl2_window = sdl2_video
        .window("ruffle4consoles", dimensions.width, dimensions.height)
        .opengl()
        .resizable()
        .position_centered()
        .build()
        .unwrap();

    let gl_context = sdl2_window.gl_create_context().unwrap();
    let _ = sdl2_window.gl_make_current(&gl_context);

    let glow_context = Arc::new(unsafe {
        glow::Context::from_loader_function(|s| sdl2_video.gl_get_proc_address(s) as *const _)
    });

    let font = BitmapFont::new(&glow_context);

    let mut controllers: Vec<sdl2::controller::GameController> = Vec::new();
    for i in 0..sdl2_joystick.num_joysticks().unwrap() {
        if sdl2_game_controller.is_game_controller(i) {
            controllers.push(sdl2_game_controller.open(i).unwrap());
        }
    }

    let mut event_pump = sdl2_context.event_pump().unwrap();
    let mut menu_state = MenuState::new(BASE_PATH);
    let mut last_render = Instant::now();
    let mut player_state: Option<(Arc<Mutex<ruffle_core::Player>>, NullExecutor, Instant)> = None;
    let mut start_held = false;

    'main: loop {
        if player_state.is_some() {
            // ============= PLAYING STATE =============
            let (player, mut executor, mut last_frame_time) = player_state.take().unwrap();
            let mut return_to_menu = false;

            #[cfg(target_os = "horizon")]
            {
                let (nx_width, nx_height) = sdl2_window.drawable_size();
                if nx_width != dimensions.width || nx_height != dimensions.height {
                    dimensions.width = nx_width;
                    dimensions.height = nx_height;
                    player.lock().unwrap().set_viewport_dimensions(dimensions);
                }
            }

            for event in event_pump.poll_iter() {
                match event {
                    sdl2::event::Event::Quit { .. } => {
                        return_to_menu = true;
                        break;
                    }
                    sdl2::event::Event::Window {
                        win_event: sdl2::event::WindowEvent::Resized(w, h),
                        ..
                    } => {
                        if w > 0 && h > 0 {
                            dimensions.width = w as u32;
                            dimensions.height = h as u32;
                            player.lock().unwrap().set_viewport_dimensions(dimensions);
                        }
                    }
                    sdl2::event::Event::ControllerDeviceAdded { which, .. } => {
                        controllers.push(sdl2_game_controller.open(which).unwrap());
                    }
                    sdl2::event::Event::ControllerDeviceRemoved { which, .. } => {
                        if let Some(pos) = controllers.iter().position(|c| c.instance_id() == which) {
                            controllers.remove(pos);
                        }
                    }
                    sdl2::event::Event::ControllerButtonDown { button, .. } => {
                        if button == sdl2::controller::Button::Start {
                            start_held = true;
                        } else if button == sdl2::controller::Button::Back && start_held {
                            return_to_menu = true;
                            break;
                        }
                        let ruffle_button = sdl_gamepadbutton_to_ruffle(button);
                        if let Some(ruffle_button) = ruffle_button {
                            player
                                .lock()
                                .unwrap()
                                .handle_event(PlayerEvent::GamepadButtonDown {
                                    button: ruffle_button,
                                });
                        }
                    }
                    sdl2::event::Event::ControllerButtonUp { button, .. } => {
                        if button == sdl2::controller::Button::Start {
                            start_held = false;
                        }
                        let ruffle_button = sdl_gamepadbutton_to_ruffle(button);
                        if let Some(ruffle_button) = ruffle_button {
                            player
                                .lock()
                                .unwrap()
                                .handle_event(PlayerEvent::GamepadButtonUp {
                                    button: ruffle_button,
                                });
                        }
                    }
                    sdl2::event::Event::ControllerAxisMotion { axis, value, .. } => {
                        let x_axis = axis == Axis::LeftX;
                        let y_axis = axis == Axis::LeftY;
                        let deadzone = 8000;
                        let up = y_axis && value < -deadzone;
                        let down = y_axis && value > deadzone;
                        let left = x_axis && value < -deadzone;
                        let right = x_axis && value > deadzone;
                        if up {
                            player.lock().unwrap().handle_event(PlayerEvent::GamepadButtonDown {
                                button: GamepadButton::DPadUp,
                            });
                        }
                        if down {
                            player.lock().unwrap().handle_event(PlayerEvent::GamepadButtonDown {
                                button: GamepadButton::DPadDown,
                            });
                        }
                        if left {
                            player.lock().unwrap().handle_event(PlayerEvent::GamepadButtonDown {
                                button: GamepadButton::DPadLeft,
                            });
                        }
                        if right {
                            player.lock().unwrap().handle_event(PlayerEvent::GamepadButtonDown {
                                button: GamepadButton::DPadRight,
                            });
                        }
                    }
                    sdl2::event::Event::FingerMotion { x, y, .. } => {
                        player.lock().unwrap().handle_event(PlayerEvent::MouseMove {
                            x: x as f64 * dimensions.width as f64,
                            y: y as f64 * dimensions.height as f64,
                        });
                    }
                    sdl2::event::Event::FingerDown { x, y, .. } => {
                        player.lock().unwrap().handle_event(PlayerEvent::MouseDown {
                            x: x as f64 * dimensions.width as f64,
                            y: y as f64 * dimensions.height as f64,
                            button: MouseButton::Left,
                            index: None,
                        });
                    }
                    sdl2::event::Event::FingerUp { x, y, .. } => {
                        player.lock().unwrap().handle_event(PlayerEvent::MouseUp {
                            x: x as f64 * dimensions.width as f64,
                            y: y as f64 * dimensions.height as f64,
                            button: MouseButton::Left,
                        });
                    }
                    sdl2::event::Event::TextInput { text, .. } => {
                        for codepoint in text.chars() {
                            player
                                .lock()
                                .unwrap()
                                .handle_event(PlayerEvent::TextInput { codepoint });
                        }
                    }
                    sdl2::event::Event::KeyDown { scancode, .. } => {
                        if scancode == Some(sdl2::keyboard::Scancode::Backspace) {
                            player.lock().unwrap().handle_event(PlayerEvent::TextControl {
                                code: TextControlCode::Backspace,
                            });
                        }
                    }
                    _ => {}
                }
            }

            if !return_to_menu {
                let new_time = Instant::now();
                let dt = new_time.duration_since(last_frame_time).as_micros();
                executor.run();
                if dt > 0 {
                    last_frame_time = new_time;
                    if let Ok(mut p) = player.lock() {
                        p.tick(dt as f64 / 1000.0);
                        if p.needs_render() {
                            p.render();
                            sdl2_window.gl_swap_window();
                        }
                    } else {
                        tracing::error!("player lock poisoned — game has likely panicked");
                    }
                }
                player_state = Some((player, executor, last_frame_time));
            }
        } else {
            // ============= MENU STATE =============
            let dt = last_render.elapsed().as_millis();
            last_render = Instant::now();
            menu_state.update_stub_timer(dt);

            let mut should_exit = false;
            for event in event_pump.poll_iter() {
                match event {
                    sdl2::event::Event::Quit { .. } => {
                        should_exit = true;
                    }
                    sdl2::event::Event::ControllerDeviceAdded { which, .. } => {
                        controllers.push(sdl2_game_controller.open(which).unwrap());
                    }
                    sdl2::event::Event::ControllerDeviceRemoved { which, .. } => {
                        if let Some(pos) = controllers.iter().position(|c| c.instance_id() == which) {
                            controllers.remove(pos);
                        }
                    }
                    sdl2::event::Event::ControllerButtonDown { button, .. } => {
                        if let Some(action) = menu_state.handle_button(true, button) {
                            match action {
                                MenuAction::Launch(swf_name) => {
                                    let (gamepad_button_mapping, letterbox_config) =
                                        load_config_for_swf(BASE_PATH, &swf_name);
                                    player_state = launch_game(
                                        &sdl2_window,
                                        &gl_context,
                                        &glow_context,
                                        &sdl2_context,
                                        &sdl2_video,
                                        swf_name,
                                        gamepad_button_mapping,
                                        letterbox_config,
                                        dimensions,
                                    );
                                    menu_state.refresh(BASE_PATH);
                                    last_render = Instant::now();
                                }
                                MenuAction::Exit => {
                                    should_exit = true;
                                }
                            }
                        }
                    }
                    sdl2::event::Event::ControllerAxisMotion { axis, value, .. } => {
                        menu_state.handle_axis_motion(axis, value as i32);
                    }
                    _ => {}
                }
            }

            if should_exit {
                break 'main;
            }

            unsafe {
                glow_context.clear_color(0.08, 0.08, 0.12, 1.0);
                glow_context.clear(glow::COLOR_BUFFER_BIT);
            }
            menu_state.render(
                &glow_context,
                &font,
                dimensions.width as f32,
                dimensions.height as f32,
            );
            sdl2_window.gl_swap_window();
        }
    }
}

fn launch_game(
    sdl2_window: &sdl2::video::Window,
    _gl_context: &sdl2::video::GLContext,
    glow_context: &Arc<glow::Context>,
    sdl2_context: &sdl2::Sdl,
    sdl2_video: &sdl2::VideoSubsystem,
    swf_name: String,
    gamepad_button_mapping: HashMap<GamepadButton, KeyCode>,
    letterbox_config: Letterbox,
    dimensions: ViewportDimensions,
) -> Option<(Arc<Mutex<ruffle_core::Player>>, NullExecutor, Instant)> {
    let swf_url = format!("file:///{}/{}.swf", BASE_PATH, swf_name);
    let swf_path = format!("{}/swf/{}.swf", BASE_PATH, swf_name);

    let swf_data = match std::fs::read(&swf_path) {
        Ok(d) => d,
        Err(e) => {
            println!("Couldn't load {}: {}", swf_path, e);
            return None;
        }
    };

    let movie = match SwfMovie::from_data(&swf_data, swf_url.into(), None) {
        Ok(m) => m,
        Err(e) => {
            println!("Couldn't parse {}: {}", swf_path, e);
            return None;
        }
    };

    let renderer = GlowRenderBackend::new(glow_context.clone(), false, StageQuality::High).unwrap();
    let audio = SdlAudioBackend::new(sdl2_context.audio().unwrap()).unwrap();
    let ui_backend = SdlUiBackend::new(Box::new(sdl2_window.clone()));

    let storage_path = format!("{}/{}", BASE_PATH, "storage");
    let _ = std::fs::create_dir_all(storage_path.clone());
    let executor = NullExecutor::new();

    let player = PlayerBuilder::new()
        .with_renderer(renderer)
        .with_ui(ui_backend)
        .with_storage(Box::new(DiskStorageBackend::new(std::path::PathBuf::from(
            storage_path,
        ))))
        .with_navigator(
            NullNavigatorBackend::with_base_path(std::path::Path::new(BASE_PATH), &executor)
                .unwrap(),
        )
        .with_movie(movie)
        .with_viewport_dimensions(dimensions.width, dimensions.height, dimensions.scale_factor)
        .with_fullscreen(true)
        .with_letterbox(letterbox_config)
        .with_player_runtime(ruffle_core::PlayerRuntime::AIR)
        .with_gamepad_button_mapping(gamepad_button_mapping)
        .with_autoplay(true)
        .with_log(ConsoleLogBackend::default())
        .build();

    let preload_start = Instant::now();
    let preload_timeout = Duration::from_secs(30);
    loop {
        let mut limit = ExecutionLimit::with_max_ops_and_time(100000, Duration::from_secs(10));
        let done = player.lock().unwrap().preload(&mut limit);
        let elapsed = preload_start.elapsed();
        tracing::info!("preload running... {:?} elapsed", elapsed);
        if done || elapsed >= preload_timeout {
            if !done {
                tracing::error!(
                    "preload did not complete within {:?}, aborting",
                    preload_timeout
                );
            }
            break;
        }
    }

    Some((player, executor, Instant::now()))
}

fn sdl_gamepadbutton_to_ruffle(button: sdl2::controller::Button) -> Option<GamepadButton> {
    return match button {
        sdl2::controller::Button::DPadUp => Some(GamepadButton::DPadUp),
        sdl2::controller::Button::DPadDown => Some(GamepadButton::DPadDown),
        sdl2::controller::Button::DPadLeft => Some(GamepadButton::DPadLeft),
        sdl2::controller::Button::DPadRight => Some(GamepadButton::DPadRight),
        sdl2::controller::Button::A => Some(GamepadButton::South),
        sdl2::controller::Button::B => Some(GamepadButton::East),
        sdl2::controller::Button::X => Some(GamepadButton::West),
        sdl2::controller::Button::Y => Some(GamepadButton::North),
        sdl2::controller::Button::Start => Some(GamepadButton::Start),
        sdl2::controller::Button::Back => Some(GamepadButton::Select),
        sdl2::controller::Button::RightShoulder => Some(GamepadButton::RightTrigger),
        sdl2::controller::Button::LeftShoulder => Some(GamepadButton::LeftTrigger),
        _ => None,
    };
}
