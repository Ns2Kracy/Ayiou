use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use tokio::{fs, process::Command};

pub async fn render_html_to_png(html: &str) -> Result<Vec<u8>> {
    let dir = tempdir().context("failed to create temp dir for weather card")?;
    let html_path = dir.path().join("weather-card.html");
    let png_path = dir.path().join("weather-card.png");

    fs::write(&html_path, html)
        .await
        .with_context(|| format!("failed to write html file: {}", html_path.display()))?;

    let browser = detect_browser_bin().context(
        "No chrome/chromium executable found. Set WEATHER_SHOT_BIN to your browser binary.",
    )?;

    let file_url = format!("file://{}", html_path.display());
    let screenshot_arg = format!("--screenshot={}", png_path.display());

    let output = Command::new(browser)
        .args([
            "--headless",
            "--disable-gpu",
            "--hide-scrollbars",
            "--window-size=1200,675",
            screenshot_arg.as_str(),
            file_url.as_str(),
        ])
        .output()
        .await
        .context("failed to execute headless browser")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("weather card screenshot failed: {}", stderr.trim());
    }

    fs::read(&png_path)
        .await
        .with_context(|| format!("failed to read png output: {}", png_path.display()))
}

fn detect_browser_bin() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("WEATHER_SHOT_BIN") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    let candidates = [
        "google-chrome",
        "chromium",
        "chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ];

    for candidate in candidates {
        if candidate.starts_with('/') {
            let p = PathBuf::from(candidate);
            if p.exists() {
                return Ok(p);
            }
            continue;
        }

        if let Some(found) = find_in_path(candidate) {
            return Ok(found);
        }
    }

    bail!("browser executable not found")
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(name);
        if is_executable(&full) {
            return Some(full);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
}
