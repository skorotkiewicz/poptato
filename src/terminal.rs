use gtk::{gio, glib, prelude::*};
use std::cell::{Cell, OnceCell};
use std::rc::Rc;
use vte4::prelude::*;

pub struct TerminalTab {
    pub widget: gtk::ScrolledWindow,
    terminal: OnceCell<vte4::Terminal>,
    running: Rc<Cell<bool>>,
    spawn: gio::Cancellable,
}

impl TerminalTab {
    pub fn new(height: i32) -> Self {
        let widget = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .height_request(height)
            .visible(false)
            .build();
        widget.add_css_class("menu-terminal");
        Self {
            widget,
            terminal: OnceCell::new(),
            running: Rc::new(Cell::new(false)),
            spawn: gio::Cancellable::new(),
        }
    }

    pub fn running(&self) -> bool {
        self.running.get()
    }

    pub fn show(&self) {
        let terminal = self.terminal.get_or_init(|| {
            let terminal = vte4::Terminal::new();
            terminal.set_audible_bell(false);
            terminal.connect_child_exited({
                let running = self.running.clone();
                move |terminal, _| {
                    running.set(false);
                    terminal.feed(b"\r\n[Shell exited. Reopen the Terminal tab to restart.]\r\n");
                }
            });
            self.widget.set_child(Some(&terminal));
            terminal
        });
        self.widget.set_visible(true);
        terminal.grab_focus();
        if self.running.replace(true) {
            return;
        }
        terminal.reset(true, false);
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        terminal.spawn_async(
            vte4::PtyFlags::DEFAULT,
            None,
            &[&shell],
            &[],
            glib::SpawnFlags::SEARCH_PATH,
            || {},
            -1,
            Some(&self.spawn),
            {
                let running = self.running.clone();
                let terminal = terminal.downgrade();
                move |result| {
                    if let Err(error) = result {
                        running.set(false);
                        if let Some(terminal) = terminal.upgrade() {
                            let message: String = error
                                .to_string()
                                .chars()
                                .filter(|c| !c.is_control())
                                .collect();
                            terminal
                                .feed(format!("[Could not start shell: {message}]\r\n").as_bytes());
                        }
                    }
                }
            },
        );
    }

    pub fn cancel_spawn(&self) {
        self.spawn.cancel();
    }
}
