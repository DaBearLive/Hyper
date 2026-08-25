use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use tauri::Emitter;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

fn hyper_cmd<S: AsRef<std::ffi::OsStr>>(program: S) -> Command {
    #[cfg(windows)]
    {
        let mut cmd = Command::new(program);
        cmd.creation_flags(0x08000000);
        return cmd;
    }
    #[cfg(not(windows))]
    Command::new(program)
}

fn use_base64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_default_save_path() -> String {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let hyper_path = base.join("HyperDownloads");
    let _ = std::fs::create_dir_all(&hyper_path);
    hyper_path.to_string_lossy().to_string()
}

#[tauri::command]
fn ensure_hyper_downloads() -> String {
    get_default_save_path()
}

// --- Bundled / downloaded binary helpers ---

fn bundled_bin_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

fn find_bundled_bin(bin: &str) -> Option<PathBuf> {
    let name = bundled_bin_name(bin);
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join(&name));
            candidates.push(parent.join("bin").join(&name));
            candidates.push(parent.join("resources").join(&name));
            candidates.push(parent.join("resources").join("bin").join(&name));
            candidates.push(parent.join("../Resources").join(&name));
            candidates.push(parent.join("../resources").join(&name));
            candidates.push(parent.join(format!("../lib/hyper/bin/{}", &name)));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin").join(&name));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("src-tauri/bin").join(&name));
        candidates.push(cwd.join("bin").join(&name));
    }
    for p in candidates {
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn downloaded_ffmpeg_bin() -> Option<PathBuf> {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("HyperDownloads").join(".ffmpeg");
    let p = dir.join(bundled_bin_name("ffmpeg"));
    if p.exists() { Some(p) } else { None }
}

fn system_ffmpeg_exists() -> bool {
    hyper_cmd(bundled_bin_name("ffmpeg"))
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ytdlp_bin() -> String {
    if let Some(p) = find_bundled_bin("yt-dlp") {
        return p.to_string_lossy().to_string();
    }
    bundled_bin_name("yt-dlp")
}

fn ffmpeg_dir() -> Option<String> {
    // 1. Downloaded on-demand in HyperDownloads/.ffmpeg
    if let Some(p) = downloaded_ffmpeg_bin() {
        if let Some(dir) = p.parent() {
            return Some(dir.to_string_lossy().to_string());
        }
    }
    // 2. Bundled (if we ever bundle again)
    if let Some(p) = find_bundled_bin("ffmpeg") {
        if let Some(dir) = p.parent() {
            return Some(dir.to_string_lossy().to_string());
        }
    }
    // 3. System PATH -> let yt-dlp find it, no need to pass --ffmpeg-location
    if system_ffmpeg_exists() {
        return None;
    }
    // No ffmpeg found at all -> still return None, caller can trigger ensure_ffmpeg
    None
}

#[tauri::command]
fn get_bundled_status() -> serde_json::Value {
    let ytdlp = find_bundled_bin("yt-dlp").map(|p| p.to_string_lossy().to_string());
    let ffmpeg_downloaded = downloaded_ffmpeg_bin().map(|p| p.to_string_lossy().to_string());
    let ffmpeg_bundled = find_bundled_bin("ffmpeg").map(|p| p.to_string_lossy().to_string());
    serde_json::json!({
        "ytdlp_bundled": ytdlp,
        "ffmpeg_downloaded": ffmpeg_downloaded,
        "ffmpeg_bundled": ffmpeg_bundled,
        "ffmpeg_dir": ffmpeg_dir(),
        "system_ffmpeg": system_ffmpeg_exists(),
        "ytdlp_bin_used": ytdlp_bin(),
        "save_path": get_default_save_path()
    })
}

#[tauri::command]
fn ensure_ffmpeg() -> Result<String, String> {
    if let Some(dir) = ffmpeg_dir() {
        return Ok(format!("ffmpeg ready at {}", dir));
    }
    if system_ffmpeg_exists() {
        return Ok("system ffmpeg found".to_string());
    }

    // Download on-demand
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let out_dir = base.join("HyperDownloads").join(".ffmpeg");
    let _ = std::fs::create_dir_all(&out_dir);

    #[cfg(target_os = "windows")]
    {
        let url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        let tmp = std::env::temp_dir().join("ffmpeg-win.zip");
        let dl = hyper_cmd("curl").args(["-L", "-o", &tmp.to_string_lossy(), url]).output()
            .map_err(|e| format!("curl not found: {}", e))?;
        if !dl.status.success() {
            return Err(format!("Failed to download ffmpeg: {}", String::from_utf8_lossy(&dl.stderr)));
        }
        // Try unzip via powershell or unzip
        let out = hyper_cmd("powershell")
            .args(["-Command", &format!("Expand-Archive -Force '{}' '{}'", tmp.to_string_lossy(), out_dir.to_string_lossy())])
            .output();
        if out.is_err() || !out.unwrap().status.success() {
            // fallback to unzip
            let _ = hyper_cmd("unzip").args(["-o", &tmp.to_string_lossy(), "-d", &out_dir.to_string_lossy()]).output();
        }
        // Find ffmpeg.exe inside
        if let Ok(entries) = std::fs::read_dir(&out_dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let cand = p.join("bin").join("ffmpeg.exe");
                    if cand.exists() {
                        let _ = std::fs::copy(&cand, out_dir.join("ffmpeg.exe"));
                        let probe = p.join("bin").join("ffprobe.exe");
                        if probe.exists() { let _ = std::fs::copy(&probe, out_dir.join("ffprobe.exe")); }
                        break;
                    }
                }
            }
        }
        let _ = std::fs::remove_file(tmp);
        if out_dir.join("ffmpeg.exe").exists() {
            return Ok(out_dir.to_string_lossy().to_string());
        }
        return Err("ffmpeg download failed - please install ffmpeg manually (winget install ffmpeg)".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        let url = "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz";
        let tmp = std::env::temp_dir().join("ffmpeg-linux.tar.xz");
        let dl = hyper_cmd("curl").args(["-L", "-o", &tmp.to_string_lossy(), url]).output()
            .map_err(|e| format!("curl failed: {}", e))?;
        if !dl.status.success() {
            return Err(format!("Failed to download ffmpeg: {}", String::from_utf8_lossy(&dl.stderr)));
        }
        let _ = hyper_cmd("tar").args(["-xf", &tmp.to_string_lossy(), "-C", &std::env::temp_dir().to_string_lossy()]).output();
        // Find extracted dir
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && p.file_name().unwrap_or_default().to_string_lossy().starts_with("ffmpeg-") {
                    let src_ff = p.join("ffmpeg");
                    let src_probe = p.join("ffprobe");
                    if src_ff.exists() {
                        let _ = std::fs::copy(&src_ff, out_dir.join("ffmpeg"));
                        let _ = std::fs::copy(&src_probe, out_dir.join("ffprobe"));
                        let _ = hyper_cmd("chmod").args(["+x", &out_dir.join("ffmpeg").to_string_lossy()]).output();
                        let _ = hyper_cmd("chmod").args(["+x", &out_dir.join("ffprobe").to_string_lossy()]).output();
                        let _ = std::fs::remove_dir_all(&p);
                        break;
                    }
                }
            }
        }
        let _ = std::fs::remove_file(tmp);
        if out_dir.join("ffmpeg").exists() {
            return Ok(out_dir.to_string_lossy().to_string());
        }
        return Err("ffmpeg download failed - please run sudo pacman -S ffmpeg".to_string());
    }
    #[cfg(target_os = "macos")]
    {
        return Err("Please install ffmpeg via brew install ffmpeg".to_string());
    }
}

