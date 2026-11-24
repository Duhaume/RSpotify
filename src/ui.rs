//! The window.

use crate::config::*;
use crate::generate;
use crate::runner::{sh_quote, Runner};
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Stroke, Vec2};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const ACCENT: Color32 = Color32::from_rgb(61, 220, 151);
const ACCENT_DIM: Color32 = Color32::from_rgb(28, 92, 66);
const BG: Color32 = Color32::from_rgb(18, 20, 24);
const PANEL: Color32 = Color32::from_rgb(26, 29, 35);
const CARD: Color32 = Color32::from_rgb(34, 38, 46);
const TEXT_DIM: Color32 = Color32::from_rgb(150, 158, 170);
const WARN: Color32 = Color32::from_rgb(240, 180, 70);
const ERR: Color32 = Color32::from_rgb(240, 100, 90);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Speaker,
    Sound,
    Advanced,
    Hotspot,
    Install,
    Docker,
    Files,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Speaker,
        Section::Sound,
        Section::Advanced,
        Section::Hotspot,
        Section::Install,
        Section::Docker,
        Section::Files,
    ];
    fn title(self) -> &'static str {
        match self {
            Section::Speaker => "Enceinte",
            Section::Sound => "Son",
            Section::Advanced => "Avancé",
            Section::Hotspot => "Point d'accès Wi-Fi",
            Section::Install => "Installation",
            Section::Docker => "Images et YAML",
            Section::Files => "Fichiers générés",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Section::Speaker => "Nom et type visibles dans Spotify",
            Section::Sound => "Qualité, volume, normalisation",
            Section::Advanced => "Cache, journal, réseau",
            Section::Hotspot => "Le Pi crée son propre réseau",
            Section::Install => "Docker sur le Pi, réglages en http",
            Section::Docker => "Compiler, docker compose",
            Section::Files => "Ce qui sera copié sur le Pi",
        }
    }
    pub fn parse(s: &str) -> Option<Section> {
        Some(match s {
            "speaker" => Section::Speaker,
            "sound" => Section::Sound,
            "advanced" => Section::Advanced,
            "hotspot" => Section::Hotspot,
            "install" => Section::Install,
            "docker" => Section::Docker,
            "files" => Section::Files,
            _ => return None,
        })
    }
}

pub struct ScreenshotPlan {
    pub path: PathBuf,
    pub frames: u32,
    pub requested: bool,
    pub done: Arc<AtomicBool>,
}

pub struct App {
    s: Settings,
    saved: Settings,
    section: Section,
    file_tab: usize,
    show_pass: bool,
    runner: Runner,
    toast: Option<(String, f64)>,
    shot: Option<ScreenshotPlan>,
    pi: Arc<std::sync::Mutex<PiStatus>>,
    /// The app is running on the Pi itself.
    on_pi: bool,
    host: String,
    show_ssh: bool,
    /// Docker page: show the "images already compiled" YAML.
    yaml_images: bool,
    /// Docker page: compile for a 32-bit Pi system.
    arm32: bool,
}

/// What the app knows about the remote Pi (this app runs on the computer, not on the Pi).
#[derive(Clone, PartialEq)]
pub enum PiStatus {
    Unknown,
    Checking,
    Reachable(String),
    Unreachable(String),
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, section: Option<Section>) -> Self {
        style(&cc.egui_ctx);
        let s = if shot.is_some() { Settings::default() } else { Settings::load() };
        let mut app = Self {
            saved: s.clone(),
            s,
            section: section.unwrap_or(Section::Speaker),
            file_tab: 0,
            show_pass: false,
            runner: Runner::default(),
            toast: None,
            pi: Arc::new(std::sync::Mutex::new(PiStatus::Unknown)),
            on_pi: running_on_pi(),
            host: host_name(),
            show_ssh: false,
            yaml_images: false,
            arm32: false,
            shot: shot.map(|path| ScreenshotPlan {
                path,
                frames: 0,
                requested: false,
                done: Arc::new(AtomicBool::new(false)),
            }),
        };
        if app.shot.is_some() && section == Some(Section::Hotspot) {
            app.s.hotspot.enabled = true;
            app.s.hotspot.password = "musique-salon".into();
        }
        app
    }

    fn notify(&mut self, ctx: &egui::Context, msg: impl Into<String>) {
        self.toast = Some((msg.into(), ctx.input(|i| i.time) + 4.0));
    }

    fn out_dir(&self) -> PathBuf {
        PathBuf::from(self.s.deploy.output_dir.trim())
    }

    fn write_files(&mut self, ctx: &egui::Context) -> bool {
        let dir = self.out_dir();
        match generate::write_all(&self.s, &dir) {
            Ok(files) => {
                self.runner.push(format!("✔ {} fichiers écrits dans {}", files.len(), dir.display()));
                self.notify(ctx, "Fichiers écrits");
                true
            }
            Err(e) => {
                self.runner.push(format!("✖ écriture impossible dans {} : {e}", dir.display()));
                self.notify(ctx, "Écriture impossible");
                false
            }
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(plan) = &mut self.shot else { return };
        ctx.request_repaint();
        if plan.done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        plan.frames += 1;
        if plan.frames >= 120 && !plan.requested {
            plan.requested = true;
            let path = plan.path.clone();
            let done = plan.done.clone();
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                if let Err(e) = save_png(&path, &img) {
                    eprintln!("screenshot failed: {e}");
                }
                done.store(true, Ordering::SeqCst);
                c2.request_repaint();
            });
        }
    }
}

fn save_png(path: &Path, img: &egui::ColorImage) -> Result<(), String> {
    if let Some(d) = path.parent() {
        if !d.as_os_str().is_empty() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
    }
    let [w, h] = img.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for p in &img.pixels {
        buf.extend_from_slice(&[p.r(), p.g(), p.b(), 255]);
    }
    image::save_buffer(path, &buf, w as u32, h as u32, image::ColorType::Rgba8).map_err(|e| e.to_string())
}

fn style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|st| {
        let v = &mut st.visuals;
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.extreme_bg_color = Color32::from_rgb(14, 16, 19);
        v.selection.bg_fill = ACCENT_DIM;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.widgets.inactive.corner_radius = CornerRadius::same(6);
        v.widgets.hovered.corner_radius = CornerRadius::same(6);
        v.widgets.active.corner_radius = CornerRadius::same(6);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(44, 49, 59);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(54, 60, 72);
        st.spacing.item_spacing = Vec2::new(8.0, 8.0);
        st.spacing.button_padding = Vec2::new(12.0, 6.0);
        st.spacing.interact_size.y = 26.0;
        st.interaction.selectable_labels = false;
    });
}

/// iOS-style switch.
fn toggle(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let size = Vec2::new(40.0, 22.0);
    let (rect, mut resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool_responsive(resp.id, *on);
    let bg = Color32::from_rgb(70, 76, 88).lerp_to_gamma(ACCENT, t);
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(11), bg);
    let x = egui::lerp((rect.left() + 11.0)..=(rect.right() - 11.0), t);
    p.circle_filled(egui::pos2(x, rect.center().y), 8.0, Color32::WHITE);
    resp
}

