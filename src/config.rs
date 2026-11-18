use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const DEFAULT_CONFIG: &str = include_str!("../config.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub menu: Menu,
    pub search: Search,
    pub theme: Theme,
    pub keys: Keys,
    #[serde(default)]
    pub modes: BTreeMap<String, CustomMode>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Menu {
    pub mode: String,
    pub prompt: Option<String>,
    pub query: String,
    pub width: i32,
    pub lines: usize,
    pub placement: Placement,
    pub margin: i32,
    pub layer: Layer,
    pub close_on_focus_loss: bool,
    pub wrap_navigation: bool,
    pub activate_on_single_click: bool,
    pub show_icons: bool,
    pub show_descriptions: bool,
    pub empty_message: String,
    pub allow_custom: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    Center,
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Overlay,
    Top,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Search {
    pub matching: Matching,
    pub case_sensitive: bool,
    pub sort: bool,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Matching {
    Fuzzy,
    Substring,
    Prefix,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theme {
    pub background: String,
    pub foreground: String,
    pub muted: String,
    pub input_background: String,
    pub border_color: String,
    pub selected_background: String,
    pub selected_foreground: String,
    pub font_family: String,
    pub font_size: u16,
    pub icon_size: i32,
    pub padding: u16,
    pub spacing: i32,
    pub row_padding: u16,
    pub border_width: u16,
    pub radius: u16,
    pub css: Option<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub page_up: Vec<String>,
    pub page_down: Vec<String>,
    pub accept: Vec<String>,
    pub accept_custom: Vec<String>,
    pub cancel: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomMode {
    pub prompt: Option<String>,
    pub source: Vec<String>,
    pub action: Option<Vec<String>>,
}

impl Config {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let path = explicit.map(Path::to_path_buf).or_else(default_path);
        let source = match path.as_deref().map(fs::read_to_string) {
            Some(Ok(source)) => source,
            Some(Err(error)) if explicit.is_none() && error.kind() == ErrorKind::NotFound => {
                String::new()
            }
            Some(Err(error)) => {
                return Err(error).with_context(|| {
                    format!("could not read config {}", path.as_ref().unwrap().display())
                });
            }
            None => String::new(),
        };

        let load = || -> Result<Self> {
            let mut defaults = DEFAULT_CONFIG.parse::<toml::Table>()?;
            merge(&mut defaults, source.parse::<toml::Table>()?);
            let mut config: Self = defaults.try_into()?;
            config.validate()?;
            if let Some(css) = &mut config.theme.css
                && let Some(parent) = path.as_deref().and_then(Path::parent)
            {
                *css = parent.join(&css);
            }
            Ok(config)
        };
        load().with_context(|| match path {
            Some(path) => format!("invalid config {}", path.display()),
            None => "invalid bundled config".to_owned(),
        })
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=50).contains(&self.menu.lines),
            "menu.lines must be 1–50"
        );
        ensure!(
            (160..=4096).contains(&self.menu.width),
            "menu.width must be 160–4096"
        );
        ensure!(
            (0..=4096).contains(&self.menu.margin),
            "menu.margin must be 0–4096"
        );
        let theme = &self.theme;
        ensure!(
            (1..=128).contains(&theme.font_size),
            "theme.font_size must be 1–128"
        );
        ensure!(
            (1..=128).contains(&theme.icon_size),
            "theme.icon_size must be 1–128"
        );
        ensure!(
            (0..=128).contains(&theme.spacing),
            "theme.spacing must be 0–128"
        );
        for (key, value) in [
            ("padding", theme.padding),
            ("row_padding", theme.row_padding),
            ("border_width", theme.border_width),
            ("radius", theme.radius),
        ] {
            ensure!(value <= 128, "theme.{key} must be 0–128");
        }
        for (key, value) in [
            ("background", &theme.background),
            ("foreground", &theme.foreground),
            ("muted", &theme.muted),
            ("input_background", &theme.input_background),
            ("border_color", &theme.border_color),
            ("selected_background", &theme.selected_background),
            ("selected_foreground", &theme.selected_foreground),
        ] {
            let valid = value.strip_prefix('#').is_some_and(|hex| {
                matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
            });
            ensure!(valid, "theme.{key} must be a hex color");
        }
        ensure!(
            !theme.font_family.trim().is_empty(),
            "theme.font_family must not be empty"
        );
        for (name, mode) in &self.modes {
            ensure!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    && !builtin(name),
                "invalid or reserved mode name {name:?}"
            );
            validate_command(&mode.source).with_context(|| format!("modes.{name}.source"))?;
            if let Some(action) = &mode.action {
                validate_command(action).with_context(|| format!("modes.{name}.action"))?;
                ensure!(
                    action[0] != "{selection}",
                    "modes.{name}.action needs a fixed executable"
                );
            }
        }
        ensure!(
            builtin(&self.menu.mode) || self.modes.contains_key(&self.menu.mode),
            "unknown menu.mode {:?}",
            self.menu.mode
        );
        Ok(())
    }
}

fn builtin(name: &str) -> bool {
    matches!(name, "apps" | "drun" | "run" | "dmenu")
}

fn validate_command(args: &[String]) -> Result<()> {
    ensure!(
        args.first().is_some_and(|arg| !arg.is_empty()),
        "expected a nonempty executable"
    );
    ensure!(
        args.iter().all(|arg| !arg.contains('\0')),
        "arguments must not contain NUL"
    );
    Ok(())
}

fn default_path() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".config"))
        })
        .map(|base| base.join("fzf-menu/config.toml"))
}

fn merge(defaults: &mut toml::Table, overrides: toml::Table) {
    for (key, value) in overrides {
        match (defaults.get_mut(&key), value) {
            (Some(toml::Value::Table(base)), toml::Value::Table(overrides)) => {
                merge(base, overrides)
            }
            (_, value) => {
                defaults.insert(key, value);
            }
        }
    }
}
