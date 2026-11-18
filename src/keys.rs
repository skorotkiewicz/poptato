use crate::config::Keys;
use anyhow::{Context, Result, anyhow, ensure};
use gtk::gdk::{Key, ModifierType as Mods};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub enum Action {
    Up,
    Down,
    PageUp,
    PageDown,
    Accept,
    AcceptCustom,
    Cancel,
}

pub struct Bindings(HashMap<(Key, Mods), Action>);

impl Bindings {
    pub fn new(keys: &Keys) -> Result<Self> {
        let mut bindings = HashMap::new();
        for (name, shortcuts, action) in [
            ("up", &keys.up, Action::Up),
            ("down", &keys.down, Action::Down),
            ("page_up", &keys.page_up, Action::PageUp),
            ("page_down", &keys.page_down, Action::PageDown),
            ("accept", &keys.accept, Action::Accept),
            ("accept_custom", &keys.accept_custom, Action::AcceptCustom),
            ("cancel", &keys.cancel, Action::Cancel),
        ] {
            for shortcut in shortcuts {
                let key = parse(shortcut)
                    .with_context(|| format!("keys.{name} shortcut {shortcut:?}"))?;
                ensure!(
                    bindings.insert(key, action).is_none(),
                    "keys.{name} repeats shortcut {shortcut:?}"
                );
            }
        }
        Ok(Self(bindings))
    }

    pub fn action(&self, key: Key, mods: Mods) -> Option<Action> {
        self.0
            .get(&(
                normalize(key),
                mods & (Mods::CONTROL_MASK | Mods::ALT_MASK | Mods::SHIFT_MASK | Mods::SUPER_MASK),
            ))
            .copied()
    }
}

fn parse(shortcut: &str) -> Result<(Key, Mods)> {
    let mut parts = shortcut.split('+');
    let name = parts.next_back().unwrap_or_default();
    ensure!(
        !name.is_empty() && !name.contains('\0'),
        "expected a key name"
    );
    let key = Key::from_name(name).ok_or_else(|| anyhow!("unknown GDK key {name:?}"))?;
    let mut mods = Mods::empty();
    for part in parts {
        let modifier = match part.to_ascii_lowercase().as_str() {
            "ctrl" => Mods::CONTROL_MASK,
            "alt" => Mods::ALT_MASK,
            "shift" => Mods::SHIFT_MASK,
            "super" => Mods::SUPER_MASK,
            _ => return Err(anyhow!("unknown modifier {part:?}")),
        };
        ensure!(!mods.contains(modifier), "duplicate modifier {part:?}");
        mods |= modifier;
    }
    Ok((normalize(key), mods))
}

fn normalize(key: Key) -> Key {
    if key == Key::ISO_Left_Tab {
        Key::Tab
    } else {
        key.to_lower()
    }
}
