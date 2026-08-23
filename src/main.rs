mod backend;

use backend::FocusEvent;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

fn data_file() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE")) // Windows fallback
        .unwrap_or_else(|_| "/tmp".into());
    let dir = PathBuf::from(home).join(".local/share/window-time-tracker");
    fs::create_dir_all(&dir).ok();
    dir.join("durations.tsv")
}

fn load_existing(path: &PathBuf) -> HashMap<String, Duration> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if let Some((name, secs)) = line.split_once('\t') {
                if let Ok(s) = secs.trim().parse::<u64>() {
                    map.insert(name.to_string(), Duration::from_secs(s));
                }
            }
        }
    }
    map
}

fn save(path: &PathBuf, map: &HashMap<String, Duration>) {
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by_key(|(_, d)| std::cmp::Reverse(d.as_secs()));
    let mut out = String::new();
    for (name, dur) in entries {
        out.push_str(&format!("{}\t{}\n", name, dur.as_secs()));
    }
    if let Ok(mut f) = OpenOptions::new().write(true).create(true).truncate(true).open(path) {
        let _ = f.write_all(out.as_bytes());
    }
}

fn main() -> std::io::Result<()> {
    let path = data_file();
    let mut durations = load_existing(&path);

    let backend = backend::detect_backend()?;
    println!("Using backend: {}", backend.name());

    let (tx, rx) = mpsc::channel::<FocusEvent>();
    std::thread::spawn(move || {
        if let Err(e) = backend.run(tx) {
            eprintln!("backend error: {e}");
        }
    });

    let mut current: Option<String> = None;
    let mut since = SystemTime::now();

    for event in rx {
        if let Some(prev) = &current {
            let elapsed = event
                .at
                .duration_since(since)
                .unwrap_or(Duration::ZERO);
            *durations.entry(prev.clone()).or_insert(Duration::ZERO) += elapsed;
            save(&path, &durations);
        }
        since = event.at;
        current = event.window.clone();

        match &current {
            Some(w) => println!("-> {w}"),
            None => println!("-> (no active window)"),
        }
    }

    Ok(())
}
