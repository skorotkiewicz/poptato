use std::io::Write;
use std::process::{Command, Output, Stdio};

fn menu(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_poptato"))
        .args(args)
        .env("XDG_CONFIG_HOME", "")
        .env_remove("HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn dmenu_list_fuzzy_matches_and_preserves_original_text() {
    let output = menu(&["-dmenu", "--list", "-q", "ff"], "  Firefox  \nChrome\n");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"  Firefox  \n");
}

#[test]
fn dmenu_list_preserves_order_and_duplicates_for_empty_query() {
    let output = menu(&["--dmenu", "--list"], "zeta\nalpha\nzeta\n");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"zeta\nalpha\nzeta\n");
}

#[test]
fn bundled_config_is_accepted_by_the_cli() {
    let output = menu(&["--config", "config.toml", "--check-config"], "");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(output.stdout, b"");
}
