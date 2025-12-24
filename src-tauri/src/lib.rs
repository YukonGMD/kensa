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
fn fetch_package_history(name: String) -> Vec<String> {
    // 1. Construct URL (e.g. https://archive.archlinux.org/packages/k/kensa/)
    let first_char = name.chars().next().unwrap_or('a');
    let url = format!("https://archive.archlinux.org/packages/{}/{}/", first_char, name);
    
    // 2. Fetch the HTML listing
    let response = match reqwest::blocking::get(&url) {
        Ok(res) => res.text().unwrap_or_default(),
        Err(_) => return Vec::new(),
    };

    // 3. Simple HTML Parse (Find links ending in .pkg.tar.zst)
    let mut found_versions = Vec::new();
    for line in response.lines() {
        if line.contains("href=\"") && line.contains(".pkg.tar.zst\"") {
            let start = line.find("href=\"").unwrap() + 6;
            let end = line[start..].find("\"").unwrap() + start;
            let filename = &line[start..end];
            // Filter out signatures (.sig) and ensure it matches the package name
            if !filename.ends_with(".sig") && filename.starts_with(&name) {
                // Return full URL so pacman can download it
                found_versions.push(format!("{}{}", url, filename));
            }
        }
    }
    // Newest usually at bottom of HTML listing, so reverse for UI
    found_versions.reverse(); 
    found_versions
}

#[tauri::command]
fn get_news() -> Vec<NewsItem> {
    let url = "https://archlinux.org/feeds/news/";
    let response = reqwest::blocking::get(url);
    if response.is_err() { return Vec::new(); }
    
    let content = response.unwrap().text().unwrap_or_default();
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

#[tauri::command]
fn install_updates(target_cmd: Option<String>) {
    let terminals = ["konsole", "kitty", "alacritty", "gnome-terminal", "xfce4-terminal", "xterm", "foot"];
    
    let final_cmd = target_cmd.unwrap_or_else(|| "sudo pacman -Syu".to_string());
    let bash_cmd = format!("{}; echo ''; echo 'Press Enter to close...'; read", final_cmd);

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
            fetch_package_history,
            get_news, 
            install_updates, 
            open_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}