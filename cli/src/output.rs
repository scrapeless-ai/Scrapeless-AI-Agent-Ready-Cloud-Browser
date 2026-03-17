use std::sync::OnceLock;

use crate::color;
use crate::connection::Response;

static BOUNDARY_NONCE: OnceLock<String> = OnceLock::new();

/// Per-process nonce for content boundary markers. Uses a CSPRNG (getrandom) so
/// that untrusted page content cannot predict or spoof the boundary delimiter.
/// Process ID or timestamps would be insufficient since pages can read those.
fn get_boundary_nonce() -> &'static str {
    BOUNDARY_NONCE.get_or_init(|| {
        let mut buf = [0u8; 16];
        getrandom::getrandom(&mut buf).expect("failed to generate random nonce");
        buf.iter().map(|b| format!("{:02x}", b)).collect()
    })
}

#[derive(Default)]
pub struct OutputOptions {
    pub json: bool,
    pub content_boundaries: bool,
    pub max_output: Option<usize>,
}

fn truncate_if_needed(content: &str, max: Option<usize>) -> String {
    let Some(limit) = max else {
        return content.to_string();
    };
    // Fast path: byte length is a lower bound on char count, so if the
    // byte length is within the limit the char count must be too.
    if content.len() <= limit {
        return content.to_string();
    }
    // Find the byte offset of the limit-th character.
    match content.char_indices().nth(limit).map(|(i, _)| i) {
        Some(byte_offset) => {
            let total_chars = content.chars().count();
            format!(
                "{}\n[truncated: showing {} of {} chars. Use --max-output to adjust]",
                &content[..byte_offset],
                limit,
                total_chars
            )
        }
        // Content has fewer than `limit` chars despite more bytes
        None => content.to_string(),
    }
}

fn print_with_boundaries(content: &str, origin: Option<&str>, opts: &OutputOptions) {
    let content = truncate_if_needed(content, opts.max_output);
    if opts.content_boundaries {
        let origin_str = origin.unwrap_or("unknown");
        let nonce = get_boundary_nonce();
        println!(
            "--- SCRAPELESS_BROWSER_PAGE_CONTENT nonce={} origin={} ---",
            nonce, origin_str
        );
        println!("{}", content);
        println!("--- END_SCRAPELESS_BROWSER_PAGE_CONTENT nonce={} ---", nonce);
    } else {
        println!("{}", content);
    }
}

