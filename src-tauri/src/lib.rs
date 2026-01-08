// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

#[cfg(target_os = "windows")]
use encoding_rs::EUC_KR;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[derive(Serialize)]
struct CopilotResult {
    output: String,
    temp_path: Option<String>,
    context_path: Option<String>,
}

#[derive(Serialize)]
struct CopilotStatus {
    installed: bool,
    version: Option<String>,
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunCopilotArgs {
    prompt: String,
    model: String,
    context_path: Option<String>,
}

#[tauri::command]
async fn run_copilot(args: RunCopilotArgs) -> Result<CopilotResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let token = resolve_github_token();
        let mut command = copilot_command();
        let mut temp_path: Option<String> = None;
        let context_path_for_debug = args.context_path.clone();
        let full_prompt = match args.context_path.as_ref() {
            Some(path) if !path.trim().is_empty() => {
                let path_buf = PathBuf::from(path);
                let file_name = path_buf
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path);
                let temp_root = env::temp_dir().join("ghc");
                fs::create_dir_all(&temp_root)
                    .map_err(|err| format!("Failed to create temp dir: {err}"))?;
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|err| format!("Failed to generate temp name: {err}"))?
                    .as_millis();
                let temp_name = format!(".copilot-context-{stamp}-{file_name}");
                let temp_file = temp_root.join(&temp_name);
                fs::copy(&path_buf, &temp_file)
                    .map_err(|err| format!("Failed to copy context file: {err}"))?;
                temp_path = Some(temp_file.display().to_string());
                format!("{} {}", args.prompt, temp_file.display())
            }
            _ => args.prompt,
        };
        command
            .arg("-s")
            .arg("-p")
            .arg(full_prompt)
            .arg("--model")
            .arg(args.model);
        if let Some(token) = token {
            command.env("GITHUB_TOKEN", token);
        }

        let temp_path_for_result = temp_path.clone();
        let output = match command.output() {
            Ok(output) => output,
            Err(err) => {
                if let Some(ref path) = temp_path {
                    let _ = fs::remove_file(path);
                }
                return Err(format!("Failed to run copilot: {err}"));
            }
        };
        if let Some(ref path) = temp_path {
            let _ = fs::remove_file(path);
        }

        if output.status.success() {
            Ok(CopilotResult {
                output: String::from_utf8_lossy(&output.stdout).trim().to_string(),
                temp_path: temp_path_for_result,
                context_path: context_path_for_debug,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(stderr.trim().to_string())
        }
    })
    .await
    .map_err(|err| format!("Failed to run copilot: {err}"))?
}

#[tauri::command]
async fn get_copilot_version() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let output = copilot_command()
            .arg("--version")
            .output()
            .map_err(|err| format!("Failed to run copilot: {err}"))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(stderr.trim().to_string())
        }
    })
    .await
    .map_err(|err| format!("Failed to run copilot: {err}"))?
}

#[tauri::command]
async fn get_copilot_status() -> Result<CopilotStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Ok(output) = copilot_command().arg("--version").output() {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let path = resolve_copilot_path().map(|path| path.display().to_string());
                return Ok(CopilotStatus {
                    installed: true,
                    version: Some(version),
                    path,
                });
            }
        }

        let path = resolve_copilot_path().map(|path| path.display().to_string());
        Ok(CopilotStatus {
            installed: path.is_some(),
            version: None,
            path,
        })
    })
    .await
    .map_err(|err| format!("Failed to check copilot: {err}"))?
}

#[tauri::command]
async fn get_copilot_where_log() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("where");
            cmd.arg("copilot");
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
            let output = cmd
                .output()
                .map_err(|err| format!("Failed to run where: {err}"))?;
            let mut log = String::new();
            if !output.stdout.is_empty() {
                log.push_str("STDOUT:\n");
                log.push_str(&decode_platform_bytes(&output.stdout));
            }
            if !output.stderr.is_empty() {
                if !log.is_empty() {
                    log.push('\n');
                }
                log.push_str("STDERR:\n");
                log.push_str(&decode_platform_bytes(&output.stderr));
            }
            if log.is_empty() {
                log.push_str("No output from where copilot");
            }
            return Ok(log.trim_end().to_string());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let output = std::process::Command::new("which")
                .arg("copilot")
                .output()
                .map_err(|err| format!("Failed to run which: {err}"))?;
            let mut log = String::new();
            if !output.stdout.is_empty() {
                log.push_str("STDOUT:\n");
                log.push_str(&decode_platform_bytes(&output.stdout));
            }
            if !output.stderr.is_empty() {
                if !log.is_empty() {
                    log.push('\n');
                }
                log.push_str("STDERR:\n");
                log.push_str(&decode_platform_bytes(&output.stderr));
            }
            if log.is_empty() {
                log.push_str("No output from which copilot");
            }
            return Ok(log.trim_end().to_string());
        }
    })
    .await
    .map_err(|err| format!("Failed to get copilot where log: {err}"))?
}

