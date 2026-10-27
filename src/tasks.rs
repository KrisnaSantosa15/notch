//! Simple to-do list. A time suffix becomes a reminder: "Team sync 15.30", "Call Dina at 9:00".

use crate::media::unix_now;
use windows::Win32::System::SystemInformation::GetLocalTime;

#[derive(Clone)]
pub struct Task {
    pub text: String,
    pub done: bool,
    /// Reminder timestamp (unix seconds).
    pub due: Option<f64>,
    /// Formatted reminder time to display, e.g. "15:30".
    pub label: String,
    pub notified: bool,
}

impl Task {
    pub fn parse(input: &str) -> Option<Task> {
        let mut words: Vec<&str> = input.split_whitespace().collect();
        let mut due = None;
        let mut label = String::new();
        if let Some((h, m)) = words.last().and_then(|w| clock(w.trim_start_matches('@'))) {
            words.pop();
            if matches!(words.last().map(|w| w.to_lowercase()).as_deref(), Some("at" | "jam" | "pukul" | "@")) {
                words.pop();
            }
            due = Some(next_occurrence(h, m));
            label = format!("{h:02}:{m:02}");
        }
        let text = words.join(" ");
        (!text.is_empty()).then_some(Task { text, done: false, due, label, notified: false })
    }

    pub fn overdue(&self) -> bool {
        !self.done && self.due.is_some_and(|d| d <= unix_now())
    }
}

/// "15:30" or "15.30" -> (15, 30)
fn clock(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once([':', '.'])?;
    if m.len() != 2 || h.is_empty() || h.len() > 2 {
        return None;
    }
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// Next matching time: today if not yet passed, otherwise tomorrow.
fn next_occurrence(h: u32, m: u32) -> f64 {
    let now = unsafe { GetLocalTime() };
    let current = now.wHour as i64 * 3600 + now.wMinute as i64 * 60 + now.wSecond as i64;
    let mut delta = (h as i64 * 3600 + m as i64 * 60) - current;
    if delta <= 0 {
        delta += 86_400;
    }
    unix_now() + delta as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_time_suffix() {
        let t = Task::parse("Rapat tim jam 15.30").unwrap();
        assert_eq!((t.text.as_str(), t.label.as_str()), ("Rapat tim", "15:30"));
        assert!(t.due.is_some());
        let t = Task::parse("Call Dina at 9:05").unwrap();
        assert_eq!((t.text.as_str(), t.label.as_str()), ("Call Dina", "09:05"));
        let t = Task::parse("Beli kopi").unwrap();
        assert!(t.due.is_none());
        assert!(Task::parse("15:30").is_none());
        assert!(Task::parse("Versi 1.2").unwrap().due.is_none());
    }
}
