pub fn clock(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

pub fn size(bytes: u64) -> String {
    let mib = bytes as f64 / 1_048_576.0;
    if mib >= 1024.0 { format!("{:.1} GiB", mib / 1024.0) } else { format!("{mib:.1} MiB") }
}

pub fn started(rfc3339: &str) -> String {
    rfc3339.get(..19).map(|s| s.replace('T', " ")).unwrap_or_else(|| rfc3339.to_string())
}