#[tauri::command]
async fn install_copilot_cli() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if cfg!(target_os = "macos") {
            if !command_available("brew", &["--version"]) {
                return Err("Homebrew not found. Install it from https://brew.sh.".to_string());
            }
            let output = std::process::Command::new("brew")
                .arg("install")
                .arg("copilot-cli")
                .output()
                .map_err(|err| format!("Failed to run brew: {err}"))?;
            if output.status.success() {
                return Ok("Copilot CLI installed via Homebrew.".to_string());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(stderr.trim().to_string());
        }

        if cfg!(target_os = "windows") {
            if !command_available("winget", &["--version"]) {
                return Err(
                    "winget not found. Install App Installer from the Microsoft Store.".to_string(),
                );
            }
            let mut cmd = std::process::Command::new("cmd");
            cmd.args([
                "/C",
                "start",
                "",
                "/B",
                "winget",
                "install",
                "GitHub.Copilot",
            ]);
            #[cfg(target_os = "windows")]
            {
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::null());
            cmd.stderr(Stdio::null());

            cmd.spawn()
                .map_err(|err| format!("Failed to launch winget install: {err}"))?;

            return Ok("Copilot CLI install started via winget (background).".to_string());
        }

        Err("Copilot install is only supported on macOS and Windows.".to_string())
    })
    .await
    .map_err(|err| format!("Failed to install copilot: {err}"))?
}

#[tauri::command]
fn has_github_token() -> bool {
    resolve_github_token().is_some()
}

#[tauri::command]
fn clear_github_token() -> Result<(), String> {
    env::remove_var("GITHUB_TOKEN");
    let env_path = env_path()?;
    let contents = fs::read_to_string(&env_path).unwrap_or_default();
    let updated: String = contents
        .lines()
        .filter(|line| !line.trim_start().starts_with("GITHUB_TOKEN="))
        .map(|line| format!("{line}\n"))
        .collect();
    if updated != contents {
        fs::write(&env_path, updated).map_err(|err| format!("Failed to write ~/.env: {err}"))?;
    }
    Ok(())
}

#[derive(Serialize)]
struct TokenStatus {
    has_token: bool,
    tail: Option<String>,
}

#[tauri::command]
fn get_token_status() -> TokenStatus {
    if let Some(token) = resolve_github_token() {
        let tail = token
            .chars()
            .rev()
            .take(3)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        return TokenStatus {
            has_token: true,
            tail: Some(tail),
        };
    }
    TokenStatus {
        has_token: false,
        tail: None,
    }
}

fn command_available(command: &str, args: &[&str]) -> bool {
    std::process::Command::new(command)
        .args(args)
        .output()
        .is_ok()
}

#[cfg(target_os = "windows")]
fn decode_platform_bytes(bytes: &[u8]) -> String {
    let (cow, _, _) = EUC_KR.decode(bytes);
    cow.to_string()
}