fn card(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).size(15.0).strong().color(Color32::WHITE));
            ui.add_space(6.0);
            add(ui);
        });
    ui.add_space(12.0);
}

fn label(ui: &mut egui::Ui, text: &str, help: &str) {
    ui.label(RichText::new(text).color(Color32::from_rgb(220, 224, 230)))
        .on_hover_text(help);
}

fn combo_str(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[&str], names: impl Fn(&str) -> String) {
    egui::ComboBox::from_id_salt(id)
        .width(200.0)
        .selected_text(names(value))
        .show_ui(ui, |ui| {
            for o in options {
                ui.selectable_value(value, o.to_string(), names(o));
            }
        });
}

fn text_field(ui: &mut egui::Ui, value: &mut String, hint: &str) {
    ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(260.0));
}

fn device_type_name(t: &str) -> String {
    match t {
        "speaker" => "Enceinte",
        "computer" => "Ordinateur",
        "tablet" => "Tablette",
        "smartphone" => "Téléphone",
        "tv" => "Télévision",
        "avr" => "Ampli audio/vidéo",
        "stb" => "Décodeur TV",
        "audiodongle" => "Clé audio",
        "gameconsole" => "Console de jeu",
        "castaudio" => "Diffusion audio",
        "castvideo" => "Diffusion vidéo",
        "automobile" => "Voiture",
        "smartwatch" => "Montre",
        "chromebook" => "Chromebook",
        "carthing" => "Accessoire voiture",
        "homething" => "Appareil maison",
        other => other,
    }
    .to_string()
}

/// Computer --SSH--> Raspberry Pi (containers) --network--> phone.
/// Read-only file text with comments dimmed and keys highlighted.
fn code_view(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(Color32::from_rgb(14, 16, 19))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for line in text.lines() {
                let t = line.trim_start();
                let col = if t.starts_with('#') {
                    TEXT_DIM
                } else if t.ends_with(':') || t.contains(": ") || t.contains('=') {
                    ACCENT
                } else {
                    Color32::from_rgb(215, 219, 225)
                };
                ui.label(RichText::new(if line.is_empty() { " " } else { line }).monospace().size(12.5).color(col));
            }
        });
}

/// A small picture of the settings page as the browser shows it.
fn browser_preview(ui: &mut egui::Ui, url: &str, s: &Settings) {
    let w = ui.available_width().clamp(260.0, 460.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 210.0), egui::Sense::hover());
    let p = ui.painter();
    let font = |n: f32| egui::FontId::proportional(n);
    p.rect_filled(rect, CornerRadius::same(8), Color32::from_rgb(18, 20, 24));
    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, Color32::from_rgb(70, 76, 88)), egui::StrokeKind::Inside);
    // Window bar with the address.
    let bar = egui::Rect::from_min_size(rect.min, Vec2::new(w, 28.0));
    p.rect_filled(bar, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, Color32::from_rgb(44, 49, 59));
    for (i, c) in [ERR, WARN, ACCENT].iter().enumerate() {
        p.circle_filled(egui::pos2(bar.left() + 12.0 + i as f32 * 12.0, bar.center().y), 4.0, *c);
    }
    let addr = egui::Rect::from_min_max(egui::pos2(bar.left() + 52.0, bar.top() + 5.0), egui::pos2(bar.right() - 8.0, bar.bottom() - 5.0));
    p.rect_filled(addr, CornerRadius::same(4), Color32::from_rgb(14, 16, 19));
    p.text(addr.left_center() + Vec2::new(8.0, 0.0), egui::Align2::LEFT_CENTER, url, egui::FontId::monospace(11.0), Color32::from_rgb(210, 214, 220));
    // Page content.
    let x = rect.left() + 14.0;
    let mut y = bar.bottom() + 12.0;
    p.circle_stroke(egui::pos2(x + 8.0, y + 8.0), 7.0, Stroke::new(2.0, ACCENT));
    p.text(egui::pos2(x + 22.0, y), egui::Align2::LEFT_TOP, "Raspotify Docker — réglages", font(13.0), Color32::WHITE);
    y += 26.0;
    let pill = |x: f32, y: f32, t: &str, on: bool| {
        let galley = p.layout_no_wrap(t.to_string(), font(10.5), if on { ACCENT } else { TEXT_DIM });
        let r = egui::Rect::from_min_size(egui::pos2(x, y), galley.size() + Vec2::new(14.0, 6.0));
        p.rect_stroke(r, CornerRadius::same(9), Stroke::new(1.0, if on { ACCENT } else { TEXT_DIM }), egui::StrokeKind::Inside);
        p.galley(r.min + Vec2::new(7.0, 3.0), galley, Color32::WHITE);
        r.right()
    };
    let nx = pill(x, y, "Lecteur : en marche", true);
    pill(nx + 6.0, y, if s.hotspot.enabled { "Point d'accès : en marche" } else { "Point d'accès : éteint" }, s.hotspot.enabled);
    y += 28.0;
    let name = if s.speaker.name.trim().is_empty() { "raspotify (nom-du-pi)" } else { s.speaker.name.trim() };
    for (label, value) in [("Nom dans Spotify", name), ("Réseau Wi-Fi", if s.hotspot.enabled { s.hotspot.ssid.as_str() } else { "—" })] {
        p.text(egui::pos2(x, y), egui::Align2::LEFT_TOP, label, font(10.5), TEXT_DIM);
        let field = egui::Rect::from_min_size(egui::pos2(x, y + 14.0), Vec2::new(w - 28.0, 20.0));
        p.rect_filled(field, CornerRadius::same(4), Color32::from_rgb(14, 16, 19));
        p.text(field.left_center() + Vec2::new(6.0, 0.0), egui::Align2::LEFT_CENTER, value, font(11.0), Color32::WHITE);
        y += 40.0;
    }
    let btn = egui::Rect::from_min_size(egui::pos2(x, y), Vec2::new(150.0, 22.0));
    p.rect_filled(btn, CornerRadius::same(5), ACCENT_DIM);
    p.text(btn.center(), egui::Align2::CENTER_CENTER, "Enregistrer et appliquer", font(10.5), Color32::WHITE);
}

