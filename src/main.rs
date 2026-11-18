mod config;
mod items;
mod keys;
mod search;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use config::{Config, DEFAULT_CONFIG};
use gtk::prelude::*;
use items::Mode;
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

    /// Select apps, run, dmenu, or a configured custom mode
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

    let default_mode = if io::stdin().is_terminal() {
        config.menu.mode.as_str()
    } else {
        // A desktop key binding may inherit /dev/null rather than a terminal.
        // Only a pipe or redirected file implies stdin selection.
        use std::os::unix::fs::FileTypeExt;
        let piped = std::fs::metadata("/proc/self/fd/0")
            .is_ok_and(|fd| fd.is_file() || fd.file_type().is_fifo() || fd.file_type().is_socket());
        if piped { "dmenu" } else { &config.menu.mode }
    };
    let mode_name = if cli.dmenu {
        "dmenu"
    } else {
        cli.mode.as_deref().unwrap_or(default_mode)
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

    let mode = Mode::resolve(mode_name, &config)?;
    let items: Rc<[items::Item]> = mode.items()?.into();
    if cli.list {
        let mut stdout = io::stdout().lock();
        for index in search::matches(&items, &config.menu.query, &config.search) {
            writeln!(stdout, "{}", items[index].label)?;
        }
        return Ok(true);
    }

    let prompt = cli
        .prompt
        .as_deref()
        .or(config.menu.prompt.as_deref())
        .unwrap_or_else(|| mode.prompt());
    let allow_custom = config.menu.allow_custom && !cli.no_custom && mode.accepts_custom();
    let selection = ui::choose(items.clone(), &config, bindings, prompt, allow_custom)?;
    let Some(selection) = selection else {
        return Ok(false);
    };
    let context = gtk::gdk::Display::default()
        .context("could not get the display")?
        .app_launch_context();
    match selection {
        ui::Selection::Item(index) => {
            mode.activate(Some(&items[index]), "", Some(context.upcast_ref()))?
        }
        ui::Selection::Custom(text) => mode.activate(None, &text, Some(context.upcast_ref()))?,
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
