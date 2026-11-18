# Contributing to Poptato

Bug fixes, themes, and documentation are welcome. If you're proposing a larger change, open an issue to discuss how it should work.

## Build and run

Fork the repository, clone your fork, and create a branch for your change. You'll need Rust 1.92 or newer, a C toolchain, `pkg-config`, and development files for GTK4 and GTK4 Layer Shell. See the [build instructions](README.md#get-it-while-its-hot).

Run Poptato from the repository root using the bundled config.

```sh
cargo run --locked -- --config config.toml --mode apps
```

If GTK4 Layer Shell is unavailable, build with `cargo build --locked --no-default-features`.

## Run the checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
```

For changes to the launcher window or key bindings, also try searching and selecting an item in a running window. If you change a config option, update the commented [config reference](config.toml).

## Add a theme

Use [Paper](themes/paper/) as a starting point. Give your theme its own folder under `themes/` with a `config.toml`, a `theme.css`, and a PNG screenshot of the running launcher. Set `css = "theme.css"` under `[theme]` to load the stylesheet from the same folder.

```sh
cargo run --locked -- --config themes/paper/config.toml --check-config
cargo run --locked -- --config themes/paper/config.toml --mode tabs
```

Substitute your theme's path in both commands. Check that the selected row stays readable when you move the pointer over it and when you change the selection with the arrow keys.

## Report a bug or open a pull request

- For a bug report, include your Poptato version (`poptato --version`), the command you ran, steps to reproduce, and what you expected to happen. For a display issue, mention Wayland or X11 and your compositor or desktop environment. Share only the config settings relevant to the bug.
- For a pull request, describe what changed and how you tested it. Include a screenshot for visual changes and link any related issue.
