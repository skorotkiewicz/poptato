# Poptato

A keyboard-driven Linux launcher written in Rust. Open installed applications,
run commands on your `PATH`, or pipe in a list and print the selected line.
The menu uses GTK and matches as you type.

## Install

Build with Rust 1.92 or newer, `pkg-config`, and the development files for
GTK 4 and GTK4 Layer Shell.

```sh
cargo install --path . --locked
```

On Wayland compositors that support layer shell, the menu opens as a keyboard-focused
overlay. On X11 and other Wayland compositors, it opens as an undecorated window
that your window manager places. To build with GTK 4 alone, add
`--no-default-features` to the install command.
[GTK4 Layer Shell](https://github.com/wmww/gtk4-layer-shell#supported-desktops)

## Use

```sh
poptato --mode apps
poptato --mode run
printf '%s\n' alpha beta gamma | poptato --dmenu -p 'Pick a value'
```

Bind `poptato --mode apps` to a shortcut in your desktop or window manager
to open the launcher from anywhere. With no mode flag, piped or redirected
stdin selects dmenu mode. Otherwise the default is `apps`.

| Key | Action |
| --- | --- |
| Up, Ctrl+K | Previous result |
| Down, Ctrl+J | Next result |
| Page Up, Page Down | Move one page |
| Enter | Accept the highlighted result |
| Shift+Enter | Accept the typed text in modes that allow it |
| Escape, Ctrl+G | Cancel |

Enter also accepts typed text when there are no matches and the mode allows it.
Use `--no-custom` to require a match. In run mode, typed commands support quoted
arguments. Use an explicit shell command if you need shell syntax.

Dmenu mode reads UTF-8 lines from stdin and writes the selection to stdout.
Success exits with status 0, cancellation with 1, and errors with 2.
For a noninteractive search, `--list` prints every match and skips the popup.

```sh
printf '%s\n' alpha beta gamma | poptato --list --query bt
poptato -show drun
poptato --help
```

## Configure

The launcher reads `$XDG_CONFIG_HOME/poptato/config.toml`, or
`~/.config/poptato/config.toml` when `XDG_CONFIG_HOME` is unset, empty, or relative.
Use `--config FILE` to load a different file. You can start from the commented
[default config](config.toml), or create a file with only the settings you want
to change.

These commands create your first config in the default directory.

```sh
mkdir -p ~/.config/poptato
poptato --print-config > ~/.config/poptato/config.toml
```

```toml
[menu]
width = 480
lines = 6
placement = "top"
show_descriptions = false

[search]
matching = "substring"

[theme]
font_size = 16
selected_background = "#a5d6a7"

[keys]
down = ["Down", "Ctrl+n"]
up = ["Up", "Ctrl+p"]
```

`placement`, `margin`, and `layer` apply to layer-shell windows. With
`close_on_focus_loss` enabled, the launcher closes when the window loses focus.
Layer-shell overlays request exclusive keyboard focus.

Set a key's list to `[]` to disable that binding. Key names use GDK spelling,
such as `Return`, `Page_Down`, or `F2`. Modifiers are `Ctrl`, `Alt`, `Shift`,
and `Super`.

For additional styling, set `theme.css = "theme.css"`. Relative paths are
resolved next to `config.toml`. CSS selectors include `.poptato .menu-panel`,
`.menu-input`, `.menu-list`, `.menu-row:selected`, `.menu-label`,
`.menu-description`, `.menu-icon`, and `.menu-empty`.

```sh
poptato --print-config
poptato --config config.toml --check-config
```

## Add a custom mode

A custom mode runs a command to get its list. Its optional action runs when you
choose a result. Each exact `{selection}` argument is replaced with the result
as a single argument, including any spaces or quotes.

```toml
[modes.colors]
prompt = "Pick a color"
source = ["printf", "%s\n", "red", "green", "blue"]
action = ["notify-send", "Color", "{selection}"]
```

Run it with `poptato --mode colors`. Omit `action` to print the selection to
stdout. To preview what the source returns, run `poptato --mode colors --list`.
