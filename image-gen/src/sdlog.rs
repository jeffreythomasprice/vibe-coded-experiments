use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

use diffusion_rs_sys::{sd_log_level_t, sd_set_log_callback, sd_set_progress_callback};

pub fn init() {
    unsafe {
        sd_set_log_callback(Some(log_callback), std::ptr::null_mut());
        sd_set_progress_callback(Some(progress_callback), std::ptr::null_mut());
    }
}

unsafe extern "C" fn log_callback(level: sd_log_level_t, text: *const c_char, _data: *mut c_void) {
    if text.is_null() {
        return;
    }
    let message = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    let message = message.trim_end();
    match level {
        sd_log_level_t::SD_LOG_DEBUG => tracing::debug!(target: "image_gen::sd", "{message}"),
        sd_log_level_t::SD_LOG_INFO => tracing::info!(target: "image_gen::sd", "{message}"),
        sd_log_level_t::SD_LOG_WARN => tracing::warn!(target: "image_gen::sd", "{message}"),
        sd_log_level_t::SD_LOG_ERROR => tracing::error!(target: "image_gen::sd", "{message}"),
        _ => tracing::trace!(target: "image_gen::sd", "{message}"),
    }
}

/// Redraws stable-diffusion.cpp's progress bar to stderr, mirroring
/// `pretty_progress`/`print_progress_line` in util.cpp. Installing this callback also
/// stops the native code from `printf`-ing the same bar to stdout.
unsafe extern "C" fn progress_callback(step: c_int, steps: c_int, time: f32, _data: *mut c_void) {
    if step == 0 {
        return;
    }

    let (speed, unit) = if time > 0.0 && time < 1.0 {
        (1.0 / time, "it/s")
    } else {
        (time, "s/it")
    };

    let width = 50;
    let current = if steps > 0 {
        (step as f32 * width as f32 / steps as f32) as i32
    } else {
        0
    };
    let mut bar = String::from("  |");
    for i in 0..width {
        if i > current {
            bar.push(' ');
        } else if i == current && i != width - 1 {
            bar.push('>');
        } else {
            bar.push('=');
        }
    }
    bar.push('|');

    let end = if step == steps { "\n" } else { "" };
    eprint!("\r{bar} {step}/{steps} - {speed:.2}{unit}\x1b[K{end}");
}