// --- YouTube bot-check workaround ---

fn is_bot_error(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    s.contains("sign in to confirm") || s.contains("not a bot") || s.contains("cookies")
}

fn strategy_cache_file() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| std::env::temp_dir()).join("hyper").join("ytdlp_strategy.txt")
}

fn saved_strategy() -> Option<String> {
    std::fs::read_to_string(strategy_cache_file()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn save_strategy(s: &str) {
    if let Some(parent) = strategy_cache_file().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(strategy_cache_file(), s);
}

/// Run yt-dlp with automatic bot-check workarounds:
/// 1. plain  2. saved strategy  3. alternate player clients  4. browser cookies
fn run_ytdlp(bin: &str, base_args: &[String]) -> Result<String, String> {
    let mut extra_variants: Vec<(String, Vec<String>)> = Vec::new();
    extra_variants.push(("default".to_string(), vec![]));
    extra_variants.push(("clients".to_string(), vec![
        "--extractor-args".to_string(), "youtube:player_client=tv,web_safari,android_vr,mweb".to_string(),
    ]));
    for browser in ["brave", "chrome", "chromium", "firefox", "edge", "opera", "vivaldi", "whale"] {
        extra_variants.push((format!("cookies:{}", browser), vec![
            "--cookies-from-browser".to_string(), browser.to_string(),
        ]));
    }

    // Try saved strategy first if present
    let mut ordered: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(s) = saved_strategy() {
        if let Some(pos) = extra_variants.iter().position(|(name, _)| *name == s) {
            ordered.push(extra_variants.remove(pos));
        }
    }
    ordered.extend(extra_variants);

    let mut last_err = String::new();
    for (name, extra) in ordered {
        let mut cmd = hyper_cmd(bin);
        cmd.args(base_args).args(&extra);
        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                // browser not installed for cookies-from-browser etc.
                last_err = format!("{}", e);
                continue;
            }
        };
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            save_strategy(&name);
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
        last_err = stderr.clone();
        if !is_bot_error(&stderr) {
            // real error (bad URL, private video...) - no point trying other variants
            return Err(stderr);
        }
    }
    Err(last_err)
}

fn fmt_size(bytes: f64) -> String {
    if bytes >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.2} GB", bytes / 1024.0 / 1024.0 / 1024.0)
    } else {
        format!("{} MB", (bytes / 1024.0 / 1024.0).round() as u64)
    }
}

// Estimate downloaded size per quality tier from yt-dlp format data
fn fmt_bytes(f: &serde_json::Value, dur: f64) -> Option<f64> {
    if let Some(sz) = f.get("filesize").and_then(|v| v.as_f64()) {
        if sz > 0.0 { return Some(sz); }
    }
    if let Some(sz) = f.get("filesize_approx").and_then(|v| v.as_f64()) {
        if sz > 0.0 { return Some(sz); }
    }
    if let Some(tbr) = f.get("tbr").and_then(|v| v.as_f64()) {
        if tbr > 0.0 && dur > 0.0 { return Some(tbr * 1000.0 / 8.0 * dur); }
    }
    None
}