pub fn print_response_with_opts(resp: &Response, action: Option<&str>, opts: &OutputOptions) {
    if opts.json {
        if opts.content_boundaries {
            let mut json_val = serde_json::to_value(resp).unwrap_or_default();
            if let Some(obj) = json_val.as_object_mut() {
                let nonce = get_boundary_nonce();
                let origin = obj
                    .get("data")
                    .and_then(|d| d.get("origin"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                obj.insert(
                    "_boundary".to_string(),
                    serde_json::json!({
                        "nonce": nonce,
                        "origin": origin,
                    }),
                );
            }
            println!("{}", serde_json::to_string(&json_val).unwrap_or_default());
        } else {
            println!("{}", serde_json::to_string(resp).unwrap_or_default());
        }
        return;
    }

    if !resp.success {
        eprintln!(
            "{} {}",
            color::error_indicator(),
            resp.error.as_deref().unwrap_or("Unknown error")
        );
        return;
    }

    if let Some(data) = &resp.data {
        // Scrapeless session management responses - check first to avoid conflicts
        if let Some(task_id) = data.get("taskId").and_then(|v| v.as_str()) {
            match action {
                Some("scrapeless_create") => {
                    println!("{} Session created: {}", color::success_indicator(), color::green(task_id));
                    if let Some(msg) = data.get("message").and_then(|v| v.as_str()) {
                        println!("  {}", color::dim(msg));
                    }
                    return;
                }
                Some("scrapeless_stop") => {
                    println!("{} Session stopped: {}", color::success_indicator(), task_id);
                    if let Some(msg) = data.get("message").and_then(|v| v.as_str()) {
                        println!("  {}", color::dim(msg));
                    }
                    return;
                }
                Some("scrapeless_live") => {
                    if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
                        println!("{} Live preview URL:", color::success_indicator());
                        println!("  {}", color::cyan(url));
                    }
                    return;
                }
                _ => {}
            }
        }
        
        // Scrapeless stop-all response
        if let Some(stopped) = data.get("stopped").and_then(|v| v.as_bool()) {
            if stopped && action == Some("scrapeless_stop_all") {
                if let Some(msg) = data.get("message").and_then(|v| v.as_str()) {
                    println!("{} {}", color::success_indicator(), msg);
                } else {
                    println!("{} All sessions stopped", color::success_indicator());
                }
                return;
            }
        }
        
        // Navigation response
        if let Some(url) = data.get("url").and_then(|v| v.as_str()) {
            if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                println!("{} {}", color::success_indicator(), color::bold(title));
                println!("  {}", color::dim(url));
                return;
            }
            println!("{}", url);
            return;
        }
        // Diff responses -- route by action to avoid fragile shape probing
        if let Some(obj) = data.as_object() {
            match action {
                Some("diff_snapshot") => {
                    print_snapshot_diff(obj);
                    return;
                }
                Some("diff_screenshot") => {
                    print_screenshot_diff(obj);
                    return;
                }
                Some("diff_url") => {
                    if let Some(snap_data) = obj.get("snapshot").and_then(|v| v.as_object()) {
                        println!("{}", color::bold("Snapshot diff:"));
                        print_snapshot_diff(snap_data);
                    }
                    if let Some(ss_data) = obj.get("screenshot").and_then(|v| v.as_object()) {
                        println!("\n{}", color::bold("Screenshot diff:"));
                        print_screenshot_diff(ss_data);
                    }
                    return;
                }
                _ => {}
            }
        }
        let origin = data.get("origin").and_then(|v| v.as_str());
        // Snapshot
        if let Some(snapshot) = data.get("snapshot").and_then(|v| v.as_str()) {
            print_with_boundaries(snapshot, origin, opts);
            return;
        }
        // Title
        if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
            println!("{}", title);
            return;
        }
        // Text
        if let Some(text) = data.get("text").and_then(|v| v.as_str()) {
            print_with_boundaries(text, origin, opts);
            return;
        }
        // HTML
        if let Some(html) = data.get("html").and_then(|v| v.as_str()) {
            print_with_boundaries(html, origin, opts);
            return;
        }
        // Value
        if let Some(value) = data.get("value").and_then(|v| v.as_str()) {
            println!("{}", value);
            return;
        }
        // Count
        if let Some(count) = data.get("count").and_then(|v| v.as_i64()) {
            // Check if this is a sessions response (has sessions array)
            if let Some(sessions) = data.get("sessions").and_then(|v| v.as_array()) {
                if sessions.is_empty() {
                    println!("No running sessions");
                } else {
                    println!("{}", color::bold(&format!("Running sessions ({}):", count)));
                    for session in sessions {
                        let task_id = session.get("taskId").and_then(|v| v.as_str()).unwrap_or("");
                        let state = session.get("state").and_then(|v| v.as_str()).unwrap_or("");
                        let create_time = session.get("createTime").and_then(|v| v.as_str()).unwrap_or("");
                        let session_name = session.get("sessionName").and_then(|v| v.as_str()).unwrap_or("");
                        
                        let name_str = if !session_name.is_empty() {
                            format!(" ({})", session_name)
                        } else {
                            String::new()
                        };
                        
                        println!("  {} {} - {}{}", 
                            color::green(task_id),
                            color::dim(state),
                            color::dim(create_time),
                            color::cyan(&name_str)
                        );
                    }
                }
                return;
            }
            // Otherwise just print the count
            println!("{}", count);
            return;
        }
        // Boolean results
        if let Some(visible) = data.get("visible").and_then(|v| v.as_bool()) {
            println!("{}", visible);
            return;
        }
        if let Some(enabled) = data.get("enabled").and_then(|v| v.as_bool()) {
            println!("{}", enabled);
            return;
        }
        if let Some(checked) = data.get("checked").and_then(|v| v.as_bool()) {
            println!("{}", checked);
            return;
        }
        // Eval result
        if let Some(result) = data.get("result") {
            let formatted = serde_json::to_string_pretty(result).unwrap_or_default();
            print_with_boundaries(&formatted, origin, opts);
            return;
        }
        // Tabs
        if let Some(tabs) = data.get("tabs").and_then(|v| v.as_array()) {
            for (i, tab) in tabs.iter().enumerate() {
                let title = tab
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Untitled");
                let url = tab.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let active = tab.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
                let marker = if active {
                    color::cyan("→")
                } else {
                    " ".to_string()
                };
                println!("{} [{}] {} - {}", marker, i, title, url);
            }
            return;
        }
        // Console logs
        if let Some(logs) = data.get("messages").and_then(|v| v.as_array()) {
            if opts.content_boundaries {
                let mut console_output = String::new();
                for log in logs {
                    let level = log.get("type").and_then(|v| v.as_str()).unwrap_or("log");
                    let text = log.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    console_output.push_str(&format!(
                        "{} {}\n",
                        color::console_level_prefix(level),
                        text
                    ));
                }
                if console_output.ends_with('\n') {
                    console_output.pop();
                }
                print_with_boundaries(&console_output, origin, opts);
            } else {
                for log in logs {
                    let level = log.get("type").and_then(|v| v.as_str()).unwrap_or("log");
                    let text = log.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    println!("{} {}", color::console_level_prefix(level), text);
                }
            }
            return;
        }
        // Errors
        if let Some(errors) = data.get("errors").and_then(|v| v.as_array()) {
            for err in errors {
                let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("");
                println!("{} {}", color::error_indicator(), msg);
            }
            return;
        }
        // Cookies
        if let Some(cookies) = data.get("cookies").and_then(|v| v.as_array()) {
            for cookie in cookies {
                let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let value = cookie.get("value").and_then(|v| v.as_str()).unwrap_or("");
                println!("{}={}", name, value);
            }
            return;
        }
        // Network requests
        if let Some(requests) = data.get("requests").and_then(|v| v.as_array()) {
            if requests.is_empty() {
                println!("No requests captured");
            } else {
                for req in requests {
                    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
                    let url = req.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let resource_type = req
                        .get("resourceType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    println!("{} {} ({})", method, url, resource_type);
                }
            }
            return;
        }
        // Cleared (cookies or request log)
        if let Some(cleared) = data.get("cleared").and_then(|v| v.as_bool()) {
            if cleared {
                let label = match action {
                    Some("cookies_clear") => "Cookies cleared",
                    _ => "Request log cleared",
                };
                println!("{} {}", color::success_indicator(), label);
                return;
            }
        }
        // Bounding box
        if let Some(box_data) = data.get("box") {
            println!(
                "{}",
                serde_json::to_string_pretty(box_data).unwrap_or_default()
            );
            return;
        }
        // Element styles
        if let Some(elements) = data.get("elements").and_then(|v| v.as_array()) {
            for (i, el) in elements.iter().enumerate() {
                let tag = el.get("tag").and_then(|v| v.as_str()).unwrap_or("?");
                let text = el.get("text").and_then(|v| v.as_str()).unwrap_or("");
                println!("[{}] {} \"{}\"", i, tag, text);

                if let Some(box_data) = el.get("box") {
                    let w = box_data.get("width").and_then(|v| v.as_i64()).unwrap_or(0);
                    let h = box_data.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
                    let x = box_data.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
                    let y = box_data.get("y").and_then(|v| v.as_i64()).unwrap_or(0);
                    println!("    box: {}x{} at ({}, {})", w, h, x, y);
                }

                if let Some(styles) = el.get("styles") {
                    let font_size = styles
                        .get("fontSize")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let font_weight = styles
                        .get("fontWeight")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let font_family = styles
                        .get("fontFamily")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let color = styles.get("color").and_then(|v| v.as_str()).unwrap_or("");
                    let bg = styles
                        .get("backgroundColor")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let radius = styles
                        .get("borderRadius")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    println!("    font: {} {} {}", font_size, font_weight, font_family);
                    println!("    color: {}", color);
                    println!("    background: {}", bg);
                    if radius != "0px" {
                        println!("    border-radius: {}", radius);
                    }
                }
                println!();
            }
            return;
        }
        // Closed (browser or tab)
        if data.get("closed").is_some() {
            let label = match action {
                Some("tab_close") => "Tab closed",
                _ => "Browser closed",
            };
            println!("{} {}", color::success_indicator(), label);
            return;
        }
        // Recording start (has "started" field)
        if let Some(started) = data.get("started").and_then(|v| v.as_bool()) {
            if started {
                match action {
                    Some("profiler_start") => {
                        println!("{} Profiling started", color::success_indicator());
                    }
                    _ => {
                        if let Some(path) = data.get("path").and_then(|v| v.as_str()) {
                            println!("{} Recording started: {}", color::success_indicator(), path);
                        } else {
                            println!("{} Recording started", color::success_indicator());
                        }
                    }
                }
                return;
            }
        }
        // Recording restart (has "stopped" field - from recording_restart action)
        if data.get("stopped").is_some() {
            let path = data
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            if let Some(prev_path) = data.get("previousPath").and_then(|v| v.as_str()) {
                println!(
                    "{} Recording restarted: {} (previous saved to {})",
                    color::success_indicator(),
                    path,
                    prev_path
                );
            } else {
                println!("{} Recording started: {}", color::success_indicator(), path);
            }
            return;
        }
        // Recording stop (has "frames" field - from recording_stop action)
        if data.get("frames").is_some() {
            if let Some(path) = data.get("path").and_then(|v| v.as_str()) {
                if let Some(error) = data.get("error").and_then(|v| v.as_str()) {
                    println!(
                        "{} Recording saved to {} - {}",
                        color::warning_indicator(),
                        path,
                        error
                    );
                } else {
                    println!("{} Recording saved to {}", color::success_indicator(), path);
                }
            } else {
                println!("{} Recording stopped", color::success_indicator());
            }
            return;
        }
        // Download response (has "suggestedFilename" or "filename" field)
        if data.get("suggestedFilename").is_some() || data.get("filename").is_some() {
            if let Some(path) = data.get("path").and_then(|v| v.as_str()) {
                let filename = data
                    .get("suggestedFilename")
                    .or_else(|| data.get("filename"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if filename.is_empty() {
                    println!(
                        "{} Downloaded to {}",
                        color::success_indicator(),
                        color::green(path)
                    );
                } else {
                    println!(
                        "{} Downloaded to {} ({})",
                        color::success_indicator(),
                        color::green(path),
                        filename
                    );
                }
                return;
            }
        }
        // Trace stop without path
        if data.get("traceStopped").is_some() {
            println!("{} Trace stopped", color::success_indicator());
            return;
        }
        // Path-based operations (screenshot/pdf/trace/har/download/state/video)
        if let Some(path) = data.get("path").and_then(|v| v.as_str()) {
            match action.unwrap_or("") {
                "screenshot" => {
                    println!(
                        "{} Screenshot saved to {}",
                        color::success_indicator(),
                        color::green(path)
                    );
                    if let Some(annotations) = data.get("annotations").and_then(|v| v.as_array()) {
                        for ann in annotations {
                            let num = ann.get("number").and_then(|n| n.as_u64()).unwrap_or(0);
                            let ref_id = ann.get("ref").and_then(|r| r.as_str()).unwrap_or("");
                            let role = ann.get("role").and_then(|r| r.as_str()).unwrap_or("");
                            let name = ann.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if name.is_empty() {
                                println!(
                                    "   {} @{} {}",
                                    color::dim(&format!("[{}]", num)),
                                    ref_id,
                                    role,
                                );
                            } else {
                                println!(
                                    "   {} @{} {} {:?}",
                                    color::dim(&format!("[{}]", num)),
                                    ref_id,
                                    role,
                                    name,
                                );
                            }
                        }
                    }
                }
                "pdf" => println!(
                    "{} PDF saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
                "trace_stop" => println!(
                    "{} Trace saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
                "profiler_stop" => println!(
                    "{} Profile saved to {} ({} events)",
                    color::success_indicator(),
                    color::green(path),
                    data.get("eventCount").and_then(|c| c.as_u64()).unwrap_or(0)
                ),
                "har_stop" => println!(
                    "{} HAR saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
                "download" | "waitfordownload" => println!(
                    "{} Download saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
                "video_stop" => println!(
                    "{} Video saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
                "state_save" => println!(
                    "{} State saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
                "state_load" => {
                    if let Some(note) = data.get("note").and_then(|v| v.as_str()) {
                        println!("{}", note);
                    }
                    println!(
                        "{} State path set to {}",
                        color::success_indicator(),
                        color::green(path)
                    );
                }
                // video_start and other commands that provide a path with a note
                "video_start" => {
                    if let Some(note) = data.get("note").and_then(|v| v.as_str()) {
                        println!("{}", note);
                    }
                    println!("Path: {}", path);
                }
                _ => println!(
                    "{} Saved to {}",
                    color::success_indicator(),
                    color::green(path)
                ),
            }
            return;
        }

        // State list
        if let Some(files) = data.get("files").and_then(|v| v.as_array()) {
            if let Some(dir) = data.get("directory").and_then(|v| v.as_str()) {
                println!("{}", color::bold(&format!("Saved states in {}", dir)));
            }
            if files.is_empty() {
                println!("{}", color::dim("  No state files found"));
            } else {
                for file in files {
                    let filename = file.get("filename").and_then(|v| v.as_str()).unwrap_or("");
                    let size = file.get("size").and_then(|v| v.as_i64()).unwrap_or(0);
                    let modified = file.get("modified").and_then(|v| v.as_str()).unwrap_or("");
                    let encrypted = file
                        .get("encrypted")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let size_str = if size > 1024 {
                        format!("{:.1}KB", size as f64 / 1024.0)
                    } else {
                        format!("{}B", size)
                    };
                    let date_str = modified.split('T').next().unwrap_or(modified);
                    let enc_str = if encrypted { " [encrypted]" } else { "" };
                    println!(
                        "  {} {}",
                        filename,
                        color::dim(&format!("({}, {}){}", size_str, date_str, enc_str))
                    );
                }
            }
            return;
        }

        // State rename
        if let Some(true) = data.get("renamed").and_then(|v| v.as_bool()) {
            let old_name = data.get("oldName").and_then(|v| v.as_str()).unwrap_or("");
            let new_name = data.get("newName").and_then(|v| v.as_str()).unwrap_or("");
            println!(
                "{} Renamed {} -> {}",
                color::success_indicator(),
                old_name,
                new_name
            );
            return;
        }

        // State clear
        if let Some(cleared) = data.get("cleared").and_then(|v| v.as_i64()) {
            println!(
                "{} Cleared {} state file(s)",
                color::success_indicator(),
                cleared
            );
            return;
        }

        // State show summary
        if let Some(summary) = data.get("summary") {
            let cookies = summary.get("cookies").and_then(|v| v.as_i64()).unwrap_or(0);
            let origins = summary.get("origins").and_then(|v| v.as_i64()).unwrap_or(0);
            let encrypted = data
                .get("encrypted")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let enc_str = if encrypted { " (encrypted)" } else { "" };
            println!("State file summary{}:", enc_str);
            println!("  Cookies: {}", cookies);
            println!("  Origins with localStorage: {}", origins);
            return;
        }

        // State clean
        if let Some(cleaned) = data.get("cleaned").and_then(|v| v.as_i64()) {
            println!(
                "{} Cleaned {} old state file(s)",
                color::success_indicator(),
                cleaned
            );
            return;
        }

        // Informational note
        if let Some(note) = data.get("note").and_then(|v| v.as_str()) {
            println!("{}", note);
            return;
        }
        // Auth list
        if let Some(profiles) = data.get("profiles").and_then(|v| v.as_array()) {
            if profiles.is_empty() {
                println!("{}", color::dim("No auth profiles saved"));
            } else {
                println!("{}", color::bold("Auth profiles:"));
                for p in profiles {
                    let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let url = p.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    let user = p.get("username").and_then(|v| v.as_str()).unwrap_or("");
                    println!(
                        "  {} {} {}",
                        color::green(name),
                        color::dim(user),
                        color::dim(url)
                    );
                }
            }
            return;
        }

        // Auth show
        if let Some(profile) = data.get("profile").and_then(|v| v.as_object()) {
            let name = profile.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let url = profile.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let user = profile
                .get("username")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let created = profile
                .get("createdAt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let last_login = profile.get("lastLoginAt").and_then(|v| v.as_str());
            println!("Name: {}", name);
            println!("URL: {}", url);
            println!("Username: {}", user);
            println!("Created: {}", created);
            if let Some(ll) = last_login {
                println!("Last login: {}", ll);
            }
            return;
        }

        // Auth save/update/login/delete
        if data.get("saved").and_then(|v| v.as_bool()).unwrap_or(false) {
            let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
            println!(
                "{} Auth profile '{}' saved",
                color::success_indicator(),
                name
            );
            return;
        }
        if data
            .get("updated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && !data.get("saved").and_then(|v| v.as_bool()).unwrap_or(false)
        {
            let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
            println!(
                "{} Auth profile '{}' updated",
                color::success_indicator(),
                name
            );
            return;
        }
        if data
            .get("loggedIn")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(title) = data.get("title").and_then(|v| v.as_str()) {
                println!(
                    "{} Logged in as '{}' - {}",
                    color::success_indicator(),
                    name,
                    title
                );
            } else {
                println!("{} Logged in as '{}'", color::success_indicator(), name);
            }
            return;
        }
        if data
            .get("deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
                println!(
                    "{} Auth profile '{}' deleted",
                    color::success_indicator(),
                    name
                );
                return;
            }
        }

        // Confirmation required (for orchestrator use)
        if data
            .get("confirmation_required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let category = data.get("category").and_then(|v| v.as_str()).unwrap_or("");
            let description = data
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cid = data
                .get("confirmation_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("Confirmation required:");
            println!("  {}: {}", category, description);
            println!("  Run: scrapeless-scraping-browser confirm {}", cid);
            println!("  Or:  scrapeless-scraping-browser deny {}", cid);
            return;
        }
        if data
            .get("confirmed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            println!("{} Action confirmed", color::success_indicator());
            return;
        }
        if data
            .get("denied")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            println!("{} Action denied", color::success_indicator());
            return;
        }

        // Default success
        println!("{} Done", color::success_indicator());
    }
}

/// Print command-specific help. Returns true if help was printed, false if command unknown.
pub fn print_command_help(command: &str) -> bool {
    let help = match command {
        // === Navigation ===
        "open" | "goto" | "navigate" => {
            r##"
scrapeless-scraping-browser open - Navigate to a URL

Usage: scrapeless-scraping-browser open <url>

Navigates the browser to the specified URL. If no protocol is provided,
https:// is automatically prepended.

Aliases: goto, navigate

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session
  --headers <json>     Set HTTP headers (scoped to this origin)
  --headed             Show browser window

Examples:
  scrapeless-scraping-browser open example.com
  scrapeless-scraping-browser open https://github.com
  scrapeless-scraping-browser open localhost:3000
  scrapeless-scraping-browser open api.example.com --headers '{"Authorization": "Bearer token"}'
    # ^ Headers only sent to api.example.com, not other domains
"##
        }
        "back" => {
            r##"
scrapeless-scraping-browser back - Navigate back in history

Usage: scrapeless-scraping-browser back

Goes back one page in the browser history, equivalent to clicking
the browser's back button.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser back
"##
        }
        "forward" => {
            r##"
scrapeless-scraping-browser forward - Navigate forward in history

Usage: scrapeless-scraping-browser forward

Goes forward one page in the browser history, equivalent to clicking
the browser's forward button.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser forward
"##
        }
        "reload" => {
            r##"
scrapeless-scraping-browser reload - Reload the current page

Usage: scrapeless-scraping-browser reload

Reloads the current page, equivalent to pressing F5 or clicking
the browser's reload button.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser reload
"##
        }

        // === Core Actions ===
        "click" => {
            r##"
scrapeless-scraping-browser click - Click an element

Usage: scrapeless-scraping-browser click <selector> [--new-tab]

Clicks on the specified element. The selector can be a CSS selector,
XPath, or an element reference from snapshot (e.g., @e1).

Options:
  --new-tab            Open link in a new tab instead of navigating current tab
                       (only works on elements with href attribute)

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser click "#submit-button"
  scrapeless-scraping-browser click @e1
  scrapeless-scraping-browser click "button.primary"
  scrapeless-scraping-browser click "//button[@type='submit']"
  scrapeless-scraping-browser click @e3 --new-tab
"##
        }
        "dblclick" => {
            r##"
scrapeless-scraping-browser dblclick - Double-click an element

Usage: scrapeless-scraping-browser dblclick <selector>

Double-clicks on the specified element. Useful for text selection
or triggering double-click handlers.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser dblclick "#editable-text"
  scrapeless-scraping-browser dblclick @e5
"##
        }
        "fill" => {
            r##"
scrapeless-scraping-browser fill - Clear and fill an input field

Usage: scrapeless-scraping-browser fill <selector> <text>

Clears the input field and fills it with the specified text.
This replaces any existing content in the field.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser fill "#email" "user@example.com"
  scrapeless-scraping-browser fill @e3 "Hello World"
  scrapeless-scraping-browser fill "input[name='search']" "query"
"##
        }
        "type" => {
            r##"
scrapeless-scraping-browser type - Type text into an element

Usage: scrapeless-scraping-browser type <selector> <text>

Types text into the specified element character by character.
Unlike fill, this does not clear existing content first.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser type "#search" "hello"
  scrapeless-scraping-browser type @e2 "additional text"

See Also:
  For typing into contenteditable editors (Lexical, ProseMirror, etc.)
  without a selector, use 'keyboard type' instead:
    scrapeless-scraping-browser keyboard type "# My Heading"
"##
        }
        "hover" => {
            r##"
scrapeless-scraping-browser hover - Hover over an element

Usage: scrapeless-scraping-browser hover <selector>

Moves the mouse to hover over the specified element. Useful for
triggering hover states or dropdown menus.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser hover "#dropdown-trigger"
  scrapeless-scraping-browser hover @e4
"##
        }
        "focus" => {
            r##"
scrapeless-scraping-browser focus - Focus an element

Usage: scrapeless-scraping-browser focus <selector>

Sets keyboard focus to the specified element.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser focus "#input-field"
  scrapeless-scraping-browser focus @e2
"##
        }
        "check" => {
            r##"
scrapeless-scraping-browser check - Check a checkbox

Usage: scrapeless-scraping-browser check <selector>

Checks a checkbox element. If already checked, no action is taken.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser check "#terms-checkbox"
  scrapeless-scraping-browser check @e7
"##
        }
        "uncheck" => {
            r##"
scrapeless-scraping-browser uncheck - Uncheck a checkbox

Usage: scrapeless-scraping-browser uncheck <selector>

Unchecks a checkbox element. If already unchecked, no action is taken.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser uncheck "#newsletter-opt-in"
  scrapeless-scraping-browser uncheck @e8
"##
        }
        "select" => {
            r##"
scrapeless-scraping-browser select - Select a dropdown option

Usage: scrapeless-scraping-browser select <selector> <value...>

Selects one or more options in a <select> dropdown by value.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser select "#country" "US"
  scrapeless-scraping-browser select @e5 "option2"
  scrapeless-scraping-browser select "#menu" "opt1" "opt2" "opt3"
"##
        }
        "drag" => {
            r##"
scrapeless-scraping-browser drag - Drag and drop

Usage: scrapeless-scraping-browser drag <source> <target>

Drags an element from source to target location.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser drag "#draggable" "#drop-zone"
  scrapeless-scraping-browser drag @e1 @e2
"##
        }
        "upload" => {
            r##"
scrapeless-scraping-browser upload - Upload files

Usage: scrapeless-scraping-browser upload <selector> <files...>

Uploads one or more files to a file input element.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser upload "#file-input" ./document.pdf
  scrapeless-scraping-browser upload @e3 ./image1.png ./image2.png
"##
        }
        "download" => {
            r##"
scrapeless-scraping-browser download - Download a file by clicking an element

Usage: scrapeless-scraping-browser download <selector> <path>

Clicks an element that triggers a download and saves the file to the specified path.

Arguments:
  selector             Element to click (CSS selector or @ref)
  path                 Path where the downloaded file will be saved

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser download "#download-btn" ./file.pdf
  scrapeless-scraping-browser download @e5 ./report.xlsx
  scrapeless-scraping-browser download "a[href$='.zip']" ./archive.zip
"##
        }

        // === Keyboard ===
        "press" | "key" => {
            r##"
scrapeless-scraping-browser press - Press a key or key combination

Usage: scrapeless-scraping-browser press <key>

Presses a key or key combination. Supports special keys and modifiers.

Aliases: key

Special Keys:
  Enter, Tab, Escape, Backspace, Delete, Space
  ArrowUp, ArrowDown, ArrowLeft, ArrowRight
  Home, End, PageUp, PageDown
  F1-F12

Modifiers (combine with +):
  Control, Alt, Shift, Meta

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser press Enter
  scrapeless-scraping-browser press Tab
  scrapeless-scraping-browser press Control+a
  scrapeless-scraping-browser press Control+Shift+s
  scrapeless-scraping-browser press Escape
"##
        }
        "keydown" => {
            r##"
scrapeless-scraping-browser keydown - Press a key down (without release)

Usage: scrapeless-scraping-browser keydown <key>

Presses a key down without releasing it. Use keyup to release.
Useful for holding modifier keys.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser keydown Shift
  scrapeless-scraping-browser keydown Control
"##
        }
        "keyup" => {
            r##"
scrapeless-scraping-browser keyup - Release a key

Usage: scrapeless-scraping-browser keyup <key>

Releases a key that was pressed with keydown.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser keyup Shift
  scrapeless-scraping-browser keyup Control
"##
        }
        "keyboard" => {
            r##"
scrapeless-scraping-browser keyboard - Raw keyboard input (no selector needed)

Usage: scrapeless-scraping-browser keyboard <subcommand> <text>

Sends keyboard input to whatever element currently has focus.
Unlike 'type' which requires a selector, 'keyboard' operates on
the current focus — essential for contenteditable editors like
Lexical, ProseMirror, CodeMirror, and Monaco.

Subcommands:
  type <text>          Type text character-by-character with real
                       key events (keydown, keypress, keyup per char)
  inserttext <text>    Insert text without key events (like paste)

Note: For key combos (Enter, Control+a), use the 'press' command
directly — it already operates on the current focus.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser keyboard type "Hello, World!"
  scrapeless-scraping-browser keyboard type "# My Heading"
  scrapeless-scraping-browser keyboard inserttext "pasted content"

Use Cases:
  # Type into a Lexical/ProseMirror contenteditable editor:
  scrapeless-scraping-browser click "[contenteditable]"
  scrapeless-scraping-browser keyboard type "# My Heading"
  scrapeless-scraping-browser press Enter
  scrapeless-scraping-browser keyboard type "Some paragraph text"
"##
        }

        // === Scroll ===
        "scroll" => {
            r##"
scrapeless-scraping-browser scroll - Scroll the page

Usage: scrapeless-scraping-browser scroll [direction] [amount] [options]

Scrolls the page or a specific element in the specified direction.

Arguments:
  direction            up, down, left, right (default: down)
  amount               Pixels to scroll (default: 300)

Options:
  -s, --selector <sel> CSS selector for a scrollable container

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser scroll
  scrapeless-scraping-browser scroll down 500
  scrapeless-scraping-browser scroll up 200
  scrapeless-scraping-browser scroll left 100
  scrapeless-scraping-browser scroll down 500 --selector "div.scroll-container"
"##
        }
        "scrollintoview" | "scrollinto" => {
            r##"
scrapeless-scraping-browser scrollintoview - Scroll element into view

Usage: scrapeless-scraping-browser scrollintoview <selector>

Scrolls the page until the specified element is visible in the viewport.

Aliases: scrollinto

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser scrollintoview "#footer"
  scrapeless-scraping-browser scrollintoview @e15
"##
        }

        // === Wait ===
        "wait" => {
            r##"
scrapeless-scraping-browser wait - Wait for condition

Usage: scrapeless-scraping-browser wait <selector|ms|option>

Waits for an element to appear, a timeout, or other conditions.

Modes:
  <selector>           Wait for element to appear
  <ms>                 Wait for specified milliseconds
  --url <pattern>      Wait for URL to match pattern
  --load <state>       Wait for load state (load, domcontentloaded, networkidle)
  --fn <expression>    Wait for JavaScript expression to be truthy
  --text <text>        Wait for text to appear on page
  --download [path]    Wait for a download to complete (optionally save to path)

Download Options (with --download):
  --timeout <ms>       Timeout in milliseconds for download to start

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser wait "#loading-spinner"
  scrapeless-scraping-browser wait 2000
  scrapeless-scraping-browser wait --url "**/dashboard"
  scrapeless-scraping-browser wait --load networkidle
  scrapeless-scraping-browser wait --fn "window.appReady === true"
  scrapeless-scraping-browser wait --text "Welcome back"
  scrapeless-scraping-browser wait --download ./file.pdf
  scrapeless-scraping-browser wait --download ./report.xlsx --timeout 30000
"##
        }

        // === Screenshot/PDF ===
        "screenshot" => {
            r##"
scrapeless-scraping-browser screenshot - Take a screenshot

Usage: scrapeless-scraping-browser screenshot [path]

Captures a screenshot of the current page. If no path is provided,
saves to a temporary directory with a generated filename.

Options:
  --full, -f           Capture full page (not just viewport)
  --annotate           Overlay numbered labels on interactive elements.
                       Each label [N] corresponds to ref @eN from snapshot.
                       Prints a legend mapping labels to element roles/names.
                       With --json, annotations are included in the response.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser screenshot
  scrapeless-scraping-browser screenshot ./screenshot.png
  scrapeless-scraping-browser screenshot --full ./full-page.png
  scrapeless-scraping-browser screenshot --annotate              # Labeled screenshot + legend
  scrapeless-scraping-browser screenshot --annotate ./page.png   # Save annotated screenshot
  scrapeless-scraping-browser screenshot --annotate --json       # JSON output with annotations
"##
        }
        "pdf" => {
            r##"
scrapeless-scraping-browser pdf - Save page as PDF

Usage: scrapeless-scraping-browser pdf <path>

Saves the current page as a PDF file.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser pdf ./page.pdf
  scrapeless-scraping-browser pdf ~/Documents/report.pdf
"##
        }

        // === Snapshot ===
        "snapshot" => {
            r##"
scrapeless-scraping-browser snapshot - Get accessibility tree snapshot

Usage: scrapeless-scraping-browser snapshot [options]

Returns an accessibility tree representation of the page with element
references (like @e1, @e2) that can be used in subsequent commands.
Designed for AI agents to understand page structure.

Options:
  -i, --interactive    Only include interactive elements
  -C, --cursor         Include cursor-interactive elements (cursor:pointer, onclick, tabindex)
  -c, --compact        Remove empty structural elements
  -d, --depth <n>      Limit tree depth
  -s, --selector <sel> Scope snapshot to CSS selector

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser snapshot
  scrapeless-scraping-browser snapshot -i
  scrapeless-scraping-browser snapshot -i -C         # Interactive + cursor-interactive elements
  scrapeless-scraping-browser snapshot --compact --depth 5
  scrapeless-scraping-browser snapshot -s "#main-content"
"##
        }

        // === Eval ===
        "eval" => {
            r##"
scrapeless-scraping-browser eval - Execute JavaScript

Usage: scrapeless-scraping-browser eval [options] <script>

Executes JavaScript code in the browser context and returns the result.

Options:
  -b, --base64         Decode script from base64 (avoids shell escaping issues)
  --stdin              Read script from stdin (useful for heredocs/multiline)

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser eval "document.title"
  scrapeless-scraping-browser eval "window.location.href"
  scrapeless-scraping-browser eval "document.querySelectorAll('a').length"
  scrapeless-scraping-browser eval -b "ZG9jdW1lbnQudGl0bGU="

  # Read from stdin with heredoc
  cat <<'EOF' | scrapeless-scraping-browser eval --stdin
  const links = document.querySelectorAll('a');
  links.length;
  EOF
"##
        }

        // === Close ===
        "close" | "quit" | "exit" => {
            r##"
scrapeless-scraping-browser close - Close the browser

Usage: scrapeless-scraping-browser close

Closes the browser instance for the current session.

Aliases: quit, exit

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser close
  scrapeless-scraping-browser close --session mysession
"##
        }

        // === Get ===
        "get" => {
            r##"
scrapeless-scraping-browser get - Retrieve information from elements or page

Usage: scrapeless-scraping-browser get <subcommand> [args]

Retrieves various types of information from elements or the page.

Subcommands:
  text <selector>            Get text content of element
  html <selector>            Get inner HTML of element
  value <selector>           Get value of input element
  attr <selector> <name>     Get attribute value
  title                      Get page title
  url                        Get current URL
  count <selector>           Count matching elements
  box <selector>             Get bounding box (x, y, width, height)
  styles <selector>          Get computed styles of elements

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser get text @e1
  scrapeless-scraping-browser get html "#content"
  scrapeless-scraping-browser get value "#email-input"
  scrapeless-scraping-browser get attr "#link" href
  scrapeless-scraping-browser get title
  scrapeless-scraping-browser get url
  scrapeless-scraping-browser get count "li.item"
  scrapeless-scraping-browser get box "#header"
  scrapeless-scraping-browser get styles "button"
  scrapeless-scraping-browser get styles @e1
"##
        }

        // === Is ===
        "is" => {
            r##"
scrapeless-scraping-browser is - Check element state

Usage: scrapeless-scraping-browser is <subcommand> <selector>

Checks the state of an element and returns true/false.

Subcommands:
  visible <selector>   Check if element is visible
  enabled <selector>   Check if element is enabled (not disabled)
  checked <selector>   Check if checkbox/radio is checked

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser is visible "#modal"
  scrapeless-scraping-browser is enabled "#submit-btn"
  scrapeless-scraping-browser is checked "#agree-checkbox"
"##
        }

        // === Find ===
        "find" => {
            r##"
scrapeless-scraping-browser find - Find and interact with elements by locator

Usage: scrapeless-scraping-browser find <locator> <value> [action] [text]

Finds elements using semantic locators and optionally performs an action.

Locators:
  role <role>              Find by ARIA role (--name <n>, --exact)
  text <text>              Find by text content (--exact)
  label <label>            Find by associated label (--exact)
  placeholder <text>       Find by placeholder text (--exact)
  alt <text>               Find by alt text (--exact)
  title <text>             Find by title attribute (--exact)
  testid <id>              Find by data-testid attribute
  first <selector>         First matching element
  last <selector>          Last matching element
  nth <index> <selector>   Nth matching element (0-based)

Actions (default: click):
  click, fill, type, hover, focus, check, uncheck

Options:
  --name <name>        Filter role by accessible name
  --exact              Require exact text match

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser find role button click --name Submit
  scrapeless-scraping-browser find text "Sign In" click
  scrapeless-scraping-browser find label "Email" fill "user@example.com"
  scrapeless-scraping-browser find placeholder "Search..." type "query"
  scrapeless-scraping-browser find testid "login-form" click
  scrapeless-scraping-browser find first "li.item" click
  scrapeless-scraping-browser find nth 2 ".card" hover
"##
        }

        // === Mouse ===
        "mouse" => {
            r##"
scrapeless-scraping-browser mouse - Low-level mouse operations

Usage: scrapeless-scraping-browser mouse <subcommand> [args]

Performs low-level mouse operations for precise control.

Subcommands:
  move <x> <y>         Move mouse to coordinates
  down [button]        Press mouse button (left, right, middle)
  up [button]          Release mouse button
  wheel <dy> [dx]      Scroll mouse wheel

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser mouse move 100 200
  scrapeless-scraping-browser mouse down
  scrapeless-scraping-browser mouse up
  scrapeless-scraping-browser mouse down right
  scrapeless-scraping-browser mouse wheel 100
  scrapeless-scraping-browser mouse wheel -50 0
"##
        }

        // === Set ===
        "set" => {
            r##"
scrapeless-scraping-browser set - Configure browser settings

Usage: scrapeless-scraping-browser set <setting> [args]

Configures various browser settings and emulation options.

Settings:
  viewport <w> <h> [scale]   Set viewport size (scale = deviceScaleFactor, e.g. 2 for retina)
  device <name>              Emulate device (e.g., "iPhone 12")
  geo <lat> <lng>            Set geolocation
  offline [on|off]           Toggle offline mode
  headers <json>             Set extra HTTP headers
  credentials <user> <pass>  Set HTTP authentication
  media [dark|light]         Set color scheme preference
        [reduced-motion]     Enable reduced motion

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser set viewport 1920 1080
  scrapeless-scraping-browser set viewport 1920 1080 2    # 2x retina
  scrapeless-scraping-browser set device "iPhone 12"
  scrapeless-scraping-browser set geo 37.7749 -122.4194
  scrapeless-scraping-browser set offline on
  scrapeless-scraping-browser set headers '{"X-Custom": "value"}'
  scrapeless-scraping-browser set credentials admin secret123
  scrapeless-scraping-browser set media dark
  scrapeless-scraping-browser set media light reduced-motion
"##
        }

        // === Network ===
        "network" => {
            r##"
scrapeless-scraping-browser network - Network interception and monitoring

Usage: scrapeless-scraping-browser network <subcommand> [args]

Intercept, mock, or monitor network requests.

Subcommands:
  route <url> [options]      Intercept requests matching URL pattern
    --abort                  Abort matching requests
    --body <json>            Respond with custom body
  unroute [url]              Remove route (all if no URL)
  requests [options]         List captured requests
    --clear                  Clear request log
    --filter <pattern>       Filter by URL pattern

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser network route "**/api/*" --abort
  scrapeless-scraping-browser network route "**/data.json" --body '{"mock": true}'
  scrapeless-scraping-browser network unroute
  scrapeless-scraping-browser network requests
  scrapeless-scraping-browser network requests --filter "api"
  scrapeless-scraping-browser network requests --clear
"##
        }

        // === Storage ===
        "storage" => {
            r##"
scrapeless-scraping-browser storage - Manage web storage

Usage: scrapeless-scraping-browser storage <type> [operation] [key] [value]

Manage localStorage and sessionStorage.

Types:
  local                localStorage
  session              sessionStorage

Operations:
  get [key]            Get all storage or specific key
  set <key> <value>    Set a key-value pair
  clear                Clear all storage

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser storage local
  scrapeless-scraping-browser storage local get authToken
  scrapeless-scraping-browser storage local set theme "dark"
  scrapeless-scraping-browser storage local clear
  scrapeless-scraping-browser storage session get userId
"##
        }

        // === Cookies ===
        "cookies" => {
            r##"
scrapeless-scraping-browser cookies - Manage browser cookies

Usage: scrapeless-scraping-browser cookies [operation] [args]

Manage browser cookies for the current context.

Operations:
  get                                Get all cookies (default)
  set <name> <value> [options]       Set a cookie with optional properties
  clear                              Clear all cookies

Cookie Set Options:
  --url <url>                        URL for the cookie (allows setting before page load)
  --domain <domain>                  Cookie domain (e.g., ".example.com")
  --path <path>                      Cookie path (e.g., "/api")
  --httpOnly                         Set HttpOnly flag (prevents JavaScript access)
  --secure                           Set Secure flag (HTTPS only)
  --sameSite <Strict|Lax|None>       SameSite policy
  --expires <timestamp>              Expiration time (Unix timestamp in seconds)

Note: If --url, --domain, and --path are all omitted, the cookie will be set
for the current page URL.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  # Simple cookie for current page
  scrapeless-scraping-browser cookies set session_id "abc123"

  # Set cookie for a URL before loading it (useful for authentication)
  scrapeless-scraping-browser cookies set session_id "abc123" --url https://app.example.com

  # Set secure, httpOnly cookie with domain and path
  scrapeless-scraping-browser cookies set auth_token "xyz789" --domain example.com --path /api --httpOnly --secure

  # Set cookie with SameSite policy
  scrapeless-scraping-browser cookies set tracking_consent "yes" --sameSite Strict

  # Set cookie with expiration (Unix timestamp)
  scrapeless-scraping-browser cookies set temp_token "temp123" --expires 1735689600

  # Get all cookies
  scrapeless-scraping-browser cookies

  # Clear all cookies
  scrapeless-scraping-browser cookies clear
"##
        }

        // === Tabs ===
        "tab" => {
            r##"
scrapeless-scraping-browser tab - Manage browser tabs

Usage: scrapeless-scraping-browser tab [operation] [args]

Manage browser tabs in the current window.

Operations:
  list                 List all tabs (default)
  new [url]            Open new tab
  close [index]        Close tab (current if no index)
  <index>              Switch to tab by index

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser tab
  scrapeless-scraping-browser tab list
  scrapeless-scraping-browser tab new
  scrapeless-scraping-browser tab new https://example.com
  scrapeless-scraping-browser tab 2
  scrapeless-scraping-browser tab close
  scrapeless-scraping-browser tab close 1
"##
        }

        // === Window ===
        "window" => {
            r##"
scrapeless-scraping-browser window - Manage browser windows

Usage: scrapeless-scraping-browser window <operation>

Manage browser windows.

Operations:
  new                  Open new browser window

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser window new
"##
        }

        // === Frame ===
        "frame" => {
            r##"
scrapeless-scraping-browser frame - Switch frame context

Usage: scrapeless-scraping-browser frame <selector|main>

Switch to an iframe or back to the main frame.

Arguments:
  <selector>           CSS selector for iframe
  main                 Switch back to main frame

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser frame "#embed-iframe"
  scrapeless-scraping-browser frame "iframe[name='content']"
  scrapeless-scraping-browser frame main
"##
        }

        // === Config ===
        "config" => {
            r##"
scrapeless-scraping-browser config - Manage configuration

Usage: scrapeless-scraping-browser config <subcommand> [args]

Manage persistent configuration settings stored in ~/.scrapeless/config.json.
Configuration values have priority: config file > environment variables.

Subcommands:
  set <key> <value>        Set configuration value
  get <key>                Get configuration value
  list                     List all configuration values
  remove <key>             Remove configuration value

Supported Keys:
  key                      API key (same as SCRAPELESS_API_KEY)
  apiVersion               API version (same as SCRAPELESS_API_VERSION)
  sessionTtl               Session timeout in milliseconds (same as SCRAPELESS_SESSION_TTL)
  sessionName              Session name (same as SCRAPELESS_SESSION_NAME)
  sessionRecording         Enable session recording (same as SCRAPELESS_SESSION_RECORDING)
  proxyUrl                 Custom proxy URL (same as SCRAPELESS_PROXY_URL)
  proxyCountry             Proxy country code (same as SCRAPELESS_PROXY_COUNTRY)
  proxyState               Proxy state/province (same as SCRAPELESS_PROXY_STATE)
  proxyCity                Proxy city (same as SCRAPELESS_PROXY_CITY)
  fingerprint              Browser fingerprint JSON (same as SCRAPELESS_FINGERPRINT)
  debug                    Enable debug logging (same as SCRAPELESS_DEBUG)

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  # Set API key (recommended method)
  scrapeless-scraping-browser config set key your_api_key_here

  # Set proxy country
  scrapeless-scraping-browser config set proxyCountry US

  # Set session timeout (5 minutes)
  scrapeless-scraping-browser config set sessionTtl 300000

  # List all configuration
  scrapeless-scraping-browser config list

  # Get specific value
  scrapeless-scraping-browser config get key

  # Remove configuration
  scrapeless-scraping-browser config remove proxyCountry

Configuration File:
  Location: ~/.scrapeless/config.json
  Permissions: 0600 (user read/write only)
  Format: JSON with typed values
"##
        }

        // === Auth ===
        "auth" => {
            r##"
scrapeless-scraping-browser auth - Manage authentication profiles

Usage: scrapeless-scraping-browser auth <subcommand> [args]

Subcommands:
  save <name>              Save credentials for a login profile
  login <name>             Login using saved credentials
  list                     List saved profiles (names and URLs only)
  show <name>              Show profile metadata (no passwords)
  delete <name>            Delete a saved profile

Save Options:
  --url <url>              Login page URL (required)
  --username <user>        Username (required)
  --password <pass>        Password (required unless --password-stdin)
  --password-stdin          Read password from stdin (recommended)
  --username-selector <s>  Custom CSS selector for username field
  --password-selector <s>  Custom CSS selector for password field
  --submit-selector <s>    Custom CSS selector for submit button

Global Options:
  --json                   Output as JSON
  --session <name>         Use specific session

Examples:
  echo "pass" | scrapeless-scraping-browser auth save github --url https://github.com/login --username user --password-stdin
  scrapeless-scraping-browser auth save github --url https://github.com/login --username user --password pass
  scrapeless-scraping-browser auth login github
  scrapeless-scraping-browser auth list
  scrapeless-scraping-browser auth show github
  scrapeless-scraping-browser auth delete github
"##
        }

        // === Confirm/Deny ===
        "confirm" | "deny" => {
            r##"
scrapeless-scraping-browser confirm/deny - Approve or deny pending actions

Usage:
  scrapeless-scraping-browser confirm <confirmation-id>
  scrapeless-scraping-browser deny <confirmation-id>

When --confirm-actions is set, certain action categories return a
confirmation_required response with a confirmation ID. Use confirm/deny
to approve or reject the action.

Pending confirmations auto-deny after 60 seconds.

Examples:
  scrapeless-scraping-browser confirm c_8f3a1234
  scrapeless-scraping-browser deny c_8f3a1234
"##
        }

        // === Dialog ===
        "dialog" => {
            r##"
scrapeless-scraping-browser dialog - Handle browser dialogs

Usage: scrapeless-scraping-browser dialog <response> [text]

Respond to browser dialogs (alert, confirm, prompt).

Operations:
  accept [text]        Accept dialog, optionally with prompt text
  dismiss              Dismiss/cancel dialog

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser dialog accept
  scrapeless-scraping-browser dialog accept "my input"
  scrapeless-scraping-browser dialog dismiss
"##
        }

        // === Trace ===
        "trace" => {
            r##"
scrapeless-scraping-browser trace - Record execution trace

Usage: scrapeless-scraping-browser trace <operation> [path]

Record a trace for debugging with Playwright Trace Viewer.

Operations:
  start [path]         Start recording trace
  stop [path]          Stop recording and save trace

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser trace start
  scrapeless-scraping-browser trace start ./my-trace
  scrapeless-scraping-browser trace stop
  scrapeless-scraping-browser trace stop ./debug-trace.zip
"##
        }

        // === Profile (CDP Tracing) ===
        "profiler" => {
            r##"
scrapeless-scraping-browser profiler - Record Chrome DevTools performance profile

Usage: scrapeless-scraping-browser profiler <operation> [options]

Record a performance profile using Chrome DevTools Protocol (CDP) Tracing.
The output JSON file can be loaded into Chrome DevTools Performance panel,
Perfetto UI (https://ui.perfetto.dev/), or other trace analysis tools.

Operations:
  start                Start profiling
  stop [path]          Stop profiling and save to file

Start Options:
  --categories <list>  Comma-separated trace categories (default includes
                       devtools.timeline, v8.execute, blink, and others)

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  # Basic profiling
  scrapeless-scraping-browser profiler start
  scrapeless-scraping-browser navigate https://example.com
  scrapeless-scraping-browser click "#button"
  scrapeless-scraping-browser profiler stop ./trace.json

  # With custom categories
  scrapeless-scraping-browser profiler start --categories "devtools.timeline,v8.execute,blink.user_timing"
  scrapeless-scraping-browser profiler stop ./custom-trace.json

The output file can be viewed in:
  - Chrome DevTools: Performance panel > Load profile
  - Perfetto: https://ui.perfetto.dev/
"##
        }

        // === Record (video) ===
        "record" => {
            r##"
scrapeless-scraping-browser record - Record browser session to video

Usage: scrapeless-scraping-browser record start <path.webm> [url]
       scrapeless-scraping-browser record stop
       scrapeless-scraping-browser record restart <path.webm> [url]

Record the browser to a WebM video file using Playwright's native recording.
Creates a fresh browser context but preserves cookies and localStorage.
If no URL is provided, automatically navigates to your current page.

Operations:
  start <path> [url]     Start recording (defaults to current URL if omitted)
  stop                   Stop recording and save video
  restart <path> [url]   Stop current recording (if any) and start a new one

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  # Record from current page (preserves login state)
  scrapeless-scraping-browser open https://app.example.com/dashboard
  scrapeless-scraping-browser snapshot -i            # Explore and plan
  scrapeless-scraping-browser record start ./demo.webm
  scrapeless-scraping-browser click @e3              # Execute planned actions
  scrapeless-scraping-browser record stop

  # Or specify a different URL
  scrapeless-scraping-browser record start ./demo.webm https://example.com

  # Restart recording with a new file (stops previous, starts new)
  scrapeless-scraping-browser record restart ./take2.webm
"##
        }

        // === Console/Errors ===
        "console" => {
            r##"
scrapeless-scraping-browser console - View console logs

Usage: scrapeless-scraping-browser console [--clear]

View browser console output (log, warn, error, info).

Options:
  --clear              Clear console log buffer

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser console
  scrapeless-scraping-browser console --clear
"##
        }
        "errors" => {
            r##"
scrapeless-scraping-browser errors - View page errors

Usage: scrapeless-scraping-browser errors [--clear]

View JavaScript errors and uncaught exceptions.

Options:
  --clear              Clear error buffer

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser errors
  scrapeless-scraping-browser errors --clear
"##
        }

        // === Highlight ===
        "highlight" => {
            r##"
scrapeless-scraping-browser highlight - Highlight an element

Usage: scrapeless-scraping-browser highlight <selector>

Visually highlights an element on the page for debugging.

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser highlight "#target-element"
  scrapeless-scraping-browser highlight @e5
"##
        }

        // === State ===
        "state" => {
            r##"
scrapeless-scraping-browser state - Manage browser state

Usage: scrapeless-scraping-browser state <operation> [args]

Save, restore, list, and manage browser state (cookies, localStorage, sessionStorage).

Operations:
  save <path>                        Save current state to file
  load <path>                        Load state from file
  list                               List saved state files
  show <filename>                    Show state summary
  rename <old-name> <new-name>       Rename state file
  clear [session-name] [--all]       Clear saved states
  clean --older-than <days>          Delete expired state files

Automatic State Persistence:
  Use --session-name to auto-save/restore state across restarts:
  scrapeless-scraping-browser --session-name myapp open https://example.com
  Or set SCRAPELESS_BROWSER_SESSION_NAME environment variable.

State Encryption:
  Set SCRAPELESS_BROWSER_ENCRYPTION_KEY (64-char hex) for AES-256-GCM encryption.
  Generate a key: openssl rand -hex 32

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser state save ./auth-state.json
  scrapeless-scraping-browser state load ./auth-state.json
  scrapeless-scraping-browser state list
  scrapeless-scraping-browser state show myapp-default.json
  scrapeless-scraping-browser state rename old-name new-name
  scrapeless-scraping-browser state clear --all
  scrapeless-scraping-browser state clean --older-than 7
"##
        }

        // === Session ===
        "session" => {
            r##"
scrapeless-scraping-browser session - Manage sessions

Usage: scrapeless-scraping-browser session [operation]

Manage isolated browser sessions. Each session has its own browser
instance with separate cookies, storage, and state.

Operations:
  (none)               Show current session name
  list                 List all active sessions

Environment:
  SCRAPELESS_BROWSER_SESSION    Default session name

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser session
  scrapeless-scraping-browser session list
  scrapeless-scraping-browser --session test open example.com
"##
        }

        "diff" => {
            r##"
scrapeless-scraping-browser diff - Compare page states

Subcommands:

  diff snapshot                   Compare current snapshot to last snapshot in session
  diff screenshot --baseline <f>  Visual pixel diff against a baseline image
  diff url <url1> <url2>          Compare two pages

Snapshot Diff:

  Usage: scrapeless-scraping-browser diff snapshot [options]

  Options:
    -b, --baseline <file>    Compare against a saved snapshot file
    -s, --selector <sel>     Scope snapshot to a CSS selector or @ref
    -c, --compact            Use compact snapshot format
    -d, --depth <n>          Limit snapshot tree depth

  Without --baseline, compares against the last snapshot taken in this session.

Screenshot Diff:

  Usage: scrapeless-scraping-browser diff screenshot --baseline <file> [options]

  Options:
    -b, --baseline <file>    Baseline image to compare against (required)
    -o, --output <file>      Path for the diff image (default: temp dir)
    -t, --threshold <0-1>    Color distance threshold (default: 0.1)
    -s, --selector <sel>     Scope screenshot to element
        --full               Full page screenshot

URL Diff:

  Usage: scrapeless-scraping-browser diff url <url1> <url2> [options]

  Options:
    --screenshot             Also compare screenshots (default: snapshot only)
    --full                   Full page screenshots
    --wait-until <strategy>  Navigation wait strategy: load, domcontentloaded, networkidle (default: load)
    -s, --selector <sel>     Scope snapshots to a CSS selector or @ref
    -c, --compact            Use compact snapshot format
    -d, --depth <n>          Limit snapshot tree depth

Global Options:
  --json               Output as JSON
  --session-id <id>    Connect to specific Scrapeless session

Examples:
  scrapeless-scraping-browser diff snapshot
  scrapeless-scraping-browser diff snapshot --baseline before.txt
  scrapeless-scraping-browser diff screenshot --baseline before.png
  scrapeless-scraping-browser diff screenshot --baseline before.png --output diff.png --threshold 0.2
  scrapeless-scraping-browser diff url https://staging.example.com https://prod.example.com
  scrapeless-scraping-browser diff url https://v1.example.com https://v2.example.com --screenshot
"##
        }

        // === Scrapeless Session Management ===
        "new-session" => {
            r##"
scrapeless-scraping-browser new-session - Create a new Scrapeless browser session

Usage: scrapeless-scraping-browser new-session [options]

Creates a new browser session in the Scrapeless cloud platform with customizable
parameters for proxy location, browser configuration, and session settings.

Options:
  --name <name>          Session name for identification
  --ttl <seconds>        Session timeout in seconds (default: 180)
  --recording <bool>     Enable session recording (true/false)
  --proxy-country <code> Proxy country code (e.g., AU, US, GB)
  --proxy-state <state>  Proxy state/region (e.g., NSW, CA, NY)
  --proxy-city <city>    Proxy city (e.g., sydney, newyork, london)
  --user-agent <ua>      Custom user agent string
  --platform <platform>  Platform (Windows, macOS, Linux)
  --screen-width <px>    Screen width in pixels (default: 1920)
  --screen-height <px>   Screen height in pixels (default: 1080)
  --timezone <tz>        Timezone (default: America/New_York)
  --languages <langs>    Comma-separated language codes (default: en)

Global Options:
  --json               Output as JSON

Examples:
  # Create a basic session
  scrapeless-scraping-browser new-session

  # Create a named session with custom timeout
  scrapeless-scraping-browser new-session --name "test-session" --ttl 300

  # Create session with Australian proxy
  scrapeless-scraping-browser new-session --proxy-country AU --proxy-city sydney

  # Create session with custom browser configuration
  scrapeless-scraping-browser new-session --platform macOS --screen-width 1440 --screen-height 900

  # Create session with multiple languages
  scrapeless-scraping-browser new-session --languages "en,es,fr" --timezone "Europe/Madrid"

Returns:
  taskId - Unique identifier for the created session
  success - Boolean indicating if creation was successful

Note: Requires SCRAPELESS_API_KEY to be configured via environment variable
or config file (scrapeless-scraping-browser config set key YOUR_API_KEY).
"##
        }

        "sessions" => {
            r##"
scrapeless-scraping-browser sessions - List running browser sessions

Usage: scrapeless-scraping-browser sessions

Lists all currently running browser sessions in your Scrapeless account,
showing their status, creation time, and metadata.

Global Options:
  --json               Output as JSON

Examples:
  scrapeless-scraping-browser sessions

Returns:
  For each session:
  - taskId: Unique session identifier
  - state: Current session status (processing, stopped, etc.)
  - createTime: When the session was created
  - sessionName: Custom name if provided during creation
  - metadata: Additional session information

Note: Requires SCRAPELESS_API_KEY to be configured.
"##
        }

        _ => return false,
    };
    println!("{}", help.trim());
    true
}

pub fn print_help() {
    println!(
        r#"
scrapeless-scraping-browser - cloud browser automation CLI for AI agents

Usage: scrapeless-scraping-browser <command> [args] [options]

Core Commands:
  open <url>                 Navigate to URL
  click <sel>                Click element (or @ref)
  dblclick <sel>             Double-click element
  type <sel> <text>          Type into element
  fill <sel> <text>          Clear and fill
  press <key>                Press key (Enter, Tab, Control+a)
  keyboard type <text>       Type text with real keystrokes (no selector)
  keyboard inserttext <text> Insert text without key events
  hover <sel>                Hover element
  focus <sel>                Focus element
  check <sel>                Check checkbox
  uncheck <sel>              Uncheck checkbox
  select <sel> <val...>      Select dropdown option
  drag <src> <dst>           Drag and drop
  upload <sel> <files...>    Upload files
  download <sel> <path>      Download file by clicking element
  scroll <dir> [px]          Scroll (up/down/left/right)
  scrollintoview <sel>       Scroll element into view
  wait <sel|ms>              Wait for element or time
  screenshot [path]          Take screenshot
  pdf <path>                 Save as PDF
  snapshot                   Accessibility tree with refs (for AI)
  eval <js>                  Run JavaScript
  close                      Close browser

Navigation:
  back                       Go back
  forward                    Go forward
  reload                     Reload page

Get Info:  scrapeless-scraping-browser get <what> [selector]
  text, html, value, attr <name>, title, url, count, box, styles

Check State:  scrapeless-scraping-browser is <what> <selector>
  visible, enabled, checked

Find Elements:  scrapeless-scraping-browser find <locator> <value> <action> [text]
  role, text, label, placeholder, alt, title, testid, first, last, nth

Mouse:  scrapeless-scraping-browser mouse <action> [args]
  move <x> <y>, down [btn], up [btn], wheel <dy> [dx]

Browser Settings:  scrapeless-scraping-browser set <setting> [value]
  viewport <w> <h>, device <name>, geo <lat> <lng>
  offline [on|off], headers <json>, credentials <user> <pass>
  media [dark|light] [reduced-motion]

Network:  scrapeless-scraping-browser network <action>
  route <url> [--abort|--body <json>]
  unroute [url]
  requests [--clear] [--filter <pattern>]

Storage:
  cookies [get|set|clear]    Manage cookies (set supports --url, --domain, --path, --httpOnly, --secure, --sameSite, --expires)
  storage <local|session>    Manage web storage

Tabs:
  tab [new|list|close|<n>]   Manage tabs

Diff:
  diff snapshot              Compare current vs last snapshot
  diff screenshot --baseline Compare current vs baseline image
  diff url <u1> <u2>         Compare two pages

Debug:
  trace start|stop [path]    Record Playwright trace
  profiler start|stop [path] Record Chrome DevTools profile
  record start <path> [url]  Start video recording (WebM)
  record stop                Stop and save video
  console [--clear]          View console logs
  errors [--clear]           View page errors
  highlight <sel>            Highlight element

Auth Vault:
  auth save <name> [opts]    Save auth profile (--url, --username, --password/--password-stdin)
  auth login <name>          Login using saved credentials
  auth list                  List saved auth profiles
  auth show <name>           Show auth profile metadata
  auth delete <name>         Delete auth profile

Confirmation:
  confirm <id>               Approve a pending action
  deny <id>                  Deny a pending action

Sessions:
  session                    Show current session name
  session list               List active sessions

Configuration:
  config set <key> <value>   Set configuration value
  config get <key>           Get configuration value
  config list                List all configuration
  config remove <key>        Remove configuration value

Configuration Management:
  The config command manages persistent settings in ~/.scrapeless/config.json.
  Configuration values take priority over environment variables (only SCRAPELESS_API_KEY is supported as env var).

  Supported keys:
    apiKey                   API key (same as SCRAPELESS_API_KEY)
    apiVersion               API version (v1 or v2, default: v2)
    sessionTtl               Session timeout in seconds
    sessionName              Session name for identification
    sessionRecording         Enable session recording (true/false)
    proxyUrl                 Custom proxy URL
    proxyCountry             Proxy country code (e.g., US, UK)
    proxyState               Proxy state/province
    proxyCity                Proxy city
    fingerprint              Browser fingerprint
    userAgent                Custom user agent string
    platform                 Platform type (Windows, Linux, macOS, iOS, Android)
    screenWidth              Screen width in pixels
    screenHeight             Screen height in pixels
    timezone                 Timezone (e.g., America/New_York, Asia/Shanghai)
    languages                Comma-separated language codes (e.g., en,zh-CN)
    debug                    Enable debug logging

  Examples:
    scrapeless-scraping-browser config set apiKey your_api_key_here
    scrapeless-scraping-browser config set proxyCountry US
    scrapeless-scraping-browser config set sessionTtl 300
    scrapeless-scraping-browser config set userAgent "Custom/1.0"
    scrapeless-scraping-browser config set platform iOS
    scrapeless-scraping-browser config list
    scrapeless-scraping-browser config remove proxyCountry

Session Management:
  The session management commands allow you to control Scrapeless cloud browser sessions.
  All commands require SCRAPELESS_API_KEY to be configured.

  new-session [options]      Create a new browser session
    Options:
      --name <name>          Session name for identification
      --ttl <seconds>        Session timeout in seconds (default: 180)
      --recording <true|false> Enable session recording
      --proxy-country <code> Proxy country code (e.g., AU, US)
      --proxy-state <state>  Proxy state (e.g., NSW, CA)
      --proxy-city <city>    Proxy city (e.g., sydney, newyork)
      --user-agent <ua>      Custom user agent string
      --platform <platform>  Platform (Windows, macOS, Linux)
      --screen-width <px>    Screen width in pixels (default: 1920)
      --screen-height <px>   Screen height in pixels (default: 1080)
      --timezone <tz>        Timezone (default: America/New_York)
      --languages <langs>    Comma-separated language codes (default: en)
    Returns: taskId for the new session
    Example: scrapeless-scraping-browser new-session --name "test-session" --ttl 300

  sessions                   List all running sessions with details
    Returns: sessionId, createdAt, status, sessionName (if set)
    Example: scrapeless-scraping-browser sessions

  stop <taskId>              Stop a specific session by its task ID
    Example: scrapeless-scraping-browser stop abc123def456

  stop-all                   Stop all running sessions
    Returns: Number of sessions stopped and failed
    Example: scrapeless-scraping-browser stop-all

  live [taskId]              Get live preview WebSocket URL for a session
    If taskId is omitted, uses the current session or latest running session
    Returns: WebSocket URL for live browser viewing
    Example: scrapeless-scraping-browser live
    Example: scrapeless-scraping-browser live abc123def456

  Session workflow:
    # List all sessions
    scrapeless-scraping-browser sessions

    # Get live preview of current session
    scrapeless-scraping-browser live

    # Stop a specific session
    scrapeless-scraping-browser stop abc123def456

    # Clean up all sessions
    scrapeless-scraping-browser stop-all

Snapshot Options:
  -i, --interactive          Only interactive elements
  -c, --compact              Remove empty structural elements
  -d, --depth <n>            Limit tree depth
  -s, --selector <sel>       Scope to CSS selector

Options:
  --session-id <id>          Connect to specific Scrapeless session
  --json                     JSON output
  --full, -f                 Full page screenshot
  --annotate                 Annotated screenshot with numbered labels and legend
  --headed                   Show browser window (not headless) (or SCRAPELESS_BROWSER_HEADED env)
  --color-scheme <scheme>    Color scheme: dark, light, no-preference (or SCRAPELESS_BROWSER_COLOR_SCHEME)
  --download-path <path>     Default download directory (or SCRAPELESS_BROWSER_DOWNLOAD_PATH)
  --content-boundaries       Wrap page output in boundary markers (or SCRAPELESS_BROWSER_CONTENT_BOUNDARIES)
  --max-output <chars>       Truncate page output to N chars (or SCRAPELESS_BROWSER_MAX_OUTPUT)
  --debug                    Debug output
  --version, -V              Show version

Environment Variables:
  SCRAPELESS_API_KEY         Your Scrapeless API token (required)

  Note: All other configuration should be done via the config command:
    scrapeless-scraping-browser config set <key> <value>

  Browser-specific environment variables (for local browser features):
  SCRAPELESS_BROWSER_HEADED  Show browser window (not headless)
  SCRAPELESS_BROWSER_COLOR_SCHEME Color scheme preference
  SCRAPELESS_BROWSER_DOWNLOAD_PATH Default download directory
  SCRAPELESS_BROWSER_CONTENT_BOUNDARIES Wrap page output in boundary markers
  SCRAPELESS_BROWSER_MAX_OUTPUT Max characters for page output

Install:
  npm install -g scrapeless-scraping-browser-skills

Try without installing:
  npx scrapeless-scraping-browser-skills open example.com

Examples:
  # Configuration (recommended method)
  scrapeless-scraping-browser config set apiKey your_api_key_here
  scrapeless-scraping-browser config set proxyCountry US
  scrapeless-scraping-browser config set sessionTtl 300
  scrapeless-scraping-browser config list

  # Or use environment variable for API key only
  export SCRAPELESS_API_KEY=your_token

  # Basic browser automation
  scrapeless-scraping-browser open example.com
  scrapeless-scraping-browser snapshot -i              # Interactive elements only
  scrapeless-scraping-browser click @e2                # Click by ref from snapshot
  scrapeless-scraping-browser fill @e3 "test@example.com"
  scrapeless-scraping-browser find role button click --name Submit
  scrapeless-scraping-browser get text @e1
  scrapeless-scraping-browser screenshot --full
  scrapeless-scraping-browser screenshot --annotate    # Labeled screenshot for vision models
  scrapeless-scraping-browser wait --load networkidle  # Wait for slow pages to load
  scrapeless-scraping-browser --session-id abc123 open example.com  # Connect to specific session
  scrapeless-scraping-browser --color-scheme dark open example.com  # Dark mode
  scrapeless-scraping-browser --headed open example.com             # Show browser window

Session Management:
  scrapeless-scraping-browser sessions                 # List running sessions
  scrapeless-scraping-browser stop <taskId>            # Stop specific session
  scrapeless-scraping-browser stop-all                 # Stop all sessions
  scrapeless-scraping-browser live                     # Get live preview URL

Command Chaining:
  Chain commands with && in a single shell call (session persists):

  scrapeless-scraping-browser open example.com && scrapeless-scraping-browser wait --load networkidle && scrapeless-scraping-browser snapshot -i
  scrapeless-scraping-browser fill @e1 "user@example.com" && scrapeless-scraping-browser fill @e2 "pass" && scrapeless-scraping-browser click @e3
  scrapeless-scraping-browser open example.com && scrapeless-scraping-browser wait --load networkidle && scrapeless-scraping-browser screenshot page.png

Proxy Configuration:
  scrapeless-scraping-browser config set proxyCountry US
  scrapeless-scraping-browser config set proxyState CA
  scrapeless-scraping-browser config set proxyCity "Los Angeles"
  scrapeless-scraping-browser config set proxyUrl "http://user:pass@proxy.com:8080"

Browser Fingerprinting:
  scrapeless-scraping-browser config set fingerprint chrome
  scrapeless-scraping-browser config set userAgent "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X)"
  scrapeless-scraping-browser config set platform iOS
  scrapeless-scraping-browser config set screenWidth 375
  scrapeless-scraping-browser config set screenHeight 812
  scrapeless-scraping-browser config set timezone "Asia/Shanghai"
  scrapeless-scraping-browser config set languages "zh-CN,en"

Session Recording:
  scrapeless-scraping-browser config set sessionRecording true
"#
    );
}

fn print_snapshot_diff(data: &serde_json::Map<String, serde_json::Value>) {
    let changed = data
        .get("changed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !changed {
        println!("{} No changes detected", color::success_indicator());
        return;
    }
    if let Some(diff) = data.get("diff").and_then(|v| v.as_str()) {
        for line in diff.lines() {
            if line.starts_with("+ ") {
                println!("{}", color::green(line));
            } else if line.starts_with("- ") {
                println!("{}", color::red(line));
            } else {
                println!("{}", color::dim(line));
            }
        }
        let additions = data.get("additions").and_then(|v| v.as_i64()).unwrap_or(0);
        let removals = data.get("removals").and_then(|v| v.as_i64()).unwrap_or(0);
        let unchanged = data.get("unchanged").and_then(|v| v.as_i64()).unwrap_or(0);
        println!(
            "\n{} additions, {} removals, {} unchanged",
            color::green(&additions.to_string()),
            color::red(&removals.to_string()),
            unchanged
        );
    }
}

fn print_screenshot_diff(data: &serde_json::Map<String, serde_json::Value>) {
    let mismatch = data
        .get("mismatchPercentage")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let is_match = data.get("match").and_then(|v| v.as_bool()).unwrap_or(false);
    let dim_mismatch = data
        .get("dimensionMismatch")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if dim_mismatch {
        println!(
            "{} Images have different dimensions",
            color::error_indicator()
        );
    } else if is_match {
        println!(
            "{} Images match (0% difference)",
            color::success_indicator()
        );
    } else {
        println!(
            "{} {:.2}% pixels differ",
            color::error_indicator(),
            mismatch
        );
    }
    if let Some(diff_path) = data.get("diffPath").and_then(|v| v.as_str()) {
        println!("  Diff image: {}", color::green(diff_path));
    }
    let total = data
        .get("totalPixels")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let different = data
        .get("differentPixels")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    println!(
        "  {} different / {} total pixels",
        color::red(&different.to_string()),
        total
    );
}

pub fn print_version() {
    println!("scrapeless-scraping-browser {}", env!("CARGO_PKG_VERSION"));
}
