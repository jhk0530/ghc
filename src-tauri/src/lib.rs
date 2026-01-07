// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Emitter;

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
        let mut command = std::process::Command::new("copilot");
        let mut temp_path: Option<String> = None;
        let context_path_for_debug = args.context_path.clone();
        let full_prompt = match args.context_path.as_ref() {
            Some(path) if !path.trim().is_empty() => {
                let path_buf = PathBuf::from(path);
                let file_name = path_buf
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path);
                let cwd = env::current_dir()
                    .map_err(|err| format!("Failed to resolve current dir: {err}"))?;
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|err| format!("Failed to generate temp name: {err}"))?
                    .as_millis();
                let temp_name = format!(".copilot-context-{stamp}-{file_name}");
                let temp_file = cwd.join(&temp_name);
                fs::copy(&path_buf, &temp_file).map_err(|err| {
                    format!("Failed to copy context file: {err}")
                })?;
                temp_path = Some(temp_file.display().to_string());
                format!("{} ./{temp_name}", args.prompt)
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
                output: String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .to_string(),
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
        let output = std::process::Command::new("copilot")
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
        fs::write(&env_path, updated)
            .map_err(|err| format!("Failed to write ~/.env: {err}"))?;
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
        let result =
            poll_device_token(client_id, &device.device_code, device.interval)
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
        .form(&[
            ("client_id", client_id),
            ("scope", "read:user"),
        ])
        .send()
        .map_err(|err| format!("Failed to request device code: {err}"))?;

    response
        .json::<DeviceCodeResponse>()
        .map_err(|err| format!("Failed to parse device code response: {err}"))
}

fn poll_device_token(
    client_id: &str,
    device_code: &str,
    interval: u64,
) -> Result<String, String> {
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
                "expired_token" => {
                    return Err("Device code expired. Please try again.".to_string())
                }
                "access_denied" => {
                    return Err("Access denied. Please try again.".to_string())
                }
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
    fs::write(&env_path, contents)
        .map_err(|err| format!("Failed to write ~/.env: {err}"))?;
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
    let home = env::var("HOME").map_err(|_| "Missing HOME environment variable".to_string())?;
    Ok(PathBuf::from(home).join(".env"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Ok(dir) = env::current_dir() {
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
            start_github_login,
            has_github_token,
            get_token_status,
            clear_github_token
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