#[cfg(not(target_os = "windows"))]
fn decode_platform_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn resolve_copilot_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/opt/homebrew/bin/copilot",
            "/usr/local/bin/copilot",
            "/opt/local/bin/copilot",
        ];
        for candidate in candidates {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        for cmd in &["copilot", "github-copilot"] {
            let mut command = std::process::Command::new("where");
            command.arg(cmd);
            command.creation_flags(CREATE_NO_WINDOW);
            command.stdin(Stdio::null());
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());

            if let Ok(output) = command.output() {
                if output.status.success() {
                    let stdout = decode_platform_bytes(&output.stdout);
                    if let Some(first) = stdout.lines().next() {
                        let trimmed = first.trim();
                        if !trimmed.is_empty() {
                            let path = PathBuf::from(trimmed);
                            if path.exists() {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

fn augmented_path() -> String {
    let current = env::var("PATH").unwrap_or_default();
    let separator = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };
    let mut extra: Vec<&str> = Vec::new();
    if cfg!(target_os = "macos") {
        extra.push("/opt/homebrew/bin");
        extra.push("/usr/local/bin");
    }
    if extra.is_empty() {
        return current;
    }
    if current.is_empty() {
        return extra.join(separator);
    }
    format!("{}{}{}", extra.join(separator), separator, current)
}

fn copilot_command() -> std::process::Command {
    let command_path = resolve_copilot_path().unwrap_or_else(|| PathBuf::from("copilot"));
    let mut command = std::process::Command::new(command_path);
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let path = augmented_path();
    if !path.is_empty() {
        command.env("PATH", path);
    }
    command
}

#[derive(Clone, Serialize)]
struct LoginEvent {
    status: &'static str,
    message: String,
}

#[derive(Serialize)]
struct DeviceLoginStart {
    auth_url: String,
    user_code: String,
    expires_in: u64,
    interval: u64,
}

#[tauri::command]
fn start_github_login(app: tauri::AppHandle) -> Result<DeviceLoginStart, String> {
    let client_id = "Ov23liTEmQZzOQ2bdFcm";
    let device = request_device_code(client_id)?;
    let auth_url = device
        .verification_uri_complete
        .clone()
        .unwrap_or_else(|| device.verification_uri.clone());

    std::thread::spawn(move || {
        let result = poll_device_token(client_id, &device.device_code, device.interval)
            .and_then(|token| store_github_token(&token).map(|_| token));

        let payload = match result {
            Ok(_) => LoginEvent {
                status: "ok",
                message: "GitHub token saved to ~/.env".to_string(),
            },
            Err(message) => LoginEvent {
                status: "error",
                message,
            },
        };

        let _ = app.emit("github-login-complete", payload);
    });

    Ok(DeviceLoginStart {
        auth_url,
        user_code: device.user_code,
        expires_in: device.expires_in,
        interval: device.interval,
    })
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

fn request_device_code(client_id: &str) -> Result<DeviceCodeResponse, String> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", "read:user")])
        .send()
        .map_err(|err| format!("Failed to request device code: {err}"))?;

    response
        .json::<DeviceCodeResponse>()
        .map_err(|err| format!("Failed to parse device code response: {err}"))
}

fn poll_device_token(client_id: &str, device_code: &str, interval: u64) -> Result<String, String> {
    let client = reqwest::blocking::Client::new();
    let mut wait = interval.max(5);
    let mut attempts = 0u32;
    loop {
        std::thread::sleep(Duration::from_secs(wait));
        attempts = attempts.saturating_add(1);

        let response = client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .map_err(|err| format!("Failed to poll token: {err}"))?;

        let json: serde_json::Value = response
            .json()
            .map_err(|err| format!("Failed to parse token response: {err}"))?;

        if let Some(token) = json.get("access_token").and_then(|value| value.as_str()) {
            return Ok(token.to_string());
        }

        if let Some(error) = json.get("error").and_then(|value| value.as_str()) {
            match error {
                "authorization_pending" => continue,
                "slow_down" => {
                    wait = wait.saturating_add(5);
                    continue;
                }
                "expired_token" => return Err("Device code expired. Please try again.".to_string()),
                "access_denied" => return Err("Access denied. Please try again.".to_string()),
                _ => {
                    return Err(format!("OAuth error: {error}"));
                }
            }
        }

        if attempts > 120 {
            return Err("Login timed out. Please try again.".to_string());
        }
    }
}

fn store_github_token(token: &str) -> Result<(), String> {
    let env_path = env_path()?;
    let mut contents = fs::read_to_string(&env_path).unwrap_or_default();
    contents = contents
        .lines()
        .filter(|line| !line.trim_start().starts_with("GITHUB_TOKEN="))
        .map(|line| format!("{line}\n"))
        .collect();

    contents.push_str(&format!("GITHUB_TOKEN={}\n", token));
    fs::write(&env_path, contents).map_err(|err| format!("Failed to write ~/.env: {err}"))?;
    Ok(())
}

fn resolve_github_token() -> Option<String> {
    if let Ok(token) = env::var("GITHUB_TOKEN") {
        if !token.trim().is_empty() {
            return Some(token);
        }
    }
    let env_path = env_path().ok()?;
    let contents = fs::read_to_string(env_path).ok()?;
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("GITHUB_TOKEN=") {
            if !value.trim().is_empty() {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn env_path() -> Result<PathBuf, String> {
    // Windows에서는 USERPROFILE을, 그 외에는 HOME을 사용하도록 수정합니다.
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .map_err(|_| "Missing HOME or USERPROFILE environment variable".to_string())?;
    Ok(PathBuf::from(home).join(".env"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let dir = env::temp_dir().join("ghc");
                if dir.exists() {
                    if let Ok(entries) = fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                                if name.starts_with(".copilot-context-") {
                                    let _ = fs::remove_file(path);
                                }
                            }
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            run_copilot,
            get_copilot_version,
            get_copilot_status,
            get_copilot_where_log,
            install_copilot_cli,
            start_github_login,
            has_github_token,
            get_token_status,
            clear_github_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
