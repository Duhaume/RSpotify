//! Runs shell commands in the background and collects their output.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct Log {
    pub lines: Vec<String>,
    pub running: Option<String>,
}

#[derive(Clone, Default)]
pub struct Runner {
    pub log: Arc<Mutex<Log>>,
}

impl Runner {
    pub fn push(&self, line: impl Into<String>) {
        let mut l = self.log.lock().unwrap();
        l.lines.push(line.into());
        let n = l.lines.len();
        if n > 2000 {
            l.lines.drain(0..n - 2000);
        }
    }

    pub fn busy(&self) -> bool {
        self.log.lock().unwrap().running.is_some()
    }

    /// Runs `script` with `sh -c` in `dir`; output lines go to the log.
    pub fn run(&self, ctx: &eframe::egui::Context, title: &str, dir: &str, script: String) {
        if self.busy() {
            self.push("Une commande tourne déjà, attendez qu'elle finisse.");
            return;
        }
        self.log.lock().unwrap().running = Some(title.to_string());
        self.push(format!("▶ {title}"));
        self.push(format!("$ {script}"));
        let me = self.clone();
        let ctx = ctx.clone();
        let dir = dir.to_string();
        std::thread::spawn(move || {
            let child = Command::new("sh")
                .arg("-c")
                .arg(format!("{script} 2>&1"))
                .current_dir(&dir)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .spawn();
            match child {
                Ok(mut c) => {
                    if let Some(out) = c.stdout.take() {
                        for line in BufReader::new(out).lines().map_while(Result::ok) {
                            me.push(line);
                            ctx.request_repaint();
                        }
                    }
                    match c.wait() {
                        Ok(s) if s.success() => me.push("✔ terminé"),
                        Ok(s) => me.push(format!("✖ échec (code {})", s.code().unwrap_or(-1))),
                        Err(e) => me.push(format!("✖ {e}")),
                    }
                }
                Err(e) => me.push(format!("✖ impossible de lancer la commande : {e}")),
            }
            me.log.lock().unwrap().running = None;
            ctx.request_repaint();
        });
    }
}

/// Quotes a value for a POSIX shell.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
