use std::process::Command;
use std::fs;
use tauri_plugin_opener::OpenerExt;

#[derive(serde::Serialize)]
struct Update {
    name: String,
    old_version: String,
    new_version: String,
}

#[derive(serde::Serialize)]
struct InstalledPackage {
    name: String,
    version: String,
    cached_versions: Vec<String>, 
}

#[derive(serde::Serialize)]
struct NewsItem {
    title: String,
    link: String,
    pub_date: String, 
}

#[derive(serde::Serialize)]
struct SearchResult {
    repo: String,
    name: String,
    version: String,
    description: String,
    installed: bool,
}

#[tauri::command]
fn get_updates() -> Vec<Update> {
    let output = Command::new("checkupdates").output();
    if output.is_err() { return Vec::new(); }
    let output = output.unwrap();
    let output_str = String::from_utf8_lossy(&output.stdout);
    
    let mut updates = Vec::new();
    for line in output_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            updates.push(Update {
                name: parts[0].to_string(),
                old_version: parts[1].to_string(),
                new_version: parts[3].to_string(),
            });
        }
    }
    updates
}

#[tauri::command]
fn get_installed_packages() -> Vec<InstalledPackage> {
    let output = Command::new("pacman").args(&["-Qe"]).output();
    if output.is_err() { return Vec::new(); }
    
    let output_result = output.unwrap();
    let output_str = String::from_utf8_lossy(&output_result.stdout);

    let cache_dir = "/var/cache/pacman/pkg";
    let mut cache_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    
    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if name.ends_with(".pkg.tar.zst") {
                    if let Some(last_dash) = name.rfind('-') { 
                        if let Some(second_dash) = name[..last_dash].rfind('-') { 
                             if let Some(version_start) = name[..second_dash].rfind('-') {
                                 let pkg_name = &name[..version_start];
                                 cache_map.entry(pkg_name.to_string())
                                     .or_default()
                                     .push(name);
                             }
                        }
                    }
                }
            }
        }
    }

    let mut packages = Vec::new();
    for line in output_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let name = parts[0].to_string();
            let version = parts[1].to_string();
            
            let mut cached = cache_map.remove(&name).unwrap_or_default();
            cached.sort();
            cached.reverse();

            packages.push(InstalledPackage {
                name,
                version,
                cached_versions: cached,
            });
        }
    }
    packages
}

#[tauri::command]
fn search_packages(query: String) -> Vec<SearchResult> {
    if query.trim().is_empty() { return Vec::new(); }

    if !validate_input(&query) { return Vec::new(); }

    let output = Command::new("pacman")
        .args(&["-Ss", &query])
        .output();

    if output.is_err() { return Vec::new(); }
    let output = output.unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut results = Vec::new();
    let mut current_pkg: Option<SearchResult> = None;

    for line in stdout.lines() {
        if !line.starts_with(' ') {
            if let Some(pkg) = current_pkg.take() {
                results.push(pkg);
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let full_name = parts[0];
                let (repo, name) = if let Some(idx) = full_name.find('/') {
                    (full_name[..idx].to_string(), full_name[idx+1..].to_string())
                } else {
                    ("?".to_string(), full_name.to_string())
                };
                
                let version = parts[1].to_string();
                let installed = line.contains("[installed]");
                
                current_pkg = Some(SearchResult {
                    repo,
                    name,
                    version,
                    description: String::new(),
                    installed,
                });
            }
        } else {
            if let Some(ref mut pkg) = current_pkg {
                if !pkg.description.is_empty() {
                    pkg.description.push(' ');
                }
                pkg.description.push_str(line.trim());
            }
        }
    }
    if let Some(pkg) = current_pkg {
        results.push(pkg);
    }
    
    results
}

