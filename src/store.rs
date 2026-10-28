//! Saves settings, Shelf items, and Tasks to %APPDATA%\Notch\state.txt.
//! Uses a plain-text line-based format, easy to inspect and edit manually.

use crate::media::unix_now;
use crate::tasks::Task;
use std::path::PathBuf;

pub struct Saved {
    pub lyrics: bool,
    pub preset: u32,
    pub files: Vec<String>,
    pub tasks: Vec<Task>,
    pub welcomed: bool,
}

fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var("APPDATA").ok()?).join("Notch").join("state.txt"))
}

pub fn load() -> Saved {
    let mut saved = Saved { lyrics: false, preset: 25, files: Vec::new(), tasks: Vec::new(), welcomed: false };
    let Some(text) = path().and_then(|p| std::fs::read_to_string(p).ok()) else { return saved };
    let now = unix_now();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        match key {
            "welcomed" => saved.welcomed = value == "1",
            "lyrics" => saved.lyrics = value == "1",
            "preset" => saved.preset = value.parse().unwrap_or(25),
            "file" if std::path::Path::new(value).exists() => saved.files.push(value.to_string()),
            "task" => {
                let mut parts = value.splitn(4, '\t');
                let (Some(done), Some(due), Some(label), Some(text)) = (parts.next(), parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let due = due.parse::<f64>().ok().filter(|d| *d > 0.0);
                saved.tasks.push(Task {
                    text: text.to_string(),
                    done: done == "1",
                    due,
                    label: label.to_string(),
                    // reminders missed while the app was closed are not fired again
                    notified: due.is_some_and(|d| d <= now),
                });
            }
            _ => {}
        }
    }
    saved
}

pub fn save(welcomed: bool, lyrics: bool, preset: u32, files: &[String], tasks: &[Task]) {
    let Some(path) = path() else { return };
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    let mut out = format!("welcomed={}\nlyrics={}\npreset={preset}\n", if welcomed { 1 } else { 0 }, if lyrics { 1 } else { 0 });
    for f in files {
        out += &format!("file={f}\n");
    }
    for t in tasks {
        out += &format!(
            "task={}\t{}\t{}\t{}\n",
            if t.done { 1 } else { 0 },
            t.due.unwrap_or(0.0),
            t.label,
            clean(&t.text)
        );
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saved_welcomed_parsing() {
        let text = "welcomed=1\nlyrics=0\npreset=15\n";
        let mut saved = Saved { lyrics: false, preset: 25, files: Vec::new(), tasks: Vec::new(), welcomed: false };
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                if k == "welcomed" {
                    saved.welcomed = v == "1";
                }
            }
        }
        assert!(saved.welcomed);
    }
}