fn best_audio_bytes(formats: &[serde_json::Value], dur: f64) -> Option<f64> {
    formats.iter()
        .filter(|f| f.get("acodec").and_then(|v| v.as_str()).map(|c| c != "none").unwrap_or(false)
            && f.get("vcodec").and_then(|v| v.as_str()).map(|c| c == "none").unwrap_or(true))
        .filter_map(|f| fmt_bytes(f, dur))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

fn estimate_sizes(info: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    let dur = info.get("duration").and_then(|d| d.as_f64()).unwrap_or(0.0);
    let empty = Vec::new();
    let formats = info.get("formats").and_then(|f| f.as_array()).unwrap_or(&empty);
    if formats.is_empty() || dur <= 0.0 { return out; }
    let audio = best_audio_bytes(formats, dur);
    for (tier, max_h) in [("1080p", 1080.0f64), ("720p", 720.0), ("480p", 480.0), ("360p", 360.0)] {
        // video-only candidates that yt-dlp's bestvideo[height<=H] would pick
        let video: Option<f64> = formats.iter()
            .filter(|f| f.get("vcodec").and_then(|v| v.as_str()).map(|c| c != "none").unwrap_or(false))
            .filter(|f| f.get("height").and_then(|h| h.as_f64()).map(|h| h <= max_h && h > 0.0).unwrap_or(false))
            .filter_map(|f| fmt_bytes(f, dur).map(|b| (b, f.get("height").and_then(|h| h.as_f64()).unwrap_or(0.0))))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)))
            .map(|(b, _)| b);
        // fallback: progressive format (video+audio in one)
        let combined: Option<f64> = formats.iter()
            .filter(|f| f.get("vcodec").and_then(|v| v.as_str()).map(|c| c != "none").unwrap_or(false))
            .filter(|f| f.get("acodec").and_then(|v| v.as_str()).map(|c| c != "none").unwrap_or(false))
            .filter(|f| f.get("height").and_then(|h| h.as_f64()).map(|h| h <= max_h && h > 0.0).unwrap_or(false))
            .filter_map(|f| fmt_bytes(f, dur))
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let total = match (video, audio, combined) {
            (Some(v), Some(a), _) => Some(v + a),
            (_, _, Some(c)) => Some(c),
            _ => None,
        };
        if let Some(bytes) = total {
            out.insert(tier.to_string(), serde_json::Value::String(fmt_size(bytes)));
        }
    }
    if let Some(a) = audio {
        out.insert("audio".to_string(), serde_json::Value::String(fmt_size(a)));
    }
    out
}

#[tauri::command]
async fn fetch_video_info(url: String) -> Result<String, String> {
    let bin = ytdlp_bin();
    let ffmpeg = ffmpeg_dir();
    tauri::async_runtime::spawn_blocking(move || {
        let mut base: Vec<String> = vec![
            "--dump-single-json".to_string(),
            "--no-playlist".to_string(),
            "--no-warnings".to_string(),
            "--skip-download".to_string(),
        ];
        if let Some(dir) = ffmpeg {
            base.push("--ffmpeg-location".to_string());
            base.push(dir);
        }
        base.push(url);
        let out = run_ytdlp(&bin, &base)?;
        let mut v: serde_json::Value = serde_json::from_str(&out).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = v.as_object_mut() {
            let sizes = estimate_sizes(&obj.clone().into());
            obj.insert("hyper_sizes".to_string(), serde_json::Value::Object(sizes));
        }
        Ok(v.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn settings_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| std::env::temp_dir()).join("hyper").join("settings.json")
}

#[tauri::command]
fn get_settings() -> serde_json::Value {
    std::fs::read_to_string(settings_path()).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

#[tauri::command]
fn set_settings(settings: serde_json::Value) -> Result<(), String> {
    let p = settings_path();
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let s = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| e.to_string())
}

// Resolve the ffmpeg/ffprobe executable path (downloaded -> bundled -> system PATH)
fn resolve_tool(tool: &str, ffmpeg_override: &Option<String>) -> String {
    let exe = if cfg!(windows) { format!("{}.exe", tool) } else { tool.to_string() };
    let found = ffmpeg_override.clone().or_else(|| find_bundled_bin(tool).map(|p| p.to_string_lossy().to_string()));
    match found {
        Some(f) if std::path::Path::new(&f).is_dir() => std::path::Path::new(&f).join(&exe).to_string_lossy().to_string(),
        Some(f) if std::path::Path::new(&f).exists() => f,
        _ => exe,
    }
}

fn embed_url_metadata(video: &std::path::Path, url: &str, ffmpeg_exe: &str) -> Result<(), String> {
    let ext_lc = video.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let stem = video.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "tmp".to_string());
    let ext = video.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_else(|| "mp4".to_string());
    let tmp = video.with_file_name(format!("{}.hyper_tmp.{}", stem, ext));
    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-loglevel".to_string(),
        "quiet".to_string(),
        "-i".to_string(),
        video.to_string_lossy().to_string(),
        "-map".to_string(),
        "0".to_string(),
        "-c".to_string(),
        "copy".to_string(),
        "-metadata".to_string(),
        format!("comment={}", url),
        "-metadata".to_string(),
        format!("purl={}", url),
    ];
    if ["mp4", "m4a", "mov"].contains(&ext_lc.as_str()) {
        args.push("-movflags".to_string());
        args.push("+faststart".to_string());
    }
    args.push(tmp.to_string_lossy().to_string());
    let out = hyper_cmd(ffmpeg_exe).args(&args).output().map_err(|e| e.to_string())?;
    if out.status.success() && tmp.exists() {
        let _ = std::fs::remove_file(video);
        std::fs::rename(&tmp, video).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        let _ = std::fs::remove_file(&tmp);
        Err(format!("metadata embed failed: {}", String::from_utf8_lossy(&out.stderr)))
    }
}