#[tauri::command]
fn fetch_package_history(name: String) -> Vec<String> {
    if !validate_input(&name) { return Vec::new(); }

    let first_char = name.chars().next().unwrap_or('a');
    let url = format!("https://archive.archlinux.org/packages/{}/{}/", first_char, name);
    
    // INCREASED TIMEOUT: 10s
    let client = reqwest::blocking::Client::builder()
        .user_agent("Kensa/1.0")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok();

    let client = match client {
        Some(c) => c,
        None => return Vec::new(),
    };

    let response = match client.get(&url).send() {
        Ok(res) => res.text().unwrap_or_default(),
        Err(e) => {
            eprintln!("Error fetching history for {}: {}", name, e);
            return Vec::new();
        }
    };

    let mut found_versions = Vec::new();
    for line in response.lines() {
        if line.contains("href=\"") && line.contains(".pkg.tar.zst\"") {
            let start = line.find("href=\"").unwrap() + 6;
            let end = line[start..].find("\"").unwrap() + start;
            let filename = &line[start..end];
            if !filename.ends_with(".sig") && filename.starts_with(&name) {
                found_versions.push(format!("{}{}", url, filename));
            }
        }
    }
    found_versions.reverse(); 
    found_versions
}

#[tauri::command]
fn get_news() -> Vec<NewsItem> {
    let url = "https://archlinux.org/feeds/news/";
    
    // INCREASED TIMEOUT: 10s
    let client = reqwest::blocking::Client::builder()
        .user_agent("Kensa/1.0") 
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok();

    let client = match client {
        Some(c) => c,
        None => {
            eprintln!("Failed to build HTTP client");
            return Vec::new();
        },
    };

    let response = match client.get(url).send() {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Error fetching news: {}", e);
            return Vec::new();
        },
    };
    
    let content = response.text().unwrap_or_default();
    let channel = rss::Channel::read_from(content.as_bytes()).unwrap_or_default();
    
    let mut news_list = Vec::new();
    for item in channel.items().iter().take(5) { 
        news_list.push(NewsItem {
            title: item.title().unwrap_or("No Title").to_string(),
            link: item.link().unwrap_or("#").to_string(),
            pub_date: item.pub_date().unwrap_or("").to_string(),
        });
    }
    news_list
}

fn validate_input(input: &str) -> bool {
    input.chars().all(|c| c.is_alphanumeric() || "-_+.@/:".contains(c))
}

#[tauri::command]
fn manage_packages(action: String, packages: Vec<String>) {
    for pkg in &packages {
        if !validate_input(pkg) {
            eprintln!("Security Alert: Invalid package name detected");
            return;
        }
    }

    let pacman_cmd = match action.as_str() {
        "update_all" => "sudo pacman -Syu".to_string(),
        "install" => format!("sudo pacman -S {}", packages.join(" ")),
        "remove" => format!("sudo pacman -Rns {}", packages.join(" ")),
        "upgrade_file" => format!("sudo pacman -U {}", packages.join(" ")),
        _ => return, 
    };

    let terminals = ["konsole", "kitty", "alacritty", "gnome-terminal", "xfce4-terminal", "xterm", "foot"];
    let bash_cmd = format!("{}; echo ''; echo 'Press Enter to close...'; read", pacman_cmd);

    for term in terminals {
        let mut cmd = Command::new(term);

        if term == "konsole" { cmd.arg("--nofork"); } 
        else if term == "gnome-terminal" { cmd.arg("--wait"); } 
        else if term == "xfce4-terminal" { cmd.arg("--disable-server"); }

        if term == "gnome-terminal" {
            cmd.args(&["--", "bash", "-c", &bash_cmd]);
        } else {
            cmd.arg("-e").args(&["bash", "-c", &bash_cmd]);
        }

        if let Ok(mut child) = cmd.spawn() {
            let _ = child.wait();
            return;
        }
    }
}

#[tauri::command]
fn check_is_installed(name: String) -> bool {
    if !validate_input(&name) { return false; }

    let status = Command::new("pacman")
        .arg("-Q")
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) => s.success(),
        Err(_) => false,
    }
}

#[tauri::command]
fn send_notification(title: String, body: String) {
    let _ = Command::new("notify-send")
        .arg("-a").arg("Kensa")
        .arg(&title)
        .arg(&body)
        .spawn();
}

#[tauri::command]
fn open_link(app: tauri::AppHandle, url: String) {
    let _ = app.opener().open_url(url, None::<&str>);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init()) 
        .invoke_handler(tauri::generate_handler![
            get_updates, 
            get_installed_packages, 
            search_packages, 
            fetch_package_history,
            get_news, 
            manage_packages, 
            check_is_installed, 
            send_notification,
            open_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}