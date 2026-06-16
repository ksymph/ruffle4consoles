use std::time::{Duration, Instant};
use glow::HasContext;

/// Tracks game loop health metrics for diagnosing freezes.
pub struct DiagnosticsState {
    /// Total frames processed since game launch.
    pub frame_count: u64,
    /// Total time spent in player.tick() since last heartbeat.
    pub tick_time: Duration,
    /// Total time spent in player.render() since last heartbeat.
    pub render_time: Duration,
    /// Total time spent in executor.run() since last heartbeat.
    pub executor_time: Duration,
    /// Number of times render was actually performed since last heartbeat.
    pub render_count: u64,
    /// Last heartbeat timestamp.
    pub last_heartbeat: Instant,
    /// Timestamp of game launch.
    pub game_start: Instant,
    /// How often to print heartbeat (default: 1 second).
    pub heartbeat_interval: Duration,
    /// How often to check memory (default: 5 seconds).
    pub memory_interval: Duration,
    /// Last memory check timestamp.
    pub last_memory_check: Instant,
    /// Last tick time (single frame, not accumulated).
    pub last_tick_time: Duration,
    /// Last render time (single call).
    pub last_render_time: Duration,
    /// Last dt value passed to tick().
    pub last_dt: Duration,
    /// Whether a tick is currently in progress (for freeze detection).
    pub tick_in_progress: bool,
    /// Whether a render is currently in progress.
    pub render_in_progress: bool,
    /// Timestamp when current tick started (if in_progress).
    pub tick_start: Option<Instant>,
    /// Timestamp when current render started (if in_progress).
    pub render_start: Option<Instant>,
}

impl DiagnosticsState {
    pub fn new() -> Self {
        let now = Instant::now();
        DiagnosticsState {
            frame_count: 0,
            tick_time: Duration::ZERO,
            render_time: Duration::ZERO,
            executor_time: Duration::ZERO,
            render_count: 0,
            last_heartbeat: now,
            game_start: now,
            heartbeat_interval: Duration::from_secs(1),
            memory_interval: Duration::from_secs(5),
            last_memory_check: now,
            last_tick_time: Duration::ZERO,
            last_render_time: Duration::ZERO,
            last_dt: Duration::ZERO,
            tick_in_progress: false,
            render_in_progress: false,
            tick_start: None,
            render_start: None,
        }
    }

    /// Returns elapsed time since game launch.
    pub fn elapsed(&self) -> Duration {
        self.game_start.elapsed()
    }

    /// Returns true if it's time to print a heartbeat.
    pub fn should_heartbeat(&self) -> bool {
        self.last_heartbeat.elapsed() >= self.heartbeat_interval
    }

    /// Returns true if it's time to check memory.
    pub fn should_check_memory(&self) -> bool {
        self.last_memory_check.elapsed() >= self.memory_interval
    }

    /// Resets per-heartbeat counters.
    pub fn reset_heartbeat(&mut self) {
        self.tick_time = Duration::ZERO;
        self.render_time = Duration::ZERO;
        self.executor_time = Duration::ZERO;
        self.render_count = 0;
        self.last_heartbeat = Instant::now();
    }

    /// Resets the memory check timer.
    pub fn reset_memory_check(&mut self) {
        self.last_memory_check = Instant::now();
    }

    /// Mark tick as starting.
    pub fn begin_tick(&mut self) {
        self.tick_in_progress = true;
        self.tick_start = Some(Instant::now());
    }

    /// Mark tick as finished, record duration.
    pub fn end_tick(&mut self) {
        if let Some(start) = self.tick_start.take() {
            self.last_tick_time = start.elapsed();
            self.tick_time += self.last_tick_time;
        }
        self.tick_in_progress = false;
    }

    /// Mark render as starting.
    pub fn begin_render(&mut self) {
        self.render_in_progress = true;
        self.render_start = Some(Instant::now());
    }

    /// Mark render as finished, record duration.
    pub fn end_render(&mut self) {
        if let Some(start) = self.render_start.take() {
            self.last_render_time = start.elapsed();
            self.render_time += self.last_render_time;
        }
        self.render_in_progress = false;
        self.render_count += 1;
    }
}

