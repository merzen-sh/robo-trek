use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn template_dir() -> PathBuf {
    std::env::var("TEMPLATES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("templates"))
}

fn load_template(name: &str) -> Result<String, String> {
    let path = template_dir().join(name);
    fs::read_to_string(&path).map_err(|e| {
        format!("failed to read template {}: {e}", path.display())
    })
}

fn fill_template(name: &str, vars: &[(&str, &str)]) -> Result<String, String> {
    let mut tpl = load_template(name)?;
    for (key, val) in vars {
        tpl = tpl.replace(&format!("{{{key}}}"), val);
    }
    Ok(tpl)
}

fn find_chrome() -> Option<PathBuf> {
    let candidates = [
        "/opt/google/chrome/google-chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium-browser",
        "/usr/bin/chromium",
        "/snap/bin/chromium",
    ];
    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    for name in &["google-chrome", "google-chrome-stable", "chromium-browser", "chromium"] {
        if let Ok(out) = Command::new("which").arg(name).output() {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

fn render_html(html: &str) -> Result<Vec<u8>, String> {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir();
    let html_path = tmp.join(format!("robo-trek-{id}.html"));
    let raw_path = tmp.join(format!("robo-trek-{id}.png"));
    let trim_path = tmp.join(format!("robo-trek-{id}-trimmed.png"));

    let mut f = fs::File::create(&html_path)
        .map_err(|e| format!("failed to create temp html: {e}"))?;
    f.write_all(html.as_bytes())
        .map_err(|e| format!("failed to write html: {e}"))?;
    drop(f);

    let chrome = find_chrome().ok_or_else(|| {
        "Chrome/Chromium not found. Install it: sudo apt install chromium-browser".to_string()
    })?;

    let output = Command::new(&chrome)
        .args([
            "--headless=new",
            "--disable-gpu",
            &format!("--screenshot={}", raw_path.display()),
            "--window-size=640,480",
            &format!("file://{}", html_path.display()),
        ])
        .output()
        .map_err(|e| format!("failed to launch {}: {e}", chrome.display()))?;

    let _ = fs::remove_file(&html_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&raw_path);
        return Err(format!("Chrome exited with error:\n{stderr}"));
    }

    let trim_result = Command::new("magick")
        .args(["convert", &raw_path.to_string_lossy(), "-trim", &trim_path.to_string_lossy()])
        .output()
        .or_else(|_| {
            Command::new("convert")
                .args([&raw_path.to_string_lossy(), "-trim", &trim_path.to_string_lossy()])
                .output()
        });

    let final_path = match trim_result {
        Ok(out) if out.status.success() => {
            let _ = fs::remove_file(&raw_path);
            trim_path
        }
        _ => {
            let _ = fs::remove_file(&trim_path);
            raw_path
        }
    };

    let png = fs::read(&final_path)
        .map_err(|e| format!("failed to read screenshot: {e}"))?;
    let _ = fs::remove_file(&final_path);

    Ok(png)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn release_card(version: &str) -> Result<Vec<u8>, String> {
    let html = fill_template("release.html", &[("version", &escape_html(version))])?;
    render_html(&html)
}