#[tauri::command]
async fn download_video(window: tauri::Window, url: String, quality: String, save_path: String, format_override: Option<String>, max_bytes: Option<f64>) -> Result<String, String> {
    let path = PathBuf::from(&save_path);
    let _ = std::fs::create_dir_all(&path);
    let format = format_override.unwrap_or_else(|| match quality.as_str() {
        "1080p" => "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best".to_string(),
        "720p" => "bestvideo[height<=720]+bestaudio/best[height<=720]/best".to_string(),
        "480p" => "bestvideo[height<=480]+bestaudio/best[height<=480]/best".to_string(),
        "360p" => "bestvideo[height<=360]+bestaudio/best[height<=360]/best".to_string(),
        "audio" => "bestaudio/best".to_string(),
        _ => "best".to_string(),
    });
    let bin = ytdlp_bin();
    let ffmpeg = ffmpeg_dir();
    let url_for_sidecar = url.clone();
    tauri::async_runtime::spawn_blocking(move || {
    let mut args: Vec<String> = vec![
        "-f".to_string(), format.to_string(),
        "--no-playlist".to_string(),
        "--merge-output-format".to_string(), "mp4".to_string(),
        "-o".to_string(), format!("{}/%(title)s.%(ext)s", path.to_string_lossy()),
        "--embed-thumbnail".to_string(),
        "--embed-metadata".to_string(),
        "--newline".to_string(),
        "--progress".to_string(),
    ];
    if let Some(ref dir) = ffmpeg {
        args.push("--ffmpeg-location".to_string());
        args.push(dir.clone());
    }
    if quality == "audio" {
        args.extend(["--extract-audio".to_string(), "--audio-format".to_string(), "mp3".to_string(), "--audio-quality".to_string(), "0".to_string()].map(|s| s.to_string()));
    }
    // Build retry variants like run_ytdlp but with progress
    let mut extra_variants: Vec<(String, Vec<String>)> = Vec::new();
    extra_variants.push(("default".to_string(), vec![]));
    extra_variants.push(("clients".to_string(), vec!["--extractor-args".to_string(), "youtube:player_client=tv,web_safari,android_vr,mweb".to_string()]));
    for browser in ["brave", "chrome", "chromium", "firefox", "edge", "opera", "vivaldi", "whale"] {
        extra_variants.push((format!("cookies:{}", browser), vec!["--cookies-from-browser".to_string(), browser.to_string()]));
    }
    let mut ordered: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(s) = saved_strategy() {
        if let Some(pos) = extra_variants.iter().position(|(name, _)| *name == s) {
            ordered.push(extra_variants.remove(pos));
        }
    }
    ordered.extend(extra_variants);
    let mut last_err = String::new();
    let mut success = false;
    let win = window.clone();
    for (name, extra) in ordered {
        let mut full_args = args.clone();
        full_args.extend(extra.clone());
        full_args.push(url.clone());
        let mut child = match hyper_cmd(&bin).args(&full_args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
            Ok(c) => c,
            Err(e) => { last_err = e.to_string(); continue; }
        };
        // Read stderr for progress (yt-dlp uses stderr for progress with --newline)
        let stderr = child.stderr.take();
        let stdout = child.stdout.take();
        // Spawn thread to drain stdout (prevent blocking)
        if let Some(out) = stdout {
            std::thread::spawn(move || {
                let mut r = BufReader::new(out);
                let mut buf = String::new();
                while let Ok(n) = r.read_line(&mut buf) {
                    if n == 0 { break; }
                    buf.clear();
                }
            });
        }
        let mut progress_emitted = false;
        if let Some(err) = stderr {
            let reader = BufReader::new(err);
            for line in reader.lines() {
                if let Ok(l) = line {
                    // Parse percent like " 12.3%" or "[download] 12.3%"
                    if let Some(pct_idx) = l.find('%') {
                        // Find start of number before %
                        let mut start = pct_idx;
                        while start > 0 && (l.as_bytes()[start-1].is_ascii_digit() || l.as_bytes()[start-1] == b'.') {
                            start -= 1;
                        }
                        if let Ok(pct) = l[start..pct_idx].trim().parse::<f64>() {
                            let _ = win.emit("download-progress", pct);
                            progress_emitted = true;
                        }
                    }
                    // Also emit raw line for debugging if needed
                }
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if status.success() {
            save_strategy(&name);
            success = true;
            break;
        } else {
            // Try to get stderr for error (we already consumed it, so we need to capture it differently)
            // For simplicity, read from a temp file or just use a generic error
            // Instead, we re-run without progress to get error string? Simpler: assume bot error and continue
            // We'll try to read the error by running a quick check without progress
            // For now, treat as bot error and continue to next variant if progress was emitted (meaning it was downloading)
            // If no progress was emitted, it's likely a bot/auth error, try next
            last_err = format!("yt-dlp failed for {}", name);
            if !progress_emitted {
                // Likely bot error, try next variant
                continue;
            } else {
                // Progress was shown but still failed (maybe network), return error
                return Err(last_err);
            }
        }
    }
    if !success {
        return Err(last_err);
    }
    let _ = win.emit("download-progress", 94.0);
        // Smart Quality max-size mode: if actual file exceeds the user's limit, compress via two-pass encode
        if let Some(limit) = max_bytes {
            let limit = limit as u64;
            let ffmpeg_exe = resolve_tool("ffmpeg", &ffmpeg);
            let ffprobe_exe = resolve_tool("ffprobe", &ffmpeg);
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut videos: Vec<_> = entries.flatten().filter(|e| {
                    let p = e.path();
                    p.is_file() && ["mp4", "mkv", "webm", "mov", "avi"].iter().any(|ext| p.extension().map(|ex| ex.to_string_lossy().to_lowercase() == *ext).unwrap_or(false))
                }).collect();
                videos.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));
                if let Some(entry) = videos.pop() {
                    let video = entry.path();
                    if let Ok(meta) = std::fs::metadata(&video) {
                        if meta.len() > limit {
                            let dur: f64 = hyper_cmd(&ffprobe_exe)
                                .args(["-v", "quiet", "-show_entries", "format=duration", "-of", "csv=p=0", &video.to_string_lossy()])
                                .output().ok()
                                .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
                                .unwrap_or(0.0);
                            if dur > 0.0 {
                                let tmp_out = video.with_extension("sq.tmp.mp4");
                                let pass_log = dirs::cache_dir().unwrap_or_else(|| std::env::temp_dir()).join("hyper").join("sq_pass");
                                let _ = std::fs::create_dir_all(&pass_log);
                                let pass_log = pass_log.join("pass");
                                // Up to 2 attempts: estimation inaccuracies get corrected on the retry
                                for attempt in 0..2 {
                                    let target_bytes: f64 = (limit as f64)
                                        * if attempt == 0 { 0.95 } else { 0.90 };
                                    let mut video_kbps = (target_bytes * 8.0 / 1000.0 / dur) - 128.0;
                                    if video_kbps < 40.0 { video_kbps = 40.0; }
                                    if video_kbps > 20000.0 { video_kbps = 20000.0; }
                                    let vb = format!("{}k", video_kbps as u32);
                                    // Adaptive resolution: squeeze harder only when bitrate gets too low to look OK
                                    let scale = if video_kbps < 350.0 { Some("scale=-2:min(480\\,ih)") }
                                        else if video_kbps < 800.0 { Some("scale=-2:min(720\\,ih)") }
                                        else { None };
                                    let mut p1 = vec!["-y".to_string(), "-loglevel".to_string(), "quiet".to_string(), "-i".to_string(), video.to_string_lossy().to_string()];
                                    if let Some(s) = scale { p1.extend(["-vf".to_string(), s.to_string()]); }
                                    p1.extend(["-c:v".to_string(), "libx264".to_string(), "-b:v".to_string(), vb.clone(), "-pass".to_string(), "1".to_string(), "-passlogfile".to_string(), pass_log.to_string_lossy().to_string(), "-an".to_string(), "-f".to_string(), "mp4".to_string(), tmp_out.to_string_lossy().to_string()]);
                                    let _ = hyper_cmd(&ffmpeg_exe).args(&p1).output();
                                    let mut p2 = vec!["-y".to_string(), "-loglevel".to_string(), "quiet".to_string(), "-i".to_string(), video.to_string_lossy().to_string()];
                                    if let Some(s) = scale { p2.extend(["-vf".to_string(), s.to_string()]); }
                                    p2.extend(["-c:v".to_string(), "libx264".to_string(), "-b:v".to_string(), vb.clone(), "-pass".to_string(), "2".to_string(), "-passlogfile".to_string(), pass_log.to_string_lossy().to_string(), "-c:a".to_string(), "aac".to_string(), "-b:a".to_string(), "128k".to_string(), "-movflags".to_string(), "+faststart".to_string(), tmp_out.to_string_lossy().to_string()]);
                                    let enc = hyper_cmd(&ffmpeg_exe).args(&p2).output();
                                    let out_size = std::fs::metadata(&tmp_out).map(|m| m.len()).unwrap_or(u64::MAX);
                                    if enc.is_ok() && out_size <= limit {
                                        let _ = std::fs::remove_file(&video);
                                        let _ = std::fs::rename(&tmp_out, &video);
                                        break;
                                    }
                                    let _ = std::fs::remove_file(&tmp_out);
                                }
                                for ext in ["-0.log", "-0.log.mbtree", "-0.mkv"] {
                                    let _ = std::fs::remove_file(format!("{}{}", pass_log.to_string_lossy(), ext));
                                }
                            }
                        }
                    }
                }
            }
        }
        // Embed original URL into MP4 metadata without re-encoding (no sidecar .txt)
        {
            let ffmpeg_exe = resolve_tool("ffmpeg", &ffmpeg);
            if let Ok(entries) = std::fs::read_dir(&path) {
                let mut videos: Vec<_> = entries.flatten().filter(|e| {
                    let p = e.path();
                    p.is_file() && ["mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a", "opus"].iter().any(|ext| p.extension().map(|ex| ex.to_string_lossy().to_lowercase() == *ext).unwrap_or(false))
                }).collect();
                videos.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));
                if let Some(entry) = videos.pop() {
                    let video = entry.path();
                    if let Err(e) = embed_url_metadata(&video, &url_for_sidecar, &ffmpeg_exe) {
                        eprintln!("[Hyper] metadata embed failed for {:?}: {}", video, e);
                    } else {
                        let _ = win.emit("download-progress", 96.0);
                    }
                }
            }
        }
        // Generate thumbnail cache from video frame (hidden, not in HyperDownloads)
        let cache_dir = dirs::cache_dir().unwrap_or_else(|| std::env::temp_dir()).join("hyper").join("thumbs");
        let _ = std::fs::create_dir_all(&cache_dir);
        if let Ok(entries) = std::fs::read_dir(&path) {
            let mut videos: Vec<_> = entries.flatten().filter(|e| {
                let p = e.path();
                p.is_file() && ["mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a"].iter().any(|ext| p.extension().map(|ex| ex.to_string_lossy().to_lowercase() == *ext).unwrap_or(false))
            }).collect();
            videos.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));
            if let Some(entry) = videos.last() {
                if let Some(stem) = entry.path().file_stem().map(|s| s.to_string_lossy().to_string()) {
                    let thumb_path = cache_dir.join(format!("{}.jpg", stem));
                    if !thumb_path.exists() {
                        let ffmpeg_bin = ffmpeg.clone().unwrap_or_else(|| "ffmpeg".to_string());
                        let ffmpeg_exe = if std::path::Path::new(&ffmpeg_bin).is_dir() {
                            std::path::Path::new(&ffmpeg_bin).join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" }).to_string_lossy().to_string()
                        } else if std::path::Path::new(&ffmpeg_bin).exists() {
                            ffmpeg_bin
                        } else {
                            "ffmpeg".to_string()
                        };
                        let _ = hyper_cmd(&ffmpeg_exe).args(["-y", "-loglevel", "quiet", "-i", &entry.path().to_string_lossy(), "-ss", "00:00:01", "-vframes", "1", "-q:v", "2", &thumb_path.to_string_lossy()]).output();
                    }
                }
            }
        }
        Ok("downloaded".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn list_downloads() -> Result<Vec<serde_json::Value>, String> {
    let base = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("HyperDownloads");
    let _ = std::fs::create_dir_all(&dir);
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // skip .ffmpeg hidden dir
            if path.file_name().map(|n| n.to_string_lossy().starts_with('.')).unwrap_or(false) {
                continue;
            }
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() { continue; }
        let name = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Unknown".to_string());
        let ext = path.extension().map(|e| e.to_string_lossy().to_string().to_uppercase()).unwrap_or_default();
        let size = meta.len();
        let size_str = if size >= 1024*1024*1024 {
            format!("{:.2} GB", size as f64 / 1024.0/1024.0/1024.0)
        } else if size >= 1024*1024 {
            format!("{} MB", (size / 1024/1024))
        } else if size >= 1024 {
            format!("{} KB", (size / 1024))
        } else {
            format!("{} B", size)
        };
        let modified = meta.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                let secs = d.as_secs();
                // simple relative date
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or(d).as_secs();
                let diff = now.saturating_sub(secs);
                if diff < 86400 { "Today".to_string() }
                else if diff < 172800 { "Yesterday".to_string() }
                else { format!("{} days ago", diff/86400) }
            }).unwrap_or_else(|| "Unknown".to_string());
        // skip thumbnail / metadata / sidecar files themselves
        let ext_lc = ext.to_lowercase();
        if ["jpg", "jpeg", "png", "webp", "json", "txt"].contains(&ext_lc.as_str()) {
            let stem = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            // sidecar .url.txt has stem like "video.url" -> real stem is "video"
            let real_stem = if path.file_name().unwrap_or_default().to_string_lossy().ends_with(".url.txt") {
                stem.trim_end_matches(".url").to_string()
            } else { stem.clone() };
            let has_video = ["mp4", "mkv", "webm", "mov", "avi", "mp3", "m4a", "opus"].iter().any(|e| dir.join(format!("{}.{}", real_stem, e)).exists() || dir.join(format!("{}.{}", real_stem, e.to_uppercase())).exists());
            if has_video || path.file_name().unwrap_or_default().to_string_lossy().ends_with(".url.txt") {
                continue;
            }
        }
        let id = path.to_string_lossy().to_string().len() as i64 + size as i64;
        let stem = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        // retrieve original URL: sidecar first, then embedded metadata via ffprobe
        let url_str = {
            let sidecar = dir.join(format!("{}.url.txt", stem));
            if let Ok(s) = std::fs::read_to_string(&sidecar) {
                s.trim().to_string()
            } else {
                // fallback: try to extract from embedded metadata (purl/comment)
                let ffprobe_bin = resolve_tool("ffprobe", &None);
                if let Ok(out) = hyper_cmd(&ffprobe_bin).args(["-v", "quiet", "-print_format", "json", "-show_entries", "format_tags=comment,description,purl,URL,url", &path.to_string_lossy()]).output() {
                    if out.status.success() {
                        let txt = String::from_utf8_lossy(&out.stdout);
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                            if let Some(tags) = v.get("format").and_then(|f| f.get("tags")) {
                                let mut found = String::new();
                                for key in ["purl", "PURL", "comment", "description", "URL", "url"] {
                                    if let Some(val) = tags.get(key).and_then(|x| x.as_str()) {
                                        if val.contains("http") {
                                            if let Some(start) = val.find("http") {
                                                let slice = &val[start..];
                                                let end = slice.find(|c: char| c.is_whitespace() || c=='"' || c=='\'' || c==',' || c=='}').unwrap_or(slice.len());
                                                found = slice[..end].trim().to_string();
                                                break;
                                            }
                                        }
                                    }
                                }
                                found
                            } else { String::new() }
                        } else { String::new() }
                    } else { String::new() }
                } else { String::new() }
            }
        };
        let cache_dir = dirs::cache_dir().unwrap_or_else(|| std::env::temp_dir()).join("hyper").join("thumbs");
        let _ = std::fs::create_dir_all(&cache_dir);
        let thumb_path = cache_dir.join(format!("{}.jpg", stem));
        // Lazily extract: embedded cover art first, fallback to 1s frame (small 320w for data URL)
        if !thumb_path.exists() && ["mp4", "mkv", "webm", "mov", "avi"].contains(&ext_lc.as_str()) {
            let ffmpeg_exe = {
                let found = find_bundled_bin("ffmpeg").map(|p| p.to_string_lossy().to_string())
                    .or_else(|| downloaded_ffmpeg_bin().map(|p| p.to_string_lossy().to_string()));
                found.unwrap_or_else(|| bundled_bin_name("ffmpeg"))
            };
            // 1. embedded cover art (stream after main video)
            let _ = hyper_cmd(&ffmpeg_exe).args(["-y", "-loglevel", "quiet", "-i", &path.to_string_lossy(), "-map", "0:v:1", "-frames:v", "1", "-q:v", "5", "-vf", "scale=320:-2", &thumb_path.to_string_lossy()]).output();
            // 2. fallback: frame at 1s
            if !thumb_path.exists() {
                let _ = hyper_cmd(&ffmpeg_exe).args(["-y", "-loglevel", "quiet", "-i", &path.to_string_lossy(), "-ss", "00:00:01", "-vframes", "1", "-q:v", "5", "-vf", "scale=320:-2", &thumb_path.to_string_lossy()]).output();
            }
        }
        // Embed as data URL - asset protocol can be unreliable in WebKit
        let thumb_str = std::fs::read(&thumb_path).ok()
            .map(|bytes| format!("data:image/jpeg;base64,{}", use_base64(&bytes)))
            .unwrap_or_default();
        out.push(serde_json::json!({
            "id": id,
            "title": name,
            "platform": "Local",
            "thumb": thumb_str,
            "duration": "",
            "meta": ext,
            "size": size_str,
            "date": modified,
            "quality": ext,
            "path": path.to_string_lossy().to_string(),
            "url": url_str
        }));
    }
    // sort by modified desc (newest first) - sort by id desc as proxy
    out.sort_by(|a,b| b["id"].as_i64().unwrap_or(0).cmp(&a["id"].as_i64().unwrap_or(0)));
    Ok(out)
}

