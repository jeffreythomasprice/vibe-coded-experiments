use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

use diffusion_rs_sys::{sd_log_level_t, sd_set_log_callback};

pub fn init() {
    unsafe {
        sd_set_log_callback(Some(log_callback), std::ptr::null_mut());
    }
}

unsafe extern "C" fn log_callback(level: sd_log_level_t, text: *const c_char, _data: *mut c_void) {
    if text.is_null() {
        return;
    }
    let message = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    let message = message.trim_end();
    match level {
        sd_log_level_t::SD_LOG_DEBUG => tracing::debug!(target: "diffusion::sd", "{message}"),
        sd_log_level_t::SD_LOG_INFO => tracing::info!(target: "diffusion::sd", "{message}"),
        sd_log_level_t::SD_LOG_WARN => tracing::warn!(target: "diffusion::sd", "{message}"),
        sd_log_level_t::SD_LOG_ERROR => tracing::error!(target: "diffusion::sd", "{message}"),
        _ => tracing::trace!(target: "diffusion::sd", "{message}"),
    }
}
