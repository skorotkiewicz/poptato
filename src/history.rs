use gtk::glib;
use std::env;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

fn directory() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| glib::home_dir().join(".local/state"))
        .join("poptato/recent-apps")
}

fn entry(id: &str) -> PathBuf {
    // Desktop IDs are filenames. Encode every byte to keep them inside this directory.
    let name = glib::base64_encode(id.as_bytes())
        .replace('/', "_")
        .replace('+', "-");
    directory().join(name)
}

pub fn last_launch(id: &str) -> Option<SystemTime> {
    fs::metadata(entry(id)).ok()?.modified().ok()
}

pub fn record(id: &str) -> io::Result<()> {
    let path = entry(id);
    fs::create_dir_all(directory())?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?
        .set_modified(SystemTime::now())
}
