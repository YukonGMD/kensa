use std::process::Command;
use tauri_plugin_opener::OpenerExt;

#[derive(serde::Serialize)]
struct Update {
    name: String,
    old_version: String,
    new_version: String,
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

    if output.is_err() {
        return Vec::new();
    }
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
fn get_news() -> Vec<NewsItem> {
    let url = "https://archlinux.org/feeds/news/";
    let response = reqwest::blocking::get(url);
    if response.is_err() {
        return Vec::new();
    }

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
fn install_updates() {
    let terminals = [
        "konsole",
        "kitty",
        "alacritty",
        "gnome-terminal",
        "xfce4-terminal",
        "xterm",
        "foot",
    ];

    for term in terminals {
        let mut cmd = Command::new(term);

        if term == "konsole" {
            cmd.arg("--nofork");
        } else if term == "gnome-terminal" {
            cmd.arg("--wait");
        } else if term == "xfce4-terminal" {
            cmd.arg("--disable-server");
        }

        if term == "gnome-terminal" {
            cmd.args(&[
                "--",
                "bash",
                "-c",
                "sudo pacman -Syu; echo ''; echo 'Press Enter to close...'; read",
            ]);
        } else {
            cmd.arg("-e").args(&[
                "bash",
                "-c",
                "sudo pacman -Syu; echo ''; echo 'Press Enter to close...'; read",
            ]);
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
            get_news,
            install_updates,
            open_link
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
