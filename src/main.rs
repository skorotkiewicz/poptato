mod config;
mod history;
mod items;
mod keys;
mod search;
mod ui;

use anyhow::{Context, Result, anyhow, ensure};
use clap::Parser;
use config::{Config, DEFAULT_CONFIG};
use gtk::prelude::*;
use items::{Mode, Page};
use keys::Bindings;
use std::ffi::{OsStr, OsString};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;

#[derive(Parser)]
#[command(version, about = "A keyboard-driven launcher and stdin menu")]
struct Cli {
    /// Open stdin selection mode
    #[arg(long, conflicts_with = "mode")]
    dmenu: bool,

    /// Select apps, run, dmenu, tabs, or a configured custom mode
    #[arg(long, visible_alias = "show", short = 'm')]
    mode: Option<String>,

    /// Load a specific config.toml
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Set the input placeholder
    #[arg(long, short = 'p')]
    prompt: Option<String>,

    /// Prefill the search input
    #[arg(long, short = 'q')]
    query: Option<String>,

    /// Set the number of visible rows
    #[arg(long, short = 'l', value_parser = clap::value_parser!(u8).range(1..=50))]
    lines: Option<u8>,

    /// Match case-insensitively
    #[arg(short = 'i', long, conflicts_with = "case_sensitive")]
    insensitive: bool,

    /// Match case-sensitively
    #[arg(long)]
    case_sensitive: bool,

    /// Only accept a result from the list
    #[arg(long)]
    no_custom: bool,

    /// Print all matching entries without opening the menu
    #[arg(long, conflicts_with_all = ["check_config", "print_config"])]
    list: bool,

    /// Validate config.toml without opening the menu
    #[arg(long, conflicts_with = "print_config")]
    check_config: bool,

    /// Print the bundled config.toml
    #[arg(long)]
    print_config: bool,
}

fn main() -> ExitCode {
    match run(Cli::parse_from(args())) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(error) => {
            eprintln!("poptato: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<bool> {
    if cli.print_config {
        io::stdout().lock().write_all(DEFAULT_CONFIG.as_bytes())?;
        return Ok(true);
    }

    let mut config = Config::load(cli.config.as_deref())?;
    let bindings = Bindings::new(&config.keys)?;
    if cli.check_config {
        if let Some(css) = &config.theme.css {
            ui::read_css(css)?;
        }
        return Ok(true);
    }

    use std::os::unix::fs::FileTypeExt;
    let piped = !io::stdin().is_terminal()
        && std::fs::metadata("/proc/self/fd/0")
            .is_ok_and(|fd| fd.is_file() || fd.file_type().is_fifo() || fd.file_type().is_socket());
    let default_mode = if piped && config.menu.mode != "tabs" {
        "dmenu"
    } else {
        &config.menu.mode
    };
    let mode_name = if cli.dmenu {
        "dmenu".to_owned()
    } else {
        cli.mode.unwrap_or_else(|| default_mode.to_owned())
    };
    if let Some(query) = cli.query {
        config::validate_query(&query).context("--query")?;
        config.menu.query = query;
    }
    if let Some(lines) = cli.lines {
        config.menu.lines = usize::from(lines);
    }
    if cli.insensitive || cli.case_sensitive {
        config.search.case_sensitive = cli.case_sensitive;
    }

    let recent_apps = config.menu.recent_apps;
    let pages: Rc<[Page]> = if mode_name == "tabs" {
        ensure!(!cli.list, "--list needs a single mode");
        let mut pages = vec![Page::new("Apps", Mode::Apps), Page::new("Run", Mode::Run)];
        if piped {
            pages.push(Page::new("Stdin", Mode::Dmenu));
        }
        pages.extend(
            config
                .modes
                .iter()
                .map(|(name, mode)| Page::new(name, Mode::Custom(mode.clone()))),
        );
        pages.into()
    } else {
        let page = Page::new(&mode_name, Mode::resolve(&mode_name, &config)?);
        let items = page
            .items(recent_apps)
            .map_err(|error| anyhow!("{error}"))?;
        if cli.list {
            let mut stdout = io::stdout().lock();
            for index in search::matches(items, &config.menu.query, &config.search) {
                writeln!(stdout, "{}", items[index].label)?;
            }
            return Ok(true);
        }
        vec![page].into()
    };

    let prompt = cli.prompt.or_else(|| config.menu.prompt.clone());
    let allow_custom = config.menu.allow_custom && !cli.no_custom;
    let selection = ui::choose(pages.clone(), &config, bindings, prompt, allow_custom)?;
    let Some((page_index, selection)) = selection else {
        return Ok(false);
    };
    let page = &pages[page_index];
    let context = gtk::gdk::Display::default()
        .context("could not get the display")?
        .app_launch_context();
    let context = Some(context.upcast_ref());
    match selection {
        ui::Selection::Item(index) => {
            let items = page
                .items(recent_apps)
                .map_err(|error| anyhow!("{error}"))?;
            page.mode
                .activate(Some(&items[index]), "", context, recent_apps)?;
        }
        ui::Selection::Custom(text) => page.mode.activate(None, &text, context, recent_apps)?,
    }
    Ok(true)
}

fn args() -> impl Iterator<Item = OsString> {
    std::env::args_os().scan(false, |end, arg| {
        if *end {
            return Some(arg);
        }
        Some(match arg.as_os_str() {
            value if value == OsStr::new("--") => {
                *end = true;
                arg
            }
            value if value == OsStr::new("-dmenu") => "--dmenu".into(),
            value if value == OsStr::new("-show") => "--mode".into(),
            _ => arg,
        })
    })
}