/// Browser → [Raspberry Pi: player, access point, settings page] ← Spotify app.
fn pi_diagram(ui: &mut egui::Ui, url: &str, hotspot: bool) {
    let w = ui.available_width().min(1400.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 150.0), egui::Sense::hover());
    let p = ui.painter();
    let side = 170.0;
    let gap = 95.0;
    let small = |x: f32, title: &str, sub: &str| {
        let r = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 42.0), Vec2::new(side, 66.0));
        p.rect_filled(r, CornerRadius::same(8), Color32::from_rgb(44, 49, 59));
        p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.0, Color32::from_rgb(70, 76, 88)), egui::StrokeKind::Inside);
        p.text(r.center() - Vec2::new(0.0, 10.0), egui::Align2::CENTER_CENTER, title, egui::FontId::proportional(14.0), Color32::WHITE);
        p.text(r.center() + Vec2::new(0.0, 12.0), egui::Align2::CENTER_CENTER, sub, egui::FontId::proportional(11.0), TEXT_DIM);
        r
    };
    let left = small(rect.left(), "Téléphone ou PC", "navigateur web");
    let right = small(rect.right() - side, "Téléphone", "application Spotify");
    // The Pi, holding the three containers.
    let pi = egui::Rect::from_min_max(
        egui::pos2(left.right() + gap, rect.top() + 4.0),
        egui::pos2(right.left() - gap, rect.bottom() - 4.0),
    );
    p.rect_filled(pi, CornerRadius::same(10), Color32::from_rgb(24, 52, 41));
    p.rect_stroke(pi, CornerRadius::same(10), Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
    p.text(pi.center_top() + Vec2::new(0.0, 16.0), egui::Align2::CENTER_CENTER, "Raspberry Pi", egui::FontId::proportional(15.0), Color32::WHITE);
    let chips: [(&str, bool); 3] = [("Réglages (web)", true), ("Lecteur", true), ("Point d'accès", hotspot)];
    let cw = (pi.width() - 40.0) / 3.0;
    let mut chip_rects = Vec::new();
    for (i, (name, on)) in chips.iter().enumerate() {
        let r = egui::Rect::from_min_size(
            egui::pos2(pi.left() + 10.0 + i as f32 * (cw + 10.0), pi.top() + 38.0),
            Vec2::new(cw, 70.0),
        );
        let (fill, col) = if *on { (ACCENT_DIM, Color32::WHITE) } else { (Color32::from_rgb(40, 44, 52), TEXT_DIM) };
        p.rect_filled(r, CornerRadius::same(7), fill);
        p.text(r.center() - Vec2::new(0.0, 9.0), egui::Align2::CENTER_CENTER, *name, egui::FontId::proportional(12.5), col);
        p.text(
            r.center() + Vec2::new(0.0, 12.0),
            egui::Align2::CENTER_CENTER,
            if *on { "conteneur Docker" } else { "Docker (éteint)" },
            egui::FontId::proportional(10.5),
            TEXT_DIM,
        );
        chip_rects.push(r);
    }
    p.text(pi.center_bottom() - Vec2::new(0.0, 14.0), egui::Align2::CENTER_CENTER, url, egui::FontId::monospace(11.0), ACCENT);
    let arrow = |a: egui::Pos2, b: egui::Pos2, top: &str, bottom: &str| {
        p.arrow(a, b - a, Stroke::new(2.0, ACCENT));
        let mid = egui::pos2((a.x + b.x) / 2.0, a.y);
        p.text(mid - Vec2::new(0.0, 8.0), egui::Align2::CENTER_BOTTOM, top, egui::FontId::proportional(11.0), ACCENT);
        p.text(mid + Vec2::new(0.0, 8.0), egui::Align2::CENTER_TOP, bottom, egui::FontId::proportional(10.5), TEXT_DIM);
    };
    arrow(
        egui::pos2(left.right() + 6.0, left.center().y),
        egui::pos2(pi.left() - 6.0, left.center().y),
        "réglages",
        "http, port web",
    );
    arrow(
        egui::pos2(right.left() - 6.0, right.center().y),
        egui::pos2(pi.right() + 6.0, right.center().y),
        if hotspot { "Wi-Fi du Pi" } else { "même réseau" },
        "Spotify Connect",
    );
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.screenshot_step(&ctx);

        // Keep the person's settings between runs.
        if self.shot.is_none() && self.s != self.saved {
            if self.s.save().is_ok() {
                self.saved = self.s.clone();
            }
        }
        if self.runner.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }

        egui::Panel::top("top")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(18, 12)))
            .show(ui, |ui| self.top_bar(ui));

        egui::Panel::bottom("bottom")
            .resizable(true)
            .default_size(110.0)
            .min_size(70.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 10)))
            .show(ui, |ui| self.bottom_bar(ui));

        egui::Panel::left("nav")
            .exact_size(230.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(12, 14)))
            .show(ui, |ui| self.nav(ui));

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::symmetric(22, 18)))
            .show(ui, |ui| {
                ui.label(RichText::new(self.section.title()).size(22.0).strong().color(Color32::WHITE));
                ui.label(RichText::new(self.section.hint()).color(TEXT_DIM));
                ui.add_space(10.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    match self.section {
                        Section::Speaker => self.page_speaker(ui),
                        Section::Sound => self.page_sound(ui),
                        Section::Advanced => self.page_advanced(ui),
                        Section::Hotspot => self.page_hotspot(ui),
                        Section::Install => self.page_install(ui, &ctx),
                        Section::Docker => self.page_docker(ui, &ctx),
                        Section::Files => self.page_files(ui, &ctx),
                    }
                });
            });

        if let Some((msg, until)) = &self.toast {
            let now = ctx.input(|i| i.time);
            if now > *until {
                self.toast = None;
            } else {
                let msg = msg.clone();
                egui::Area::new(egui::Id::unique("raspotify-toast"))
                    .anchor(egui::Align2::RIGHT_TOP, Vec2::new(-20.0, 70.0))
                    .show(&ctx, |ui| {
                        egui::Frame::new()
                            .fill(ACCENT_DIM)
                            .stroke(Stroke::new(1.0, ACCENT))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(14, 8))
                            .show(ui, |ui| ui.label(RichText::new(msg).color(Color32::WHITE)));
                    });
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
    }
}