/// Queries free memory on PS Vita using sceKernelGetFreeMemorySize.
/// Returns (free_kernel, free_user, free_cdram, free_phycont) in bytes, or None on error.
#[cfg(target_os = "vita")]
pub fn get_vita_memory() -> Option<(usize, usize, usize, usize)> {
    unsafe {
        let mut info = vitasdk_sys::SceKernelFreeMemorySizeInfo {
            size: 0,
            size_user: 0,
            size_cdram: 0,
            size_phycont: 0,
        };
        let ret = vitasdk_sys::sceKernelGetFreeMemorySize(&mut info);
        if ret == 0 {
            Some((
                info.size as usize,
                info.size_user as usize,
                info.size_cdram as usize,
                info.size_phycont as usize,
            ))
        } else {
            None
        }
    }
}

/// Returns None on non-Vita platforms.
#[cfg(not(target_os = "vita"))]
pub fn get_vita_memory() -> Option<(usize, usize, usize, usize)> {
    None
}

/// Format bytes as a human-readable string (KB, MB).
pub fn format_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Check for pending OpenGL errors and log them.
/// Returns the number of errors found.
pub fn check_gl_errors(context: &glow::Context, label: &str) -> u32 {
    let mut count = 0u32;
    unsafe {
        loop {
            let err = context.get_error();
            if err == glow::NO_ERROR {
                break;
            }
            count += 1;
            let err_str = match err {
                glow::INVALID_ENUM => "INVALID_ENUM",
                glow::INVALID_VALUE => "INVALID_VALUE",
                glow::INVALID_OPERATION => "INVALID_OPERATION",
                glow::OUT_OF_MEMORY => "OUT_OF_MEMORY",
                glow::INVALID_FRAMEBUFFER_OPERATION => "INVALID_FRAMEBUFFER_OPERATION",
                _ => "UNKNOWN",
            };
            tracing::error!(
                "GL error {} (0x{:04X}) after {}",
                err_str,
                err,
                label
            );
        }
    }
    count
}

/// Log a comprehensive heartbeat with all diagnostics.
pub fn log_heartbeat(
    state: &DiagnosticsState,
    glow_context: &glow::Context,
) {
    let elapsed = state.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();

    // Memory
    let mem_str = if let Some((kernel, user, cdram, phycont)) = get_vita_memory() {
        format!(
            " | MEM: user={} kernel={} cdram={} phycont={}",
            format_bytes(user),
            format_bytes(kernel),
            format_bytes(cdram),
            format_bytes(phycont)
        )
    } else {
        String::new()
    };

    // GL errors
    let gl_err = unsafe {
        let mut count = 0u32;
        loop {
            let err = glow_context.get_error();
            if err == glow::NO_ERROR {
                break;
            }
            count += 1;
        }
        count
    };

    tracing::info!(
        "[HEARTBEAT @ {:.1}s] frames={} | tick_total={:.1}ms render_total={:.1}ms (x{}) executor={:.1}ms{} | last_tick={:.1}ms last_render={:.1}ms last_dt={:.1}ms | pending_gl_err={}",
        elapsed_secs,
        state.frame_count,
        state.tick_time.as_secs_f64() * 1000.0,
        state.render_time.as_secs_f64() * 1000.0,
        state.render_count,
        state.executor_time.as_secs_f64() * 1000.0,
        mem_str,
        state.last_tick_time.as_secs_f64() * 1000.0,
        state.last_render_time.as_secs_f64() * 1000.0,
        state.last_dt.as_secs_f64() * 1000.0,
        gl_err,
    );
}

/// Log a detailed memory snapshot.
pub fn log_memory_snapshot(label: &str) {
    if let Some((kernel, user, cdram, phycont)) = get_vita_memory() {
        tracing::info!(
            "[MEMORY @ {}] user={} kernel={} cdram={} phycont={}",
            label,
            format_bytes(user),
            format_bytes(kernel),
            format_bytes(cdram),
            format_bytes(phycont),
        );
    }
}
