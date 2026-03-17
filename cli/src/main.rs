mod color;
mod commands;
mod connection;
mod flags;
mod install;
mod native;
mod output;
#[cfg(test)]
mod test_utils;
mod validation;

use serde_json::json;
use std::env;
use std::fs;
use std::process::exit;

#[cfg(windows)]
use windows_sys::Win32::Foundation::CloseHandle;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use commands::{gen_id, parse_command, ParseError};
use connection::{ensure_daemon, get_socket_dir, send_command, DaemonOptions};
use flags::{clean_args, parse_flags};
use install::run_install;
use output::{
    print_command_help, print_help, print_response_with_opts, print_version, OutputOptions,
};

use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use dirs;

/// Stop daemon if it's running for the default session
fn stop_daemon_if_running() -> Result<(), String> {
    let session = "default";
    let socket_dir = get_socket_dir();
    let pid_file = socket_dir.join(format!("{}.pid", session));
    
    if !pid_file.exists() {
        return Ok(()); // No daemon running
    }
    
    let pid_str = match fs::read_to_string(&pid_file) {
        Ok(content) => content,
        Err(_) => return Ok(()), // Can't read PID file
    };
    
    let pid: i32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => return Ok(()), // Invalid PID
    };
    
    // Try to terminate the process
    #[cfg(unix)]
    {
        unsafe {
            if libc::kill(pid, libc::SIGTERM) == 0 {
                // Wait a bit for graceful shutdown
                std::thread::sleep(std::time::Duration::from_millis(500));
                
                // Check if still running, force kill if needed
                if libc::kill(pid, 0) == 0 {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
    
    #[cfg(windows)]
    {
        use std::process::Command;
        let _ = Command::new("taskkill")
            .args(&["/PID", &pid.to_string(), "/F"])
            .output();
    }
    
    // Clean up PID file
    let _ = fs::remove_file(&pid_file);
    
    Ok(())
}

/// Run update command to update the package and skills
fn run_update(args: &[String], json_mode: bool) -> ! {
    let force = args.iter().any(|a| a == "--force" || a == "-f");
    let check_only = args.iter().any(|a| a == "--check" || a == "-c");
    
    if check_only {
        // Just check for updates without installing
        match ProcessCommand::new("npm")
            .args(&["outdated", "-g", "scrapeless-scraping-browser"])
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    if json_mode {
                        println!(r#"{{"success":true,"upToDate":true,"message":"Already up to date"}}"#);
                    } else {
                        println!("{} Already up to date", color::success_indicator());
                    }
                } else {
                    if json_mode {
                        println!(r#"{{"success":true,"upToDate":false,"message":"Update available"}}"#);
                    } else {
                        println!("{} Update available", color::warning_indicator());
                        println!("{}", stdout);
                    }
                }
                exit(0);
            }
            Err(e) => {
                if json_mode {
                    println!(r#"{{"success":false,"error":"Failed to check for updates: {}"}}"#, e);
                } else {
                    eprintln!("{} Failed to check for updates: {}", color::error_indicator(), e);
                }
                exit(1);
            }
        }
    }
    
    // Update the package
    let npm_args = if force {
        vec!["install", "-g", "scrapeless-scraping-browser@latest", "--force"]
    } else {
        vec!["install", "-g", "scrapeless-scraping-browser@latest"]
    };
    
    if !json_mode {
        println!("{} Updating scrapeless-scraping-browser...", color::info_indicator());
    }
    
    match ProcessCommand::new("npm")
        .args(&npm_args)
        .output()
    {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if json_mode {
                    println!(r#"{{"success":false,"error":"Failed to update package: {}"}}"#, stderr);
                } else {
                    eprintln!("{} Failed to update package: {}", color::error_indicator(), stderr);
                }
                exit(1);
            }
            
            if !json_mode {
                println!("{} Package updated successfully", color::success_indicator());
                println!("{} Updating skills...", color::info_indicator());
            }
            
            // Update skills
            match ProcessCommand::new("npx")
                .args(&["skills", "add", "scrapeless-ai/scraping-browser-skill", "-g", "-y"])
                .output()
            {
                Ok(skills_output) => {
                    if !skills_output.status.success() {
                        let stderr = String::from_utf8_lossy(&skills_output.stderr);
                        if json_mode {
                            println!(r#"{{"success":false,"error":"Package updated but failed to update skills: {}"}}"#, stderr);
                        } else {
                            eprintln!("{} Package updated but failed to update skills: {}", color::warning_indicator(), stderr);
                            eprintln!("You can manually update skills with: npx skills add scrapeless-ai/scraping-browser-skill -g -y");
                        }
                        exit(1);
                    }
                    
                    if json_mode {
                        println!(r#"{{"success":true,"message":"Package and skills updated successfully"}}"#);
                    } else {
                        println!("{} Package and skills updated successfully", color::success_indicator());
                    }
                    exit(0);
                }
                Err(e) => {
                    if json_mode {
                        println!(r#"{{"success":false,"error":"Package updated but failed to update skills: {}"}}"#, e);
                    } else {
                        eprintln!("{} Package updated but failed to update skills: {}", color::warning_indicator(), e);
                        eprintln!("You can manually update skills with: npx skills add scrapeless-ai/scraping-browser-skill -g -y");
                    }
                    exit(1);
                }
            }
        }
        Err(e) => {
            if json_mode {
                println!(r#"{{"success":false,"error":"Failed to update package: {}"}}"#, e);
            } else {
                eprintln!("{} Failed to update package: {}", color::error_indicator(), e);
            }
            exit(1);
        }
    }
}

/// Run a local auth command (auth_save/list/show/delete) via node auth-cli.js.
/// These commands don't need a browser, so we handle them directly to avoid
/// sending passwords through the daemon's Unix socket channel.
fn run_auth_cli(cmd: &serde_json::Value, json_mode: bool) -> ! {
    let exe_path = env::current_exe().unwrap_or_default();
    let exe_path = exe_path.canonicalize().unwrap_or(exe_path);
    #[cfg(windows)]
    let exe_path = {
        let p = exe_path.to_string_lossy();
        if let Some(stripped) = p.strip_prefix(r"\\?\") {
            PathBuf::from(stripped)
        } else {
            exe_path
        }
    };
    let exe_dir = exe_path.parent().unwrap_or(std::path::Path::new("."));

    let script_paths = vec![
        exe_dir.join("auth-cli.js"),
        exe_dir.join("../dist/auth-cli.js"),
        PathBuf::from("dist/auth-cli.js"),
    ];

    let script_path = match script_paths.iter().find(|p| p.exists()) {
        Some(p) => p.clone(),
        None => {
            if json_mode {
                println!(r#"{{"success":false,"error":"auth-cli.js not found"}}"#);
            } else {
                eprintln!(
                    "{} auth-cli.js not found. Run from project directory.",
                    color::error_indicator()
                );
            }
            exit(1);
        }
    };

    let cmd_json = serde_json::to_string(cmd).unwrap_or_default();

    match ProcessCommand::new("node")
        .arg(&script_path)
        .arg(&cmd_json)
        .output()
    {
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stdout = stdout.trim();

            if stdout.is_empty() {
                if json_mode {
                    println!(r#"{{"success":false,"error":"No response from auth-cli"}}"#);
                } else {
                    eprintln!("{} No response from auth-cli", color::error_indicator());
                }
                exit(1);
            }

            if json_mode {
                println!("{}", stdout);
            } else {
                // Parse the JSON response and use the standard output formatter
                match serde_json::from_str::<connection::Response>(stdout) {
                    Ok(resp) => {
                        let action = cmd.get("action").and_then(|v| v.as_str());
                        let opts = OutputOptions {
                            json: false,
                            content_boundaries: false,
                            max_output: None,
                        };
                        print_response_with_opts(&resp, action, &opts);
                        if !resp.success {
                            exit(1);
                        }
                    }
                    Err(_) => {
                        println!("{}", stdout);
                    }
                }
            }
            exit(output.status.code().unwrap_or(0));
        }
        Err(e) => {
            if json_mode {
                println!(
                    r#"{{"success":false,"error":"Failed to run auth-cli: {}"}}"#,
                    e
                );
            } else {
                eprintln!("{} Failed to run auth-cli: {}", color::error_indicator(), e);
            }
            exit(1);
        }
    }
}

/// Run a local config command (config_set/get/list/remove) locally without daemon.
/// These commands manage the local configuration file and don't need browser access.
fn run_config_cli(cmd: &serde_json::Value, json_mode: bool) -> ! {
    let action = cmd.get("action").and_then(|v| v.as_str()).unwrap_or("");
    
    // Get config directory path
    let config_dir = if let Some(home) = dirs::home_dir() {
        home.join(".scrapeless")
    } else {
        eprintln!("{} Unable to determine home directory", color::error_indicator());
        exit(1);
    };
    
    let config_path = config_dir.join("config.json");
    
    // Ensure config directory exists
    if !config_dir.exists() {
        if let Err(e) = fs::create_dir_all(&config_dir) {
            if json_mode {
                println!(r#"{{"success":false,"error":"Failed to create config directory: {}"}}"#, e);
            } else {
                eprintln!("{} Failed to create config directory: {}", color::error_indicator(), e);
            }
            exit(1);
        }
        // Set directory permissions to 0700 (user only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o700)) {
                if json_mode {
                    println!(r#"{{"success":false,"error":"Failed to set config directory permissions: {}"}}"#, e);
                } else {
                    eprintln!("{} Failed to set config directory permissions: {}", color::error_indicator(), e);
                }
                exit(1);
            }
        }
    }
    
    // Load existing config
    let mut config: serde_json::Map<String, serde_json::Value> = if config_path.exists() {
        match fs::read_to_string(&config_path) {
            Ok(content) => {
                match serde_json::from_str(&content) {
                    Ok(c) => c,
                    Err(_) => {
                        if json_mode {
                            println!(r#"{{"success":false,"error":"Failed to parse config file"}}"#);
                        } else {
                            eprintln!("{} Failed to parse config file", color::error_indicator());
                        }
                        exit(1);
                    }
                }
            }
            Err(e) => {
                if json_mode {
                    println!(r#"{{"success":false,"error":"Failed to read config file: {}"}}"#, e);
                } else {
                    eprintln!("{} Failed to read config file: {}", color::error_indicator(), e);
                }
                exit(1);
            }
        }
    } else {
        serde_json::Map::new()
    };
    
    match action {
        "config_set" => {
            let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let value = cmd.get("value").and_then(|v| v.as_str()).unwrap_or("");
            
            // Convert string values to appropriate types
            let typed_value = match key {
                "sessionTtl" => {
                    match value.parse::<i64>() {
                        Ok(n) => json!(n),
                        Err(_) => {
                            if json_mode {
                                println!(r#"{{"success":false,"error":"sessionTtl must be a number"}}"#);
                            } else {
                                eprintln!("{} sessionTtl must be a number", color::error_indicator());
                            }
                            exit(1);
                        }
                    }
                }
                "sessionRecording" | "debug" => {
                    match value.to_lowercase().as_str() {
                        "true" => json!(true),
                        "false" => json!(false),
                        _ => {
                            if json_mode {
                                println!(r#"{{"success":false,"error":"{} must be true or false"}}"#, key);
                            } else {
                                eprintln!("{} {} must be true or false", color::error_indicator(), key);
                            }
                            exit(1);
                        }
                    }
                }
                _ => json!(value)
            };
            
            config.insert(key.to_string(), typed_value);
            
            // Save config
            let config_json = match serde_json::to_string_pretty(&config) {
                Ok(json) => json,
                Err(e) => {
                    if json_mode {
                        println!(r#"{{"success":false,"error":"Failed to serialize config: {}"}}"#, e);
                    } else {
                        eprintln!("{} Failed to serialize config: {}", color::error_indicator(), e);
                    }
                    exit(1);
                }
            };
            
            if let Err(e) = fs::write(&config_path, config_json) {
                if json_mode {
                    println!(r#"{{"success":false,"error":"Failed to write config file: {}"}}"#, e);
                } else {
                    eprintln!("{} Failed to write config file: {}", color::error_indicator(), e);
                }
                exit(1);
            }
            
            // Set file permissions to 0600 (user read/write only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) = fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600)) {
                    if json_mode {
                        println!(r#"{{"success":false,"error":"Failed to set config file permissions: {}"}}"#, e);
                    } else {
                        eprintln!("{} Failed to set config file permissions: {}", color::error_indicator(), e);
                    }
                    exit(1);
                }
            }
            
            // For critical config changes (API key), restart daemon to ensure changes take effect
            if key == "apiKey" || key == "key" {
                // Try to stop existing daemon
                if let Err(_) = stop_daemon_if_running() {
                    // Ignore errors - daemon might not be running
                }
                
                if json_mode {
                    println!(r#"{{"success":true,"message":"Configuration saved and daemon restarted"}}"#);
                } else {
                    println!("{} Configuration saved and daemon restarted", color::success_indicator());
                }
            } else {
                if json_mode {
                    println!(r#"{{"success":true,"message":"Configuration saved"}}"#);
                } else {
                    println!("{} Configuration saved", color::success_indicator());
                }
            }
        }
        "config_get" => {
            let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("");
            
            if let Some(value) = config.get(key) {
                if json_mode {
                    println!(r#"{{"success":true,"value":{}}}"#, value);
                } else {
                    println!("{}", value.as_str().unwrap_or(&value.to_string()));
                }
            } else {
                if json_mode {
                    println!(r#"{{"success":false,"error":"Configuration key '{}' not found"}}"#, key);
                } else {
                    eprintln!("{} Configuration key '{}' not found", color::error_indicator(), key);
                }
                exit(1);
            }
        }
        "config_list" => {
            if json_mode {
                println!(r#"{{"success":true,"config":{}}}"#, serde_json::to_string(&config).unwrap_or_default());
            } else {
                if config.is_empty() {
                    println!("No configuration set");
                } else {
                    for (key, value) in &config {
                        println!("{}: {}", key, value.as_str().unwrap_or(&value.to_string()));
                    }
                }
            }
        }
        "config_remove" => {
            let key = cmd.get("key").and_then(|v| v.as_str()).unwrap_or("");
            
            if config.remove(key).is_some() {
                // Save updated config
                let config_json = match serde_json::to_string_pretty(&config) {
                    Ok(json) => json,
                    Err(e) => {
                        if json_mode {
                            println!(r#"{{"success":false,"error":"Failed to serialize config: {}"}}"#, e);
                        } else {
                            eprintln!("{} Failed to serialize config: {}", color::error_indicator(), e);
                        }
                        exit(1);
                    }
                };
                
                if let Err(e) = fs::write(&config_path, config_json) {
                    if json_mode {
                        println!(r#"{{"success":false,"error":"Failed to write config file: {}"}}"#, e);
                    } else {
                        eprintln!("{} Failed to write config file: {}", color::error_indicator(), e);
                    }
                    exit(1);
                }
                
                if json_mode {
                    println!(r#"{{"success":true,"message":"Configuration key '{}' removed"}}"#, key);
                } else {
                    println!("{} Configuration key '{}' removed", color::success_indicator(), key);
                }
            } else {
                if json_mode {
                    println!(r#"{{"success":false,"error":"Configuration key '{}' not found"}}"#, key);
                } else {
                    eprintln!("{} Configuration key '{}' not found", color::error_indicator(), key);
                }
                exit(1);
            }
        }
        _ => {
            if json_mode {
                println!(r#"{{"success":false,"error":"Unknown config action: {}"}}"#, action);
            } else {
                eprintln!("{} Unknown config action: {}", color::error_indicator(), action);
            }
            exit(1);
        }
    }
    
    exit(0);
}

// Session management commands are now handled by the daemon, not locally

#[allow(dead_code)]
fn parse_proxy(proxy_str: &str) -> serde_json::Value {
    let Some(protocol_end) = proxy_str.find("://") else {
        return json!({ "server": proxy_str });
    };
    let protocol = &proxy_str[..protocol_end + 3];
    let rest = &proxy_str[protocol_end + 3..];

    let Some(at_pos) = rest.rfind('@') else {
        return json!({ "server": proxy_str });
    };

    let creds = &rest[..at_pos];
    let server_part = &rest[at_pos + 1..];
    let server = format!("{}{}", protocol, server_part);

    let Some(colon_pos) = creds.find(':') else {
        return json!({
            "server": server,
            "username": creds,
            "password": ""
        });
    };

    json!({
        "server": server,
        "username": &creds[..colon_pos],
        "password": &creds[colon_pos + 1..]
    })
}

fn run_session(args: &[String], session: &str, json_mode: bool) {
    let subcommand = args.get(1).map(|s| s.as_str());

    match subcommand {
        Some("list") => {
            let socket_dir = get_socket_dir();
            let mut sessions: Vec<String> = Vec::new();

            if let Ok(entries) = fs::read_dir(&socket_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    // Look for pid files in socket directory
                    if name.ends_with(".pid") {
                        let session_name = name.strip_suffix(".pid").unwrap_or("");
                        if !session_name.is_empty() {
                            // Check if session is actually running
                            let pid_path = socket_dir.join(&name);
                            if let Ok(pid_str) = fs::read_to_string(&pid_path) {
                                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                                    #[cfg(unix)]
                                    let running = unsafe {
                                        libc::kill(pid as i32, 0) == 0
                                            || std::io::Error::last_os_error().raw_os_error()
                                                != Some(libc::ESRCH)
                                    };
                                    #[cfg(windows)]
                                    let running = unsafe {
                                        let handle =
                                            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                                        if handle != 0 {
                                            CloseHandle(handle);
                                            true
                                        } else {
                                            false
                                        }
                                    };
                                    if running {
                                        sessions.push(session_name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if json_mode {
                println!(
                    r#"{{"success":true,"data":{{"sessions":{}}}}}"#,
                    serde_json::to_string(&sessions).unwrap_or_default()
                );
            } else if sessions.is_empty() {
                println!("No active sessions");
            } else {
                println!("Active sessions:");
                for s in &sessions {
                    let marker = if s == session {
                        color::cyan("→")
                    } else {
                        " ".to_string()
                    };
                    println!("{} {}", marker, s);
                }
            }
        }
        None | Some(_) => {
            // Just show current session
            if json_mode {
                println!(r#"{{"success":true,"data":{{"session":"{}"}}}}"#, session);
            } else {
                println!("{}", session);
            }
        }
    }
}

fn main() {
    // Ignore SIGPIPE to prevent panic when piping to head/tail
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    // Prevent MSYS/Git Bash path translation from mangling arguments
    #[cfg(windows)]
    {
        env::set_var("MSYS_NO_PATHCONV", "1");
        env::set_var("MSYS2_ARG_CONV_EXCL", "*");
    }

    // Remove daemon mode environment variable support
    // Daemon mode is handled differently now

    let args: Vec<String> = env::args().skip(1).collect();
    let mut flags = parse_flags(&args);
    let clean = clean_args(&args);

    if flags.engine.is_some() && !flags.native {
        flags.native = true;
    }

    let has_help = args.iter().any(|a| a == "--help" || a == "-h");
    let has_version = args.iter().any(|a| a == "--version" || a == "-V");

    if has_help {
        if let Some(cmd) = clean.first() {
            if print_command_help(cmd) {
                return;
            }
        }
        print_help();
        return;
    }

    if has_version {
        print_version();
        return;
    }

    if clean.is_empty() {
        print_help();
        return;
    }

    // Handle install separately
    if clean.first().map(|s| s.as_str()) == Some("install") {
        let with_deps = args.iter().any(|a| a == "--with-deps" || a == "-d");
        run_install(with_deps);
        return;
    }

    // Handle update separately
    if clean.first().map(|s| s.as_str()) == Some("update") {
        run_update(&args, flags.json);
        // run_update never returns (calls exit())
    }

    // Handle session separately (doesn't need daemon)
    if clean.first().map(|s| s.as_str()) == Some("session") {
        run_session(&clean, &flags.session, flags.json);
        return;
    }

    let mut cmd = match parse_command(&clean, &flags) {
        Ok(c) => c,
        Err(e) => {
            if flags.json {
                let error_type = match &e {
                    ParseError::UnknownCommand { .. } => "unknown_command",
                    ParseError::UnknownSubcommand { .. } => "unknown_subcommand",
                    ParseError::MissingArguments { .. } => "missing_arguments",
                    ParseError::InvalidValue { .. } => "invalid_value",
                    ParseError::InvalidSessionName { .. } => "invalid_session_name",
                };
                println!(
                    r#"{{"success":false,"error":"{}","type":"{}"}}"#,
                    e.format().replace('\n', " "),
                    error_type
                );
            } else {
                eprintln!("{}", color::red(&e.format()));
            }
            exit(1);
        }
    };

    // Handle --password-stdin for auth save
    if cmd.get("action").and_then(|v| v.as_str()) == Some("auth_save") {
        if cmd.get("password").is_some() {
            eprintln!(
                "{} Passwords on the command line may be visible in process listings and shell history. Use --password-stdin instead.",
                color::warning_indicator()
            );
        }
        if cmd
            .get("passwordStdin")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let mut pass = String::new();
            if std::io::stdin().read_line(&mut pass).is_err() || pass.is_empty() {
                eprintln!(
                    "{} Failed to read password from stdin",
                    color::error_indicator()
                );
                exit(1);
            }
            let pass = pass.trim_end_matches('\n').trim_end_matches('\r');
            if pass.is_empty() {
                eprintln!("{} Password from stdin is empty", color::error_indicator());
                exit(1);
            }
            cmd["password"] = json!(pass);
            cmd.as_object_mut().unwrap().remove("passwordStdin");
        }
    }

    // Handle local auth commands without starting the daemon.
    // These don't need a browser, so we avoid sending passwords through the socket.
    if let Some(action) = cmd.get("action").and_then(|v| v.as_str()) {
        if matches!(
            action,
            "auth_save" | "auth_list" | "auth_show" | "auth_delete"
        ) {
            run_auth_cli(&cmd, flags.json);
        }
        
        // Handle local config commands without starting the daemon.
        // These don't need a browser, so we handle them locally.
        if matches!(
            action,
            "config_set" | "config_get" | "config_list" | "config_remove"
        ) {
            run_config_cli(&cmd, flags.json);
        }
        
        // Handle session management commands via daemon (not locally).
        // These commands are now forwarded to the TypeScript daemon which handles API calls.
        if matches!(
            action,
            "scrapeless_sessions" | "scrapeless_create" | "scrapeless_stop" | "scrapeless_stop_all" | "scrapeless_live"
        ) {
            // Forward to daemon instead of handling locally
            // The daemon will handle the API calls using the new TypeScript API structure
        }
    }

    // Validate session name before starting daemon
    if let Some(ref name) = flags.session_name {
        if !validation::is_valid_session_name(name) {
            let msg = validation::session_name_error(name);
            if flags.json {
                println!(
                    r#"{{"success":false,"error":"{}","type":"invalid_session_name"}}"#,
                    msg.replace('"', "\\\"")
                );
            } else {
                eprintln!("{} {}", color::error_indicator(), msg);
            }
            exit(1);
        }
    }

    let daemon_opts = DaemonOptions {
        headed: flags.headed,
        debug: flags.debug,
        executable_path: flags.executable_path.as_deref(),
        extensions: &flags.extensions,
        args: flags.args.as_deref(),
        user_agent: flags.user_agent.as_deref(),
        proxy: flags.proxy.as_deref(),
        proxy_bypass: flags.proxy_bypass.as_deref(),
        ignore_https_errors: flags.ignore_https_errors,
        allow_file_access: flags.allow_file_access,
        profile: flags.profile.as_deref(),
        state: flags.state.as_deref(),
        provider: None,
        device: flags.device.as_deref(),
        session_name: flags.session_name.as_deref(),
        download_path: flags.download_path.as_deref(),
        allowed_domains: flags.allowed_domains.as_deref(),
        action_policy: flags.action_policy.as_deref(),
        confirm_actions: flags.confirm_actions.as_deref(),
        native: flags.native,
        engine: flags.engine.as_deref(),
    };
    let daemon_result = match ensure_daemon(&flags.session, &daemon_opts) {
        Ok(result) => result,
        Err(e) => {
            if flags.json {
                println!(r#"{{"success":false,"error":"{}"}}"#, e);
            } else {
                eprintln!("{} {}", color::error_indicator(), e);
            }
            exit(1);
        }
    };

    // Warn if launch-time options were explicitly passed via CLI but daemon was already running
    // Only warn about flags that were passed on the command line, not those set via environment
    // variables (since the daemon already uses the env vars when it starts).
    if daemon_result.already_running {
        let ignored_flags: Vec<&str> = [
            if flags.cli_executable_path {
                Some("--executable-path")
            } else {
                None
            },
            if flags.cli_extensions {
                Some("--extension")
            } else {
                None
            },
            if flags.cli_profile {
                Some("--profile")
            } else {
                None
            },
            if flags.cli_state {
                Some("--state")
            } else {
                None
            },
            if flags.cli_args { Some("--args") } else { None },
            if flags.cli_user_agent {
                Some("--user-agent")
            } else {
                None
            },
            if flags.cli_proxy {
                Some("--proxy")
            } else {
                None
            },
            if flags.cli_proxy_bypass {
                Some("--proxy-bypass")
            } else {
                None
            },
            flags.ignore_https_errors.then_some("--ignore-https-errors"),
            flags.cli_allow_file_access.then_some("--allow-file-access"),
            flags.cli_download_path.then_some("--download-path"),
            flags.cli_native.then_some("--native"),
        ]
        .into_iter()
        .flatten()
        .collect();

        if !ignored_flags.is_empty() && !flags.json {
            eprintln!(
                "{} {} ignored: daemon already running. Use 'scrapeless-scraping-browser close' first to restart with new options.",
                color::warning_indicator(),
                ignored_flags.join(", ")
            );
        }
    }

    // Launch Scrapeless cloud browser with optional sessionId
    // Skip launch for Scrapeless management commands that don't need a browser
    let skip_launch = if let Some(action) = cmd.get("action").and_then(|v| v.as_str()) {
        matches!(
            action,
            "scrapeless_sessions" | "scrapeless_create" | "scrapeless_stop" | "scrapeless_stop_all" | "scrapeless_live"
        )
    } else {
        false
    };

    if !skip_launch {
        let mut launch_cmd = json!({
            "id": gen_id(),
            "action": "launch"
        });

        if let Some(ref session_id) = flags.session_id {
            launch_cmd["sessionId"] = json!(session_id);
        }

        if let Some(ref cs) = flags.color_scheme {
            launch_cmd["colorScheme"] = json!(cs);
        }

        let err = match send_command(launch_cmd, &flags.session) {
            Ok(resp) if resp.success => None,
            Ok(resp) => Some(
                resp.error
                    .unwrap_or_else(|| "Scrapeless connection failed".to_string()),
            ),
            Err(e) => Some(e.to_string()),
        };

        if let Some(msg) = err {
            if flags.json {
                println!(r#"{{"success":false,"error":"{}"}}"#, msg);
            } else {
                eprintln!("{} {}", color::error_indicator(), msg);
            }
            exit(1);
        }
    }

    let output_opts = OutputOptions {
        json: flags.json,
        content_boundaries: flags.content_boundaries,
        max_output: flags.max_output,
    };

    match send_command(cmd.clone(), &flags.session) {
        Ok(resp) => {
            let success = resp.success;
            // Handle interactive confirmation
            if flags.confirm_interactive {
                if let Some(data) = &resp.data {
                    if data
                        .get("confirmation_required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        let desc = data
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown action");
                        let category = data.get("category").and_then(|v| v.as_str()).unwrap_or("");
                        let cid = data
                            .get("confirmation_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        eprintln!("[agent-browser] Action requires confirmation:");
                        eprintln!("  {}: {}", category, desc);
                        eprint!("  Allow? [y/N]: ");

                        let mut input = String::new();
                        let approved = if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
                            std::io::stdin().read_line(&mut input).is_ok()
                                && matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
                        } else {
                            false
                        };

                        let confirm_cmd = if approved {
                            json!({ "id": gen_id(), "action": "confirm", "confirmationId": cid })
                        } else {
                            json!({ "id": gen_id(), "action": "deny", "confirmationId": cid })
                        };

                        match send_command(confirm_cmd, &flags.session) {
                            Ok(r) => {
                                if !approved {
                                    eprintln!("{} Action denied", color::error_indicator());
                                    exit(1);
                                }
                                print_response_with_opts(&r, None, &output_opts);
                            }
                            Err(e) => {
                                eprintln!("{} {}", color::error_indicator(), e);
                                exit(1);
                            }
                        }
                        return;
                    }
                }
            }
            // Extract action for context-specific output handling
            let action = cmd.get("action").and_then(|v| v.as_str());
            print_response_with_opts(&resp, action, &output_opts);
            if !success {
                exit(1);
            }
        }
        Err(e) => {
            if flags.json {
                println!(r#"{{"success":false,"error":"{}"}}"#, e);
            } else {
                eprintln!("{} {}", color::error_indicator(), e);
            }
            exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_proxy_simple() {
        let result = parse_proxy("http://proxy.com:8080");
        assert_eq!(result["server"], "http://proxy.com:8080");
        assert!(result.get("username").is_none());
        assert!(result.get("password").is_none());
    }

    #[test]
    fn test_parse_proxy_with_auth() {
        let result = parse_proxy("http://user:pass@proxy.com:8080");
        assert_eq!(result["server"], "http://proxy.com:8080");
        assert_eq!(result["username"], "user");
        assert_eq!(result["password"], "pass");
    }

    #[test]
    fn test_parse_proxy_username_only() {
        let result = parse_proxy("http://user@proxy.com:8080");
        assert_eq!(result["server"], "http://proxy.com:8080");
        assert_eq!(result["username"], "user");
        assert_eq!(result["password"], "");
    }

    #[test]
    fn test_parse_proxy_no_protocol() {
        let result = parse_proxy("proxy.com:8080");
        assert_eq!(result["server"], "proxy.com:8080");
        assert!(result.get("username").is_none());
    }

    #[test]
    fn test_parse_proxy_socks5() {
        let result = parse_proxy("socks5://proxy.com:1080");
        assert_eq!(result["server"], "socks5://proxy.com:1080");
        assert!(result.get("username").is_none());
    }

    #[test]
    fn test_parse_proxy_socks5_with_auth() {
        let result = parse_proxy("socks5://admin:secret@proxy.com:1080");
        assert_eq!(result["server"], "socks5://proxy.com:1080");
        assert_eq!(result["username"], "admin");
        assert_eq!(result["password"], "secret");
    }

    #[test]
    fn test_parse_proxy_complex_password() {
        let result = parse_proxy("http://user:p@ss:w0rd@proxy.com:8080");
        assert_eq!(result["server"], "http://proxy.com:8080");
        assert_eq!(result["username"], "user");
        assert_eq!(result["password"], "p@ss:w0rd");
    }
}
