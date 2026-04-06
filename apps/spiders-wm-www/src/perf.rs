pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

pub fn log_timing(label: &str, started_ms: f64, extra: impl AsRef<str>) {
    let elapsed_ms = now_ms() - started_ms;
    let suffix = extra.as_ref();
    if suffix.is_empty() {
        web_sys::console::log_1(&format!("[perf] {} {:.2}ms", label, elapsed_ms).into());
    } else {
        web_sys::console::log_1(&format!("[perf] {} {:.2}ms {}", label, elapsed_ms, suffix).into());
    }
}
