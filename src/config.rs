//! Settings the person edits. Defaults follow raspotify's /etc/raspotify/conf
//! and librespot's own help text (see MECHANICS.md).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEVICE_TYPES: &[&str] = &[
    "speaker", "computer", "tablet", "smartphone", "tv", "avr", "stb", "audiodongle",
    "gameconsole", "castaudio", "castvideo", "automobile", "smartwatch", "chromebook",
    "carthing", "homething",
];
pub const BITRATES: &[u32] = &[96, 160, 320];
pub const FORMATS: &[&str] = &["S16", "S24", "S24_3", "S32", "F32", "F64"];
pub const DITHERS: &[&str] = &["auto", "none", "gpdf", "tpdf", "tpdf_hp"];
pub const VOLUME_CTRLS: &[&str] = &["log", "cubic", "linear", "fixed"];
pub const MIXERS: &[&str] = &["softvol", "alsa"];
pub const NORM_METHODS: &[&str] = &["dynamic", "basic"];
pub const NORM_GAIN_TYPES: &[&str] = &["auto", "track", "album"];
pub const AUTOPLAY: &[&str] = &["client", "on", "off"];
pub const WIFI_CHANNELS: &[u32] = &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 36, 40, 44, 48];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Speaker {
    /// Empty = raspotify's default "raspotify (<hostname>)".
    pub name: String,
    pub device_type: String,
    pub bitrate: u32,
    pub set_initial_volume: bool,
    pub initial_volume: u8,
    pub volume_ctrl: String,
    pub volume_range: f32,
    pub mixer: String,
    pub alsa_mixer_control: String,
    pub device: String,
    pub format: String,
    pub dither: String,
    pub autoplay: String,
    pub disable_gapless: bool,
    pub normalisation: bool,
    pub norm_method: String,
    pub norm_gain_type: String,
    pub norm_pregain: f32,
    pub norm_threshold: f32,
    pub norm_attack: u32,
    pub norm_release: u32,
    pub norm_knee: f32,
}

impl Default for Speaker {
    fn default() -> Self {
        Self {
            name: String::new(),
            device_type: "speaker".into(),
            bitrate: 160,
            set_initial_volume: false,
            initial_volume: 50,
            volume_ctrl: "log".into(),
            volume_range: 60.0,
            mixer: "softvol".into(),
            alsa_mixer_control: "PCM".into(),
            device: String::new(),
            format: "S16".into(),
            dither: "auto".into(),
            autoplay: "client".into(),
            disable_gapless: false,
            normalisation: true,
            norm_method: "dynamic".into(),
            norm_gain_type: "auto".into(),
            norm_pregain: 0.0,
            norm_threshold: -2.0,
            norm_attack: 5,
            norm_release: 100,
            norm_knee: 5.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Advanced {
    pub disable_audio_cache: bool,
    pub disable_credential_cache: bool,
    pub cache_size_limit: String,
    pub quiet: bool,
    pub verbose: bool,
    pub tmp_in_ram: bool,
    pub disable_discovery: bool,
    pub access_token: String,
    pub zeroconf_port: String,
    pub zeroconf_interface: String,
    pub ap_port: String,
    pub proxy: String,
    pub onevent: String,
    /// "container": the image runs its own avahi; "host": share the Pi's.
    pub avahi_host: bool,
}

impl Default for Advanced {
    fn default() -> Self {
        Self {
            disable_audio_cache: true,
            disable_credential_cache: true,
            cache_size_limit: String::new(),
            quiet: true,
            verbose: false,
            tmp_in_ram: false,
            disable_discovery: false,
            access_token: String::new(),
            zeroconf_port: String::new(),
            zeroconf_interface: String::new(),
            ap_port: String::new(),
            proxy: String::new(),
            onevent: String::new(),
            avahi_host: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Hotspot {
    pub enabled: bool,
    pub ssid: String,
    pub password: String,
    pub channel: u32,
    pub country: String,
    pub interface: String,
    pub address: String,
    pub dhcp_start: String,
    pub dhcp_end: String,
    pub share_internet: bool,
    pub uplink: String,
    pub hidden: bool,
}

impl Default for Hotspot {
    fn default() -> Self {
        Self {
            enabled: false,
            ssid: "Raspotify".into(),
            password: String::new(),
            channel: 6,
            country: "FR".into(),
            interface: "wlan0".into(),
            address: "192.168.50.1".into(),
            dhcp_start: "192.168.50.10".into(),
            dhcp_end: "192.168.50.100".into(),
            share_internet: true,
            uplink: "eth0".into(),
            hidden: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Deploy {
    pub output_dir: String,
    pub ssh_target: String,
    pub remote_dir: String,
    pub deb_url: String,
    /// Port of the settings page the Pi serves.
    pub web_port: u16,
    /// Code the settings page asks for (empty = none).
    pub web_code: String,
}

impl Default for Deploy {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            output_dir: format!("{home}/raspotify-docker"),
            ssh_target: "pi@raspberrypi.local".into(),
            remote_dir: "raspotify-docker".into(),
            deb_url: "auto".into(),
            web_port: 8080,
            web_code: random_code(),
        }
    }
}

/// A 6-digit code for the settings page.
pub fn random_code() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    format!("{:06}", h.finish() % 1_000_000)
}

/// This machine's host name, to tell whether the app runs on the Pi itself.
pub fn host_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "raspberrypi".into())
}

/// True on a Raspberry Pi (or any ARM Linux board).
pub fn running_on_pi() -> bool {
    std::fs::read_to_string("/proc/device-tree/model")
        .map(|m| m.contains("Raspberry Pi"))
        .unwrap_or(false)
        || cfg!(all(target_os = "linux", any(target_arch = "aarch64", target_arch = "arm")))
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub speaker: Speaker,
    pub advanced: Advanced,
    pub hotspot: Hotspot,
    pub deploy: Deploy,
}

impl Settings {
    pub fn path() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".config")
            });
        base.join("raspotify-docker").join("settings.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d)?;
        }
        std::fs::write(p, serde_json::to_string_pretty(self).unwrap_or_default())
    }