#[tauri::command]
fn open_downloads_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    { hyper_cmd("explorer").arg(&path).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "linux")]
    { hyper_cmd("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?; }
    #[cfg(target_os = "macos")]
    { hyper_cmd("open").arg(&path).spawn().map_err(|e| e.to_string())?; }
    Ok(())
}

#[tauri::command]
fn delete_download(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() { return Err("File not found".into()); }
    std::fs::remove_file(&p).map_err(|e| e.to_string())?;
    let stem = p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
    if let (Some(parent), Some(stem)) = (p.parent(), &stem) {
        let _ = std::fs::remove_file(parent.join(format!("{}.url.txt", stem)));
    }
    if let (Some(cache_dir), Some(stem)) = (dirs::cache_dir(), &stem) {
        let _ = std::fs::remove_file(cache_dir.join("hyper").join("thumbs").join(format!("{}.jpg", stem)));
    }
    Ok(())
}

#[tauri::command]
fn close_window(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}
#[tauri::command]
fn minimize_window(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(|e| e.to_string())
}
#[tauri::command]
fn start_dragging(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}
#[tauri::command]
fn resize_window(window: tauri::Window, width: f64, height: f64) -> Result<(), String> {
    eprintln!("[Hyper] resize_window -> {}x{} (was {:?})", width, height, window.inner_size());
    window.set_size(tauri::LogicalSize::new(width, height)).map_err(|e| e.to_string())
}
#[tauri::command]
fn animate_resize(window: tauri::Window, width: f64, height: f64) -> Result<(), String> {
    eprintln!("[Hyper] animate_resize -> {}x{} (cur: {:?})", width, height, window.inner_size());
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let cur = window.inner_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale).height;
    let diff = height - cur;
    if diff.abs() < 1.0 {
        return window.set_size(tauri::LogicalSize::new(width, height)).map_err(|e| e.to_string());
    }
    let w = window.clone();
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let duration_ms = 380.0;
        loop {
            let elapsed = start.elapsed().as_millis() as f64;
            let p = (elapsed / duration_ms).min(1.0);
            let eased = if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powf(3.0) / 2.0 };
            let h = cur + diff * eased;
            let _ = w.set_size(tauri::LogicalSize::new(width, h));
            if p >= 1.0 { break; }
            std::thread::sleep(std::time::Duration::from_millis(7));
        }
        let _ = w.set_size(tauri::LogicalSize::new(width, height));
        eprintln!("[Hyper] animate_resize done -> {}x{}", width, height);
    });
    Ok(())
}

