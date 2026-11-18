use crate::config::{Config, CustomMode};
use anyhow::{Context, Result, anyhow, ensure};
use gio_unix::DesktopAppInfo;
use gtk::gio::{self, prelude::*};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub enum Mode<'a> {
    Apps,
    Run,
    Dmenu,
    Custom(&'a CustomMode),
}

pub struct Item {
    pub label: String,
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub icon: Option<gio::Icon>,
    target: Target,
}

enum Target {
    App(gio::AppInfo),
    Program(PathBuf),
    Text,
}

impl<'a> Mode<'a> {
    pub fn resolve(name: &str, config: &'a Config) -> Result<Self> {
        Ok(match name {
            "apps" | "drun" => Self::Apps,
            "run" => Self::Run,
            "dmenu" => Self::Dmenu,
            name => Self::Custom(
                config
                    .modes
                    .get(name)
                    .ok_or_else(|| anyhow!("unknown mode {name:?}"))?,
            ),
        })
    }

    pub fn prompt(&self) -> &str {
        match self {
            Self::Apps => "Search applications",
            Self::Run => "Run a command",
            Self::Dmenu => "Filter items",
            Self::Custom(mode) => mode.prompt.as_deref().unwrap_or("Filter items"),
        }
    }

    pub fn items(&self) -> Result<Vec<Item>> {
        match self {
            Self::Apps => Ok(applications()),
            Self::Run => Ok(programs()),
            Self::Dmenu => {
                let stdin = io::stdin();
                ensure!(!stdin.is_terminal(), "dmenu mode needs a list on stdin");
                text_items(stdin.lock())
            }
            Self::Custom(mode) => {
                let output = Command::new(&mode.source[0])
                    .args(&mode.source[1..])
                    .stdin(Stdio::null())
                    .stderr(Stdio::inherit())
                    .output()
                    .with_context(|| format!("could not run source {:?}", mode.source[0]))?;
                ensure!(
                    output.status.success(),
                    "source {:?} exited with {}",
                    mode.source[0],
                    output.status
                );
                text_items(output.stdout.as_slice())
            }
        }
    }

    pub fn accepts_custom(&self) -> bool {
        !matches!(self, Self::Apps)
    }

    pub fn activate(
        &self,
        item: Option<&Item>,
        text: &str,
        context: Option<&gio::AppLaunchContext>,
    ) -> Result<()> {
        match item.map(|item| &item.target) {
            Some(Target::App(app)) => app
                .launch(&[], context)
                .with_context(|| format!("could not launch {}", app.display_name())),
            Some(Target::Program(path)) => spawn(path.as_os_str(), std::iter::empty::<&str>()),
            _ => {
                let selection = item.map_or(text, |item| &item.label);
                match self {
                    Self::Run => {
                        let args = shlex::split(selection)
                            .context("unclosed quote or escape in command")?;
                        let (program, args) = args.split_first().context("empty command")?;
                        spawn(program, args)
                    }
                    Self::Custom(mode) if mode.action.is_some() => {
                        let action = mode.action.as_ref().unwrap();
                        let args = action[1..].iter().map(|arg| {
                            if arg == "{selection}" {
                                selection
                            } else {
                                arg.as_str()
                            }
                        });
                        spawn(&action[0], args)
                    }
                    _ => Ok(writeln!(io::stdout().lock(), "{selection}")?),
                }
            }
        }
    }
}

fn spawn(
    program: impl AsRef<std::ffi::OsStr>,
    args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>,
) -> Result<()> {
    Command::new(&program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not launch {:?}", program.as_ref()))?;
    Ok(())
}

fn text_items(source: impl BufRead) -> Result<Vec<Item>> {
    source
        .lines()
        .map(|line| {
            Ok(Item {
                label: line.context("could not read input as UTF-8")?,
                description: None,
                keywords: Vec::new(),
                icon: None,
                target: Target::Text,
            })
        })
        .collect()
}

fn applications() -> Vec<Item> {
    let mut items: Vec<_> = gio::AppInfo::all()
        .into_iter()
        .filter(|app| app.should_show())
        .map(|app| {
            let description = app.description().map(|text| text.to_string());
            let mut keywords: Vec<_> = app
                .executable()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .into_iter()
                .collect();
            if let Some(desktop) = app.downcast_ref::<DesktopAppInfo>() {
                keywords.extend(desktop.generic_name().map(|name| name.to_string()));
                keywords.extend(desktop.keywords().into_iter().map(|word| word.to_string()));
            }
            Item {
                label: app.display_name().to_string(),
                description,
                keywords,
                icon: app.icon(),
                target: Target::App(app),
            }
        })
        .collect();
    items.sort_by_cached_key(|item| item.label.to_lowercase());
    items
}

fn programs() -> Vec<Item> {
    let mut programs = BTreeMap::new();
    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            let directory = if directory.as_os_str().is_empty() {
                Path::new(".")
            } else {
                &directory
            };
            let Ok(entries) = fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(label) = entry.file_name().into_string()
                    && !programs.contains_key(&label)
                    && path.metadata().is_ok_and(|metadata| {
                        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                    })
                {
                    programs.insert(label, path);
                }
            }
        }
    }
    programs
        .into_iter()
        .map(|(label, path)| Item {
            label,
            description: None,
            keywords: Vec::new(),
            icon: None,
            target: Target::Program(path),
        })
        .collect()
}