    /// Problems that would stop the speaker or the hotspot from working.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let s = &self.speaker;
        if s.name.contains('"') {
            out.push("Le nom du haut-parleur ne peut pas contenir de guillemets.".into());
        }
        if s.initial_volume > 100 {
            out.push("Le volume de départ doit être entre 0 et 100.".into());
        }
        if !(0.0..=100.0).contains(&s.volume_range) {
            out.push("La plage de volume doit être entre 0 et 100 dB.".into());
        }
        let a = &self.advanced;
        for (label, v) in [("Port zeroconf", &a.zeroconf_port), ("Port du serveur Spotify", &a.ap_port)] {
            if !v.trim().is_empty() && v.trim().parse::<u16>().map(|p| p == 0).unwrap_or(true) {
                out.push(format!("{label} : un nombre entre 1 et 65535."));
            }
        }
        if a.disable_discovery && a.access_token.trim().is_empty() {
            out.push(
                "Sans découverte automatique il faut un jeton d'accès, sinon personne ne peut se connecter."
                    .into(),
            );
        }
        let h = &self.hotspot;
        if h.enabled {
            let n = h.ssid.as_bytes().len();
            if n == 0 || n > 32 {
                out.push("Le nom du réseau Wi-Fi doit faire de 1 à 32 caractères.".into());
            }
            let p = h.password.chars().count();
            if p > 0 && !(8..=63).contains(&p) {
                out.push("Le mot de passe Wi-Fi doit faire de 8 à 63 caractères.".into());
            }
            if h.country.trim().len() != 2 {
                out.push("Le pays Wi-Fi est un code de 2 lettres (FR, BE, CH...).".into());
            }
            for (label, v) in [
                ("Adresse du Pi", &h.address),
                ("Début DHCP", &h.dhcp_start),
                ("Fin DHCP", &h.dhcp_end),
            ] {
                if v.parse::<std::net::Ipv4Addr>().is_err() {
                    out.push(format!("{label} : adresse IPv4 invalide."));
                }
            }
            if h.interface.trim().is_empty() {
                out.push("Indiquez l'interface Wi-Fi (souvent wlan0).".into());
            }
            if h.share_internet && h.uplink.trim() == h.interface.trim() {
                out.push(
                    "Le Pi ne peut pas partager internet par la même carte Wi-Fi que le point d'accès : branchez un câble (eth0) ou une 2e clé Wi-Fi."
                        .into(),
                );
            }
        }
        if self.deploy.web_port < 1024 {
            out.push("Le port de la page de réglages doit être 1024 ou plus (ex. 8080).".into());
        }
        if self.deploy.web_code.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"') {
            out.push("Le code de la page de réglages ne peut contenir ni espace ni guillemet.".into());
        }
        out
    }

    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.deploy.web_code.trim().is_empty() {
            out.push("Code de la page de réglages vide : l'installation en choisira un au hasard.".into());
        }
        if self.hotspot.enabled && self.hotspot.password.is_empty() {
            out.push("Réseau Wi-Fi ouvert : tout le monde à portée pourra s'y connecter.".into());
        }
        if !self.advanced.disable_audio_cache && self.advanced.cache_size_limit.trim().is_empty() {
            out.push("Cache audio sans limite : il peut remplir et user la carte SD.".into());
        }
        out
    }
}
