use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use handlebars::{Context, Handlebars, Helper, HelperDef, HelperResult, Output, RenderContext};
use tempfile::TempDir;

pub mod template {
    pub const RELEASE: &str = "release";
    pub const RELEASES: &str = "releases";
    pub const HOME: &str = "home";
    pub const TICKETS: &str = "tickets";
    pub const TICKETS_LIST: &str = "tickets_list";
}

const RELEASE_TEMPLATE: &str = include_str!("../handlebars/partials/release.hbs");
const RELEASES_TEMPLATE: &str = include_str!("../handlebars/pages/releases.hbs");
const HEAD_TEMPLATE: &str = include_str!("../handlebars/partials/head.hbs");
const TAILWIND_TEMPLATE: &str = include_str!("../handlebars/partials/tailwind.hbs");
const HOME_TEMPLATE: &str = include_str!("../handlebars/pages/home.hbs");
const NAVBAR_TEMPLATE: &str = include_str!("../handlebars/partials/navbar.hbs");
const TICKETS_TEMPLATE: &str = include_str!("../handlebars/pages/tickets.hbs");
const TICKETS_LIST_TEMPLATE: &str = include_str!("../handlebars/partials/tickets_list.hbs");

const TEMPLATES: [(&str, &str); 8] = [
    (template::RELEASE, RELEASE_TEMPLATE),
    (template::RELEASES, RELEASES_TEMPLATE),
    ("head", HEAD_TEMPLATE),
    ("tailwind", TAILWIND_TEMPLATE),
    (template::HOME, HOME_TEMPLATE),
    ("navbar", NAVBAR_TEMPLATE),
    (template::TICKETS, TICKETS_TEMPLATE),
    (template::TICKETS_LIST, TICKETS_LIST_TEMPLATE),
];

struct EqHelper;

impl HelperDef for EqHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        _reg: &'reg Handlebars<'reg>,
        _ctx: &'rc Context,
        _rc: &mut RenderContext<'reg, 'rc>,
        out: &mut dyn Output,
    ) -> HelperResult {
        let a = h.param(0).map_or(&serde_json::Value::Null, |p| p.value());
        let b = h.param(1).map_or(&serde_json::Value::Null, |p| p.value());
        out.write(&(a == b).to_string())?;
        Ok(())
    }
}

fn registry() -> &'static Result<Handlebars<'static>, String> {
    static REGISTRY: OnceLock<Result<Handlebars<'static>, String>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut reg = Handlebars::new();
        for (name, source) in TEMPLATES {
            reg.register_template_string(name, source)
                .map_err(|e| format!("failed to register template '{name}': {e}"))?;
        }
        reg.register_helper("eq", Box::new(EqHelper));
        Ok(reg)
    })
}

pub fn render(name: &str, data: &serde_json::Value) -> Result<String, String> {
    let reg = registry().as_ref().map_err(|e| e.clone())?;
    reg.render(name, data)
        .map_err(|e| format!("failed to render template '{name}': {e}"))
}

const CHROME_CANDIDATES: [&str; 6] = [
    "/opt/google/chrome/google-chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium-browser",
    "/usr/bin/chromium",
    "/snap/bin/chromium",
];

const CHROME_NAMES: [&str; 4] = [
    "google-chrome",
    "google-chrome-stable",
    "chromium-browser",
    "chromium",
];

fn find_chrome() -> Option<PathBuf> {
    CHROME_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .or_else(|| CHROME_NAMES.iter().find_map(|name| find_in_path(name)))
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn trim_whitespace(raw_path: &Path, trim_path: &Path) -> bool {
    let raw = raw_path.to_string_lossy();
    let trimmed = trim_path.to_string_lossy();

    match Command::new("magick")
        .args(["convert", raw.as_ref(), "-trim", trimmed.as_ref()])
        .output()
    {
        Ok(out) if out.status.success() => true,
        _ => Command::new("convert")
            .args([raw.as_ref(), "-trim", trimmed.as_ref()])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false),
    }
}

fn render_html(html: &str) -> Result<Vec<u8>, String> {
    let chrome = find_chrome().ok_or_else(|| {
        "Chrome/Chromium not found. Install it: sudo apt install chromium-browser".to_string()
    })?;

    let dir = TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let html_path = dir.path().join("card.html");
    let raw_path = dir.path().join("card.png");
    let trim_path = dir.path().join("card-trimmed.png");

    fs::write(&html_path, html.as_bytes()).map_err(|e| format!("failed to write html: {e}"))?;

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

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Chrome exited with error:\n{stderr}"));
    }

    let final_path = if trim_whitespace(&raw_path, &trim_path) {
        trim_path
    } else {
        raw_path
    };

    fs::read(&final_path).map_err(|e| format!("failed to read screenshot: {e}"))
}

pub fn release_card(title: &str, content: &str) -> Result<Vec<u8>, String> {
    let data = serde_json::json!({ "title": title,"content": content });
    let html = render(template::RELEASE, &data)?;
    render_html(&html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_initialized_once() {
        let first = registry();
        let second = registry();
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn release_template_substitutes_title() {
        let html = render(
            template::RELEASE,
            &serde_json::json!({"title": "New Release"}),
        )
        .unwrap();
        assert!(html.contains("New Release"));
        assert!(!html.contains("{{title}}"));
    }

    #[test]
    fn release_template_substitutes_content() {
        let html = render(template::RELEASE, &serde_json::json!({"content": "v1.2.3"})).unwrap();
        assert!(html.contains("v1.2.3"));
        assert!(!html.contains("{{content}}"));
    }

    #[test]
    fn releases_page_renders_list_and_links() {
        let html = render(
            template::RELEASES,
            &serde_json::json!({"releases": ["v1.0.0", "v2.0.0"]}),
        )
        .unwrap();
        assert!(html.contains("v1.0.0"));
        assert!(html.contains("v2.0.0"));
        assert!(html.contains("/dashboard/releases/v1.0.0"));
    }

    #[test]
    fn releases_page_renders_empty_state() {
        let html = render(template::RELEASES, &serde_json::json!({"releases": []})).unwrap();
        assert!(html.contains("No releases yet"));
    }

    #[test]
    fn render_unknown_template_errors() {
        assert!(render("does-not-exist", &serde_json::json!({})).is_err());
    }

    #[test]
    fn navbar_highlights_active_page() {
        let html = render("navbar", &serde_json::json!({"active": "releases"})).unwrap();
        assert!(html.contains(r#"href="/dashboard/releases""#));
        assert!(html.contains("bg-primary"));
        assert!(!html.contains("aria-current"));
    }

    #[test]
    fn navbar_links_both_pages() {
        let html = render("navbar", &serde_json::json!({"active": "home"})).unwrap();
        assert!(html.contains(r#"href="/""#));
        assert!(html.contains(r#"href="/dashboard/releases""#));
    }
}