impl App {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Simple logo: a speaker cone made of rings.
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), egui::Sense::hover());
            let p = ui.painter();
            p.circle_filled(rect.center(), 18.0, ACCENT_DIM);
            p.circle_stroke(rect.center(), 12.0, Stroke::new(2.5, ACCENT));
            p.circle_filled(rect.center(), 5.0, ACCENT);
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Raspotify Docker").size(20.0).strong().color(Color32::WHITE));
                ui.label(
                    RichText::new("Enceinte Spotify Connect pour Raspberry Pi, dans un conteneur, Wi-Fi en option")
                        .color(TEXT_DIM),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (txt, col) = if self.s.hotspot.enabled {
                    ("Point d'accès : activé", ACCENT)
                } else {
                    ("Point d'accès : désactivé", TEXT_DIM)
                };
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, col))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| ui.label(RichText::new(txt).color(col)));
                let url = self.web_url();
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, ACCENT))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(10, 4))
                    .show(ui, |ui| ui.label(RichText::new(format!("Tout sur le Pi · réglages : {url}")).color(ACCENT)))
                    .response
                    .on_hover_text(
                        "Le lecteur, le point d'accès et la page de réglages tournent tous sur le Pi. \
                         La page s'ouvre depuis le navigateur de n'importe quel téléphone ou ordinateur.",
                    );
            });
        });
    }

    /// Address of the settings page the Pi serves.
    fn web_url(&self) -> String {
        let host = if self.on_pi {
            format!("{}.local", self.host)
        } else {
            let t = self.s.deploy.ssh_target.trim();
            t.rsplit('@').next().unwrap_or(t).to_string()
        };
        format!("http://{host}:{}", self.s.deploy.web_port)
    }

    fn pi_label(&self) -> (String, Color32) {
        let target = self.s.deploy.ssh_target.trim().to_string();
        match self.pi.lock().unwrap().clone() {
            PiStatus::Unknown => (format!("Pi à distance : {target} · non testé"), TEXT_DIM),
            PiStatus::Checking => (format!("Pi à distance : {target} · test…"), WARN),
            PiStatus::Reachable(_) => (format!("Pi à distance : {target} · connecté"), ACCENT),
            PiStatus::Unreachable(_) => (format!("Pi à distance : {target} · injoignable"), ERR),
        }
    }

    /// Tries an SSH connection to the Pi and reads what is running there.
    fn check_pi(&mut self, ctx: &egui::Context) {
        if *self.pi.lock().unwrap() == PiStatus::Checking {
            return;
        }
        *self.pi.lock().unwrap() = PiStatus::Checking;
        let target = self.s.deploy.ssh_target.trim().to_string();
        let r = sh_quote(self.s.deploy.remote_dir.trim());
        let pi = self.pi.clone();
        let log = self.runner.clone();
        let ctx = ctx.clone();
        log.push(format!("▶ Test de la connexion SSH à {target}"));
        let script = format!(
            "echo \"Pi : $(hostname) ($(uname -m))\"; \
             if command -v docker >/dev/null 2>&1; then echo \"Docker : $(docker --version | cut -d, -f1)\"; else echo 'Docker : pas encore installé'; fi; \
             if [ -f {r}/docker-compose.yml ]; then cd {r} && (sudo -n docker compose --profile hotspot ps --format '{{{{.Name}}}} : {{{{.State}}}}' 2>/dev/null || echo 'Conteneurs : inconnus (sudo demande un mot de passe)'); else echo 'Raspotify Docker : pas encore envoyé'; fi"
        );
        std::thread::spawn(move || {
            let out = std::process::Command::new("ssh")
                .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", &target, &script])
                .stdin(std::process::Stdio::null())
                .output();
            let status = match out {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    for l in text.lines() {
                        log.push(l.to_string());
                    }
                    log.push("✔ Pi joignable");
                    PiStatus::Reachable(text)
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    log.push(format!("✖ Pi injoignable : {err}"));
                    if err.contains("Permission denied") {
                        log.push(format!("Astuce : installez votre clé SSH sur le Pi avec « ssh-copy-id {target} »."));
                    }
                    PiStatus::Unreachable(err)
                }
                Err(e) => {
                    log.push(format!("✖ ssh introuvable sur cet ordinateur : {e}"));
                    PiStatus::Unreachable(e.to_string())
                }
            };
            *pi.lock().unwrap() = status;
            ctx.request_repaint();
        });
    }

    fn nav(&mut self, ui: &mut egui::Ui) {
        for sec in Section::ALL {
            let selected = self.section == sec;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 46.0), egui::Sense::click());
            let p = ui.painter();
            if selected {
                p.rect_filled(rect, CornerRadius::same(8), Color32::from_rgb(40, 46, 56));
                p.rect_filled(
                    egui::Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())),
                    CornerRadius::same(2),
                    ACCENT,
                );
            } else if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(8), Color32::from_rgb(34, 38, 46));
            }
            let dot = if sec == Section::Hotspot && self.s.hotspot.enabled { ACCENT } else { TEXT_DIM };
            p.circle_filled(egui::pos2(rect.left() + 18.0, rect.center().y), 4.0, if selected { ACCENT } else { dot });
            p.text(
                egui::pos2(rect.left() + 32.0, rect.top() + 8.0),
                egui::Align2::LEFT_TOP,
                sec.title(),
                egui::FontId::proportional(14.5),
                if selected { Color32::WHITE } else { Color32::from_rgb(210, 214, 220) },
            );
            p.text(
                egui::pos2(rect.left() + 32.0, rect.top() + 27.0),
                egui::Align2::LEFT_TOP,
                sec.hint(),
                egui::FontId::proportional(11.0),
                TEXT_DIM,
            );
            if resp.clicked() {
                self.section = sec;
            }
            ui.add_space(2.0);
        }

        ui.add_space(14.0);
        // What a phone will see.
        egui::Frame::new()
            .fill(CARD)
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Sur le téléphone").color(TEXT_DIM).size(11.5));
                let name = if self.s.speaker.name.trim().is_empty() {
                    "raspotify (nom-du-pi)".to_string()
                } else {
                    self.s.speaker.name.trim().to_string()
                };
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), egui::Sense::hover());
                    ui.painter().circle_stroke(r.center(), 6.0, Stroke::new(2.0, ACCENT));
                    ui.label(RichText::new(name).strong().color(Color32::WHITE));
                });
                ui.label(
                    RichText::new(format!(
                        "{} · {} kbit/s",
                        device_type_name(&self.s.speaker.device_type),
                        self.s.speaker.bitrate
                    ))
                    .color(TEXT_DIM)
                    .size(11.5),
                );
                if self.s.hotspot.enabled {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "Wi-Fi « {} » {}",
                            self.s.hotspot.ssid,
                            if self.s.hotspot.password.is_empty() { "(ouvert)" } else { "(protégé)" }
                        ))
                        .color(ACCENT)
                        .size(12.0),
                    );
                }
            });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let problems = self.s.problems();
        let warnings = self.s.warnings();
        ui.horizontal_wrapped(|ui| {
            if problems.is_empty() {
                ui.label(RichText::new("● Réglages valides").color(ACCENT));
            }
            for p in &problems {
                ui.label(RichText::new(format!("● {p}")).color(ERR));
            }
            for w in &warnings {
                ui.label(RichText::new(format!("● {w}")).color(WARN));
            }
            let running = self.runner.log.lock().unwrap().running.clone();
            if let Some(r) = running {
                ui.spinner();
                ui.label(RichText::new(r).color(TEXT_DIM));
            }
        });
        ui.separator();
        let lines = self.runner.log.lock().unwrap().lines.clone();
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if lines.is_empty() {
                    ui.label(RichText::new("Journal des commandes : rien pour l'instant.").color(TEXT_DIM).monospace());
                }
                for l in &lines {
                    let col = if l.starts_with('✖') {
                        ERR
                    } else if l.starts_with('✔') || l.starts_with('▶') {
                        ACCENT
                    } else {
                        Color32::from_rgb(200, 204, 210)
                    };
                    ui.label(RichText::new(l).monospace().size(12.0).color(col));
                }
            });
    }

    fn page_speaker(&mut self, ui: &mut egui::Ui) {
        let sp = &mut self.s.speaker;
        card(ui, "Identité", |ui| {
            egui::Grid::new("speaker").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                label(ui, "Nom de l'enceinte", "Le nom affiché dans la liste des appareils Spotify.");
                text_field(ui, &mut sp.name, "raspotify (nom-du-pi)");
                ui.end_row();
                label(ui, "Type d'appareil", "L'icône que Spotify affiche.");
                combo_str(ui, "devtype", &mut sp.device_type, DEVICE_TYPES, device_type_name);
                ui.end_row();
                label(ui, "Lecture automatique", "Continuer avec des titres similaires à la fin de la file.");
                combo_str(ui, "autoplay", &mut sp.autoplay, AUTOPLAY, |v| match v {
                    "client" => "Comme le téléphone".into(),
                    "on" => "Toujours".into(),
                    _ => "Jamais".into(),
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new("Laissez le nom vide pour garder celui de raspotify : « raspotify (nom du Pi) ».")
                    .color(TEXT_DIM)
                    .size(12.0),
            );
        });
        card(ui, "Sortie audio", |ui| {
            egui::Grid::new("output").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                label(ui, "Carte son", "Nom ALSA (voir « aplay -L » sur le Pi). Vide : carte par défaut.");
                text_field(ui, &mut sp.device, "par défaut (ex. hw:1,0)");
                ui.end_row();
                label(ui, "Réglage du volume", "softvol : volume logiciel. alsa : volume de la carte son.");
                combo_str(ui, "mixer", &mut sp.mixer, MIXERS, |v| match v {
                    "softvol" => "Logiciel (softvol)".into(),
                    _ => "Carte son (alsa)".into(),
                });
                ui.end_row();
                if sp.mixer == "alsa" {
                    label(ui, "Contrôle ALSA", "Nom du contrôle de volume, ex. PCM ou Master.");
                    text_field(ui, &mut sp.alsa_mixer_control, "PCM");
                    ui.end_row();
                }
                label(ui, "Format de sortie", "S16 convient à presque toutes les cartes.");
                combo_str(ui, "format", &mut sp.format, FORMATS, |v| v.to_string());
                ui.end_row();
            });
        });
    }

    fn page_sound(&mut self, ui: &mut egui::Ui) {
        let sp = &mut self.s.speaker;
        card(ui, "Qualité et volume", |ui| {
            egui::Grid::new("quality").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                label(ui, "Qualité", "Débit du flux Spotify en kbit/s.");
                ui.horizontal(|ui| {
                    for b in BITRATES {
                        let txt = match b {
                            96 => "Normale (96)",
                            160 => "Haute (160)",
                            _ => "Très haute (320)",
                        };
                        ui.selectable_value(&mut sp.bitrate, *b, txt);
                    }
                });
                ui.end_row();
                label(ui, "Volume de départ", "Sinon : 50 % en logiciel, volume actuel de la carte en alsa.");
                ui.horizontal(|ui| {
                    toggle(ui, &mut sp.set_initial_volume);
                    ui.add_enabled(
                        sp.set_initial_volume,
                        egui::Slider::new(&mut sp.initial_volume, 0..=100).suffix(" %"),
                    );
                });
                ui.end_row();
                label(ui, "Courbe du volume", "log est la plus naturelle à l'oreille.");
                combo_str(ui, "vctrl", &mut sp.volume_ctrl, VOLUME_CTRLS, |v| match v {
                    "log" => "Logarithmique".into(),
                    "cubic" => "Cubique".into(),
                    "linear" => "Linéaire".into(),
                    _ => "Fixe (pas de réglage)".into(),
                });
                ui.end_row();
                label(ui, "Plage du volume", "Écart en dB entre le minimum et le maximum.");
                ui.add(egui::Slider::new(&mut sp.volume_range, 0.0..=100.0).suffix(" dB").step_by(1.0));
                ui.end_row();
                label(ui, "Enchaînement sans blanc", "Lecture « gapless » entre deux titres.");
                let mut gapless = !sp.disable_gapless;
                if toggle(ui, &mut gapless).changed() {
                    sp.disable_gapless = !gapless;
                }
                ui.end_row();
                label(ui, "Tramage (dither)", "Automatique : tpdf pour S16/S24, rien sinon.");
                combo_str(ui, "dither", &mut sp.dither, DITHERS, |v| if v == "auto" { "Automatique".into() } else { v.to_string() });
                ui.end_row();
            });
        });
        card(ui, "Normalisation du volume", |ui| {
            ui.horizontal(|ui| {
                toggle(ui, &mut sp.normalisation);
                ui.label("Jouer tous les titres à peu près au même volume (activé par raspotify)");
            });
            ui.add_enabled_ui(sp.normalisation, |ui| {
                egui::Grid::new("norm").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
                    label(ui, "Méthode", "dynamic ajoute un limiteur contre la saturation.");
                    combo_str(ui, "nmeth", &mut sp.norm_method, NORM_METHODS, |v| if v == "dynamic" { "Dynamique".into() } else { "Basique".into() });
                    ui.end_row();
                    label(ui, "Référence", "Par titre, par album ou automatique.");
                    combo_str(ui, "ngain", &mut sp.norm_gain_type, NORM_GAIN_TYPES, |v| match v {
                        "auto" => "Automatique".into(),
                        "track" => "Par titre".into(),
                        _ => "Par album".into(),
                    });
                    ui.end_row();
                    label(ui, "Pré-gain", "De -10 à 10 dB.");
                    ui.add(egui::Slider::new(&mut sp.norm_pregain, -10.0..=10.0).suffix(" dB").step_by(0.5));
                    ui.end_row();
                    label(ui, "Seuil du limiteur", "De -10 à 0 dBFS.");
                    ui.add(egui::Slider::new(&mut sp.norm_threshold, -10.0..=0.0).suffix(" dBFS").step_by(0.5));
                    ui.end_row();
                    label(ui, "Attaque", "De 1 à 500 ms.");
                    ui.add(egui::Slider::new(&mut sp.norm_attack, 1..=500).suffix(" ms"));
                    ui.end_row();
                    label(ui, "Relâchement", "De 1 à 1000 ms.");
                    ui.add(egui::Slider::new(&mut sp.norm_release, 1..=1000).suffix(" ms"));
                    ui.end_row();
                    label(ui, "Genou", "De 0 à 10 dB.");
                    ui.add(egui::Slider::new(&mut sp.norm_knee, 0.0..=10.0).suffix(" dB").step_by(0.5));
                    ui.end_row();
                });
            });
        });
    }

    fn page_advanced(&mut self, ui: &mut egui::Ui) {
        let a = &mut self.s.advanced;
        card(ui, "Cache et carte SD", |ui| {
            egui::Grid::new("cache").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                label(ui, "Ne pas garder la musique", "Évite d'écrire les titres sur la carte SD (réglage raspotify).");
                toggle(ui, &mut a.disable_audio_cache);
                ui.end_row();
                if !a.disable_audio_cache {
                    label(ui, "Taille max du cache", "Avec K, M ou G, ex. 2G.");
                    text_field(ui, &mut a.cache_size_limit, "ex. 2G");
                    ui.end_row();
                }
                label(ui, "Ne pas garder les identifiants", "Inutile tant que la découverte automatique est active.");
                toggle(ui, &mut a.disable_credential_cache);
                ui.end_row();
                label(ui, "Téléchargements en mémoire", "Les titres en cours restent en RAM au lieu du disque.");
                toggle(ui, &mut a.tmp_in_ram);
                ui.end_row();
            });
        });
        card(ui, "Journal", |ui| {
            egui::Grid::new("logs").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                label(ui, "Seulement les erreurs", "Réglage raspotify par défaut.");
                toggle(ui, &mut a.quiet);
                ui.end_row();
                label(ui, "Journal détaillé", "Pour chercher une panne.");
                toggle(ui, &mut a.verbose);
                ui.end_row();
            });
        });
        card(ui, "Connexion", |ui| {
            egui::Grid::new("net").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                label(ui, "Annonce sur le réseau", "Avahi dans le conteneur, ou celui du Pi (partage de /run/dbus).");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut a.avahi_host, false, "Dans le conteneur");
                    ui.selectable_value(&mut a.avahi_host, true, "Celui du Pi");
                });
                ui.end_row();
                label(ui, "Sans découverte automatique", "Il faut alors un jeton d'accès Spotify.");
                toggle(ui, &mut a.disable_discovery);
                ui.end_row();
                label(ui, "Jeton d'accès", "Jeton OAuth avec le droit « streaming ».");
                ui.add(egui::TextEdit::singleline(&mut a.access_token).password(true).hint_text("facultatif").desired_width(260.0));
                ui.end_row();
                label(ui, "Port zeroconf", "Port annoncé (1-65535). Vide : au hasard.");
                text_field(ui, &mut a.zeroconf_port, "automatique");
                ui.end_row();
                label(ui, "Interfaces zeroconf", "Adresses IP séparées par des virgules. Vide : toutes.");
                text_field(ui, &mut a.zeroconf_interface, "toutes");
                ui.end_row();
                label(ui, "Port du serveur Spotify", "80, 443 ou 4070.");
                text_field(ui, &mut a.ap_port, "automatique");
                ui.end_row();
                label(ui, "Proxy HTTP", "http://ip:port");
                text_field(ui, &mut a.proxy, "aucun");
                ui.end_row();
                label(ui, "Programme sur événement", "Lancé à chaque lecture, pause, changement de titre.");
                text_field(ui, &mut a.onevent, "aucun");
                ui.end_row();
            });
        });
    }

    fn page_hotspot(&mut self, ui: &mut egui::Ui) {
        let show_pass = &mut self.show_pass;
        let h = &mut self.s.hotspot;
        card(ui, "Réseau Wi-Fi du Pi", |ui| {
            ui.horizontal(|ui| {
                toggle(ui, &mut h.enabled);
                ui.label(RichText::new(if h.enabled {
                    "Le Pi crée son propre réseau Wi-Fi"
                } else {
                    "Désactivé : le Pi reste sur votre réseau habituel"
                }).strong());
            });
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Les téléphones se connectent directement au Pi et voient l'enceinte. \
                     Spotify ayant besoin d'internet, le Pi doit être relié autrement (câble ou 2e clé Wi-Fi).",
                )
                .color(TEXT_DIM)
                .size(12.0),
            );
        });
        ui.add_enabled_ui(h.enabled, |ui| {
            card(ui, "Nom et sécurité", |ui| {
                egui::Grid::new("ap").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    label(ui, "Nom du réseau", "Ce qui apparaît dans la liste Wi-Fi (1 à 32 caractères).");
                    text_field(ui, &mut h.ssid, "Raspotify");
                    ui.end_row();
                    label(ui, "Mot de passe", "8 à 63 caractères. Vide : réseau ouvert.");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut h.password).password(!*show_pass).hint_text("8 caractères minimum").desired_width(220.0));
                        if ui.small_button(if *show_pass { "Masquer" } else { "Afficher" }).clicked() {
                            *show_pass = !*show_pass;
                        }
                    });
                    ui.end_row();
                    label(ui, "Réseau caché", "Le nom n'est pas diffusé.");
                    toggle(ui, &mut h.hidden);
                    ui.end_row();
                    label(ui, "Canal", "1-13 : 2,4 GHz. 36-48 : 5 GHz (Pi 3B+ et plus récents).");
                    egui::ComboBox::from_id_salt("chan")
                        .width(200.0)
                        .selected_text(format!("{} ({})", h.channel, if h.channel > 14 { "5 GHz" } else { "2,4 GHz" }))
                        .show_ui(ui, |ui| {
                            for c in WIFI_CHANNELS {
                                ui.selectable_value(&mut h.channel, *c, format!("{} ({})", c, if *c > 14 { "5 GHz" } else { "2,4 GHz" }));
                            }
                        });
                    ui.end_row();
                    label(ui, "Pays", "Code à 2 lettres : fixe les canaux autorisés.");
                    ui.add(egui::TextEdit::singleline(&mut h.country).desired_width(60.0));
                    ui.end_row();
                });
            });
            card(ui, "Adresses", |ui| {
                egui::Grid::new("addr").num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
                    label(ui, "Carte Wi-Fi utilisée", "Souvent wlan0 (wlan1 pour une clé USB).");
                    text_field(ui, &mut h.interface, "wlan0");
                    ui.end_row();
                    label(ui, "Adresse du Pi", "Le Pi sur son propre réseau.");
                    text_field(ui, &mut h.address, "192.168.50.1");
                    ui.end_row();
                    label(ui, "Adresses données aux téléphones", "Plage DHCP.");
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut h.dhcp_start).desired_width(120.0));
                        ui.label("à");
                        ui.add(egui::TextEdit::singleline(&mut h.dhcp_end).desired_width(120.0));
                    });
                    ui.end_row();
                    label(ui, "Partager internet", "Les téléphones passent par le Pi pour aller sur internet.");
                    toggle(ui, &mut h.share_internet);
                    ui.end_row();
                    if h.share_internet {
                        label(ui, "Internet du Pi via", "Interface reliée à la box, souvent eth0.");
                        text_field(ui, &mut h.uplink, "eth0");
                        ui.end_row();
                    }
                });
            });
        });
    }

    fn page_install(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let ok = self.s.problems().is_empty();
        let busy = self.runner.busy();
        let url = self.web_url();
        card(ui, "Tout tourne sur le Pi", |ui| {
            pi_diagram(ui, &url, self.s.hotspot.enabled);
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Le lecteur, le point d'accès Wi-Fi et une page de réglages tournent tous sur le Pi, chacun dans \
                     son conteneur. Après l'installation, plus besoin de cette application : les réglages s'ouvrent \
                     depuis le navigateur de n'importe quel téléphone ou ordinateur, et le Pi n'a besoin ni d'écran ni de clavier.",
                )
                .color(TEXT_DIM)
                .size(12.5),
            );
        });
        card(ui, "Ouvrir les réglages depuis votre ordinateur (http)", |ui| {
            let code = self.s.deploy.web_code.trim().to_string();
            let ap_url = format!("http://{}:{}", self.s.hotspot.address.trim(), self.s.deploy.web_port);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(380.0);
                    let step = |ui: &mut egui::Ui, n: &str, t: &str| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(n).strong().color(ACCENT));
                            ui.label(t);
                        });
                    };
                    step(ui, "1", "Sur votre ordinateur (ou téléphone), ouvrez le navigateur.");
                    step(ui, "2", "Tapez l'adresse du Pi :");
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        egui::Frame::new()
                            .fill(Color32::from_rgb(14, 16, 19))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| ui.label(RichText::new(&url).monospace().size(14.0).color(ACCENT)));
                        if ui.button("Copier").clicked() {
                            ctx.copy_text(url.clone());
                            self.notify(ctx, "Adresse copiée");
                        }
                    });
                    step(
                        ui,
                        "3",
                        &if code.is_empty() {
                            "Entrez le code affiché à la fin de l'installation.".to_string()
                        } else {
                            format!("Entrez le code {code} (nom d'utilisateur : n'importe lequel).")
                        },
                    );
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "La page est servie par le conteneur Docker « raspotify-reglages » du Pi : rien à installer \
                             sur l'ordinateur. Connecté au Wi-Fi du Pi, l'adresse est {ap_url}."
                        ))
                        .color(TEXT_DIM)
                        .size(12.0),
                    );
                });
                ui.add_space(16.0);
                browser_preview(ui, &url, &self.s);
            });
        });
        card(ui, "Installer sur ce Pi", |ui| {
            if self.on_pi {
                ui.label(RichText::new(format!("Cette application tourne sur le Pi « {} ».", self.host)).color(ACCENT));
            } else {
                ui.label(
                    RichText::new(
                        "Lancez cette application sur le Pi lui-même (avec un écran), ou copiez le dossier \
                         sur le Pi et lancez la commande plus bas.",
                    )
                    .color(WARN),
                );
            }
            egui::Grid::new("pi_install").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
                label(ui, "Dossier", "Où ranger les fichiers Docker sur le Pi.");
                ui.add(egui::TextEdit::singleline(&mut self.s.deploy.output_dir).desired_width(420.0));
                ui.end_row();
                label(ui, "Paquet raspotify", "auto : la dernière version pour le processeur du Pi. Ou l'adresse d'un .deb.");
                ui.add(egui::TextEdit::singleline(&mut self.s.deploy.deb_url).desired_width(420.0));
                ui.end_row();
                label(ui, "Port de la page de réglages", "La page s'ouvre sur http://<nom-du-pi>.local:<port>.");
                ui.add(egui::DragValue::new(&mut self.s.deploy.web_port).range(1024..=65535));
                ui.end_row();
                label(ui, "Code de la page de réglages", "Demandé par le navigateur (nom d'utilisateur : n'importe lequel).");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.s.deploy.web_code).desired_width(120.0));
                    if ui.small_button("Nouveau code").clicked() {
                        self.s.deploy.web_code = random_code();
                    }
                });
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let label = if self.on_pi { "Installer et démarrer sur ce Pi" } else { "Écrire les fichiers" };
                if ui.add_enabled(ok && !busy, egui::Button::new(RichText::new(label).strong()).fill(ACCENT_DIM)).clicked()
                    && self.write_files(ctx)
                    && self.on_pi
                {
                    let dir = self.out_dir().display().to_string();
                    let script = "if command -v pkexec >/dev/null 2>&1; then pkexec sh \"$PWD/install.sh\"; else sudo -n sh ./install.sh; fi"
                        .to_string();
                    self.runner.run(ctx, "Installation sur ce Pi", &dir, script);
                }
                if self.on_pi && ui.add_enabled(!busy, egui::Button::new("Arrêter")).clicked() {
                    let dir = self.out_dir().display().to_string();
                    let script = "if command -v pkexec >/dev/null 2>&1; then pkexec sh \"$PWD/install.sh\" stop; else sudo -n sh ./install.sh stop; fi"
                        .to_string();
                    self.runner.run(ctx, "Arrêt", &dir, script);
                }
                if ui.button("Ouvrir la page de réglages").clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
                }
            });
            if !ok {
                ui.label(RichText::new("Corrigez d'abord les réglages signalés en rouge en bas.").color(ERR));
            }
        });
        card(ui, "Pi sans écran : une seule commande", |ui| {
            ui.label(RichText::new("Copiez le dossier sur le Pi (clé USB, carte SD…), puis dans un terminal du Pi :").color(TEXT_DIM));
            let cmd = "cd raspotify-docker && sudo ./install.sh".to_string();
            ui.horizontal(|ui| {
                egui::Frame::new()
                    .fill(Color32::from_rgb(14, 16, 19))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::symmetric(10, 6))
                    .show(ui, |ui| ui.label(RichText::new(&cmd).monospace().color(ACCENT)));
                if ui.button("Copier").clicked() {
                    ctx.copy_text(cmd.clone());
                    self.notify(ctx, "Commande copiée");
                }
            });
            ui.label(
                RichText::new(format!(
                    "Elle installe Docker si besoin, démarre les trois conteneurs et affiche l'adresse de la page de réglages (code : {}).",
                    if self.s.deploy.web_code.trim().is_empty() { "choisi au hasard" } else { self.s.deploy.web_code.trim() }
                ))
                .color(TEXT_DIM)
                .size(12.0),
            );
        });
        let show_ssh = self.show_ssh;
        card(ui, "Option : installer depuis un autre ordinateur (SSH)", |ui| {
            if ui.selectable_label(show_ssh, if show_ssh { "▾ Masquer" } else { "▸ Afficher" }).clicked() {
                self.show_ssh = !show_ssh;
            }
            if show_ssh {
                self.ssh_controls(ui, ctx, ok, busy);
            }
        });
    }

    fn ssh_controls(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, ok: bool, busy: bool) {
        {
            let status = self.pi.lock().unwrap().clone();
            ui.horizontal(|ui| {
                let checking = status == PiStatus::Checking;
                if ui.add_enabled(!checking, egui::Button::new("Tester la connexion au Pi")).clicked() {
                    self.check_pi(ctx);
                }
                if checking {
                    ui.spinner();
                }
                let (t, c) = self.pi_label();
                ui.label(RichText::new(t).color(c));
            });
            if let PiStatus::Reachable(info) = &status {
                for l in info.lines() {
                    ui.label(RichText::new(l).monospace().size(12.0).color(ACCENT));
                }
            }
            if let PiStatus::Unreachable(_) = &status {
                ui.label(
                    RichText::new(
                        "Vérifiez que le Pi est allumé, sur le même réseau, que SSH est activé, \
                         et que votre clé SSH y est installée (ssh-copy-id).",
                    )
                    .color(WARN)
                    .size(12.0),
                );
            }
        }
        ui.scope(|ui| {
            ui.horizontal(|ui| {
                label(ui, "Pi (SSH)", "utilisateur@adresse du Pi. Une clé SSH doit être installée (ssh-copy-id).");
                ui.add(egui::TextEdit::singleline(&mut self.s.deploy.ssh_target).desired_width(260.0));
                label(ui, "Dossier sur le Pi", "Relatif au dossier personnel.");
                ui.add(egui::TextEdit::singleline(&mut self.s.deploy.remote_dir).desired_width(160.0));
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(ok && !busy, egui::Button::new(RichText::new("Envoyer et démarrer").strong()).fill(ACCENT_DIM)).clicked()
                    && self.write_files(ctx)
                {
                    let t = sh_quote(self.s.deploy.ssh_target.trim());
                    let r = sh_quote(self.s.deploy.remote_dir.trim());
                    let script = format!(
                        "tar -cf - . | ssh -o BatchMode=yes -o ConnectTimeout=10 {t} \"mkdir -p {r} && tar -C {r} -xf - && cd {r} && sudo -n sh install.sh\""
                    );
                    let dir = self.out_dir().display().to_string();
                    self.runner.run(ctx, "Envoi au Pi et démarrage", &dir, script);
                }
                if ui.add_enabled(!busy, egui::Button::new("Voir le journal du Pi")).clicked() {
                    let t = sh_quote(self.s.deploy.ssh_target.trim());
                    let r = sh_quote(self.s.deploy.remote_dir.trim());
                    let script = format!(
                        "ssh -o BatchMode=yes -o ConnectTimeout=10 {t} \"cd {r} && sudo -n docker compose logs --tail=80\""
                    );
                    self.runner.run(ctx, "Journal du Pi", ".", script);
                }
                if ui.add_enabled(!busy, egui::Button::new("Arrêter sur le Pi")).clicked() {
                    let t = sh_quote(self.s.deploy.ssh_target.trim());
                    let r = sh_quote(self.s.deploy.remote_dir.trim());
                    let script = format!("ssh -o BatchMode=yes -o ConnectTimeout=10 {t} \"cd {r} && sudo -n sh install.sh stop\"");
                    self.runner.run(ctx, "Arrêt sur le Pi", ".", script);
                }
            });
        });
    }

    /// A command in a dark box with a copy button.
    fn command_line(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, cmd: &str) {
        ui.horizontal(|ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(14, 16, 19))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(10, 6))
                .show(ui, |ui| ui.label(RichText::new(cmd).monospace().color(ACCENT)));
            if ui.button("Copier").clicked() {
                ctx.copy_text(cmd.to_string());
                self.notify(ctx, "Commande copiée");
            }
        });
    }

    fn page_docker(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let ok = self.s.problems().is_empty();
        let busy = self.runner.busy();
        let dir = self.out_dir().display().to_string();
        let bits = if self.arm32 { " --32bits" } else { "" };
        let arch = if self.arm32 { "armhf" } else { "arm64" };

        card(ui, "Compiler les images", |ui| {
            ui.label(
                RichText::new(
                    "Trois images Docker : raspotify-docker/spotify (le lecteur), raspotify-docker/hotspot \
                     (le point d'accès) et raspotify-docker/reglages (la page de réglages). Choisissez où les compiler.",
                )
                .color(TEXT_DIM)
                .size(12.5),
            );
            ui.add_space(6.0);

            ui.label(RichText::new("A. Sur le Pi (le plus simple)").strong().color(Color32::WHITE));
            ui.label(RichText::new("Dans le dossier raspotify-docker, sur le Pi. L'installation compile aussi toute seule.").color(TEXT_DIM).size(12.0));
            self.command_line(ui, ctx, "docker compose build");
            ui.add_space(8.0);

            ui.label(RichText::new("B. Sur cet ordinateur, pour le Pi (plus rapide qu'un vieux Pi)").strong().color(Color32::WHITE));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Système du Pi :").color(TEXT_DIM).size(12.0));
                ui.selectable_value(&mut self.arm32, false, "64 bits (arm64)");
                ui.selectable_value(&mut self.arm32, true, "32 bits (armhf)");
            });
            ui.label(
                RichText::new(format!(
                    "Crée raspotify-images-{arch}.tar.gz ; copiez-le sur le Pi avec le dossier, puis chargez-le."
                ))
                .color(TEXT_DIM)
                .size(12.0),
            );
            self.command_line(ui, ctx, &format!("./build-images.sh --pi{bits}"));
            ui.label(RichText::new("Puis sur le Pi :").color(TEXT_DIM).size(12.0));
            self.command_line(ui, ctx, &format!("gunzip -c raspotify-images-{arch}.tar.gz | docker load"));
            ui.add_space(8.0);

            ui.label(RichText::new("C. Sur cet ordinateur, et envoi direct au Pi (SSH)").strong().color(Color32::WHITE));
            let target = self.s.deploy.ssh_target.trim().to_string();
            self.command_line(ui, ctx, &format!("./build-images.sh --pi{bits} --envoyer {target}"));
            ui.label(
                RichText::new(
                    "La première fois, Docker doit pouvoir imiter le processeur du Pi (QEMU) : le script propose \
                     de l'installer. Compiler ainsi prend plusieurs minutes.",
                )
                .color(TEXT_DIM)
                .size(12.0),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(ok && !busy, egui::Button::new(RichText::new("Compiler ici pour le Pi").strong()).fill(ACCENT_DIM)).clicked()
                    && self.write_files(ctx)
                {
                    self.runner.run(ctx, "Compilation des images pour le Pi", &dir, format!("sh ./build-images.sh --pi{bits}"));
                }
                if ui.add_enabled(ok && !busy, egui::Button::new("Compiler et envoyer au Pi")).clicked() && self.write_files(ctx) {
                    let script = format!("sh ./build-images.sh --pi{bits} --envoyer {}", sh_quote(&target));
                    self.runner.run(ctx, "Compilation et envoi au Pi", &dir, script);
                }
            });
        });

        card(ui, "Fichier docker compose (YAML)", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.yaml_images, false, "docker-compose.yml : compile sur le Pi");
                ui.selectable_value(&mut self.yaml_images, true, "docker-compose.images.yml : images déjà compilées");
            });
            let (name, yaml, start) = if self.yaml_images {
                (
                    "docker-compose.images.yml",
                    generate::compose_images(&self.s),
                    "docker compose -f docker-compose.images.yml up -d",
                )
            } else {
                ("docker-compose.yml", generate::compose(&self.s), "docker compose up -d --build")
            };
            ui.label(
                RichText::new(
                    "Il se place dans le dossier raspotify-docker, à côté de raspotify.conf, hotspot.env et .env \
                     (écrits aussi par « Écrire les fichiers »).",
                )
                .color(TEXT_DIM)
                .size(12.0),
            );
            ui.horizontal(|ui| {
                if ui.button("Copier le YAML").clicked() {
                    ctx.copy_text(yaml.clone());
                    self.notify(ctx, format!("{name} copié"));
                }
                if ui.add_enabled(ok, egui::Button::new("Écrire les fichiers")).clicked() {
                    self.write_files(ctx);
                }
                ui.label(RichText::new("Démarrer :").color(TEXT_DIM).size(12.0));
                ui.label(RichText::new(start).monospace().size(12.0).color(ACCENT));
            });
            code_view(ui, &yaml);
        });
    }

    fn page_files(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let files = generate::all(&self.s);
        if self.file_tab >= files.len() {
            self.file_tab = 0;
        }
        ui.horizontal_wrapped(|ui| {
            for (i, f) in files.iter().enumerate() {
                ui.selectable_value(&mut self.file_tab, i, f.name);
            }
        });
        ui.add_space(6.0);
        let f = &files[self.file_tab];
        let mut text = f.content.clone();
        if f.name == "hotspot.env" && !self.s.hotspot.password.is_empty() {
            text = text.replace(
                &format!("AP_PASSWORD={}", self.s.hotspot.password),
                "AP_PASSWORD=••••••••",
            );
        }
        if f.name == "raspotify.conf" && !self.s.advanced.access_token.trim().is_empty() {
            text = text.replace(self.s.advanced.access_token.trim(), "••••••••");
        }
        if ui.button("Copier ce fichier").clicked() {
            ctx.copy_text(f.content.clone());
            self.notify(ctx, format!("{} copié", f.name));
        }
        egui::Frame::new()
            .fill(Color32::from_rgb(14, 16, 19))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for line in text.lines() {
                    let col = if line.trim_start().starts_with('#') {
                        TEXT_DIM
                    } else if line.starts_with("LIBRESPOT_") || line.starts_with("AP_") || line.contains('=') {
                        ACCENT
                    } else {
                        Color32::from_rgb(215, 219, 225)
                    };
                    ui.label(RichText::new(line).monospace().size(12.5).color(col));
                }
            });
    }
}