// ---------- Accent-aware app icon ----------
const ICON_PTS: [(f64, f64); 6] = [(13.0, 2.0), (3.0, 14.0), (10.0, 14.0), (9.0, 22.0), (19.0, 10.0), (12.0, 10.0)];

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 { return None; }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

fn inside_rounded(x: f64, y: f64, s: f64, r: f64) -> bool {
    let cx = x.clamp(r, s - r);
    let cy = y.clamp(r, s - r);
    (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r
}

fn point_in_poly(px: f64, py: f64, pts: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn bolt_polygon(size: f64) -> Vec<(f64, f64)> {
    let w = 19.0 - 3.0;
    let h = 22.0 - 2.0;
    let scale = (size * 0.55 / w).min(size * 0.55 / h);
    let (cx, cy) = (size / 2.0, size / 2.0);
    ICON_PTS.iter().map(|&(x, y)| ((x - 12.0) * scale + cx, (y - 12.0) * scale + cy)).collect()
}

fn scale_about(pts: &[(f64, f64)], k: f64) -> Vec<(f64, f64)> {
    let cx: f64 = pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64;
    let cy: f64 = pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64;
    pts.iter().map(|&(x, y)| (cx + (x - cx) * k, cy + (y - cy) * k)).collect()
}

fn translate(pts: &[(f64, f64)], dx: f64, dy: f64) -> Vec<(f64, f64)> {
    pts.iter().map(|&(x, y)| (x + dx, y + dy)).collect()
}

pub fn render_accent_icon(hex: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    use std::io::Cursor as IoCursor;
    let (ar, ag, ab) = parse_hex_color(hex).ok_or_else(|| format!("bad hex: {}", hex))?;
    let ss: u32 = 4;
    let out: u32 = 512;
    let big = (out * ss) as f64;
    let corner = big * 0.22;
    let bg = [14u8, 14u8, 16u8];
    let bolt = bolt_polygon(big);
    let halo = scale_about(&bolt, 1.055);
    let shadow = translate(&bolt, big * 0.015, big * 0.015);
    let lighten = |c: u8| (c as f64 + (255.0 - c as f64) * 0.45).round() as u8;
    let (hr, hg, hb) = (lighten(ar), lighten(ag), lighten(ab));
    let mut rgba = vec![0u8; (out * out * 4) as usize];
    for fy in 0..out {
        for fx in 0..out {
            let (mut r, mut g, mut b, mut a): (f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0);
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (fx * ss + sx) as f64 + 0.5;
                    let y = (fy * ss + sy) as f64 + 0.5;
                    let (mut sr, mut sg, mut sb, mut sa) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
                    if inside_rounded(x, y, big, corner) {
                        sr = bg[0] as f64; sg = bg[1] as f64; sb = bg[2] as f64; sa = 255.0;
                        if point_in_poly(x, y, &shadow) {
                            let alpha = 80.0 / 255.0;
                            sr = sr * (1.0 - alpha); sg = sg * (1.0 - alpha); sb = sb * (1.0 - alpha);
                        }
                        if point_in_poly(x, y, &halo) {
                            sr = hr as f64; sg = hg as f64; sb = hb as f64;
                        }
                        if point_in_poly(x, y, &bolt) {
                            sr = ar as f64; sg = ag as f64; sb = ab as f64;
                        }
                    }
                    r += sr; g += sg; b += sb; a += sa;
                }
            }
            let n = (ss * ss) as f64;
            let i = ((fy * out + fx) * 4) as usize;
            rgba[i] = (r / n).round() as u8;
            rgba[i + 1] = (g / n).round() as u8;
            rgba[i + 2] = (b / n).round() as u8;
            rgba[i + 3] = (a / n).round() as u8;
        }
    }
    let img = image::RgbaImage::from_raw(out, out, rgba.clone()).ok_or("icon buffer")?;
    let mut png = IoCursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok((png.into_inner(), rgba))
}

