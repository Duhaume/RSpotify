mod config;
mod generate;
mod runner;
mod ui;

use eframe::egui;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let mut shot: Option<PathBuf> = None;
    let mut section: Option<ui::Section> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => shot = args.next().map(PathBuf::from),
            "--section" => section = args.next().and_then(|s| ui::Section::parse(&s)),
            "--export" => {
                // Write the Docker files with the saved settings (or defaults) and exit.
                let dir = PathBuf::from(args.next().unwrap_or_else(|| "raspotify-docker".into()));
                let defaults = args.next().as_deref() == Some("--defaults");
                let mut s = if defaults { config::Settings::default() } else { config::Settings::load() };
                if defaults {
                    // A shared template gets no code: install.sh picks one per Pi.
                    s.deploy.web_code.clear();
                }
                match generate::write_all(&s, &dir) {
                    Ok(files) => {
                        println!("{} files written to {}", files.len(), dir.display());
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!("cannot write {}: {e}", dir.display());
                        std::process::exit(1);
                    }
                }
            }
            "-h" | "--help" => {
                println!(
                    "raspotify-docker [--screenshot FILE.png [--section speaker|sound|advanced|hotspot|install|files]]\n\
                     raspotify-docker --export DIR [--defaults]"
                );
                return Ok(());
            }
            _ => {}
        }
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Raspotify Docker")
        .with_inner_size([1180.0, 780.0])
        .with_min_inner_size([900.0, 600.0]);
    if shot.is_some() {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        options.event_loop_builder = Some(Box::new(|b| {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            b.with_activate_ignoring_other_apps(false);
        }));
    }

    eframe::run_native(
        "Raspotify Docker",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, shot, section)))),
    )
}