fn apply_accent_icon(window: &tauri::Window, hex: &str) -> Result<(), String> {
    let (png, raw) = render_accent_icon(hex)?;
    let cache_dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("hyper");
    let _ = std::fs::create_dir_all(&cache_dir);
    let icon_path = cache_dir.join("icon.png");
    let _ = std::fs::write(&icon_path, &png);
    if let Some(dd) = dirs::data_dir() {
        let hi = dd.join("icons").join("hicolor").join("512x512").join("apps");
        if hi.exists() || std::fs::create_dir_all(&hi).is_ok() {
            let _ = std::fs::write(hi.join("hyper.png"), &png);
        }
    }
    if let (Some(home), Some(path_str)) = (dirs::home_dir(), icon_path.to_str()) {
        let de = home.join(".local/share/applications/hyper.desktop");
        if let Ok(txt) = std::fs::read_to_string(&de) {
            let updated: String = txt
                .lines()
                .map(|l| if l.starts_with("Icon=") { format!("Icon={}", path_str) } else { l.to_string() })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write(&de, updated);
        }
    }
    for ks in ["kbuildsycoca6", "kbuildsycoca5"] {
        if hyper_cmd(ks)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            break;
        }
    }
    let icon = tauri::image::Image::new_owned(raw, 512, 512);
    window.set_icon(icon).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_accent_icon(window: tauri::Window, hex: String) -> Result<(), String> {
    apply_accent_icon(&window, &hex)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = get_default_save_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            greet, get_default_save_path, ensure_hyper_downloads, get_bundled_status, ensure_ffmpeg, fetch_video_info, download_video, list_downloads, open_downloads_folder, delete_download, close_window, minimize_window, start_dragging, resize_window, animate_resize, get_settings, set_settings, set_accent_icon
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_accent_icons() {
        for hex in ["#38bdf8", "#ef4444", "#22c55e", "#a855f7"] {
            let (png, rgba) = super::render_accent_icon(hex).unwrap();
            assert!(png.len() > 1000);
            assert_eq!(rgba.len(), 512 * 512 * 4);
            assert!(rgba[(256 * 512 + 256) * 4 + 3] > 0, "center pixel should be opaque");
        }
    }
}
