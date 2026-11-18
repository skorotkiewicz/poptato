use crate::config::{Config, Menu, Search, Theme};
use crate::items::Item;
use crate::keys::{Action, Bindings};
use crate::search;
use anyhow::{Context, Result, bail, ensure};
use gtk::{gdk, glib, pango, prelude::*};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::Path;
use std::rc::Rc;

pub enum Selection {
    Item(usize),
    Custom(String),
}

struct Cursor {
    matches: Vec<usize>,
    selected: usize,
    offset: usize,
}

impl Cursor {
    fn new(items: &[Item], query: &str, config: &Search) -> Self {
        Self {
            matches: search::matches(items, query, config),
            selected: 0,
            offset: 0,
        }
    }

    fn move_by(&mut self, delta: isize, lines: usize, wrap: bool) -> bool {
        let len = self.matches.len();
        if len == 0 {
            return false;
        }
        let next = self.selected as isize + delta;
        self.selected = if wrap && delta.unsigned_abs() == 1 {
            next.rem_euclid(len as isize) as usize
        } else {
            next.clamp(0, len as isize - 1) as usize
        };

        let previous_offset = self.offset;
        self.offset = self
            .offset
            .clamp((self.selected + 1).saturating_sub(lines), self.selected);
        previous_offset != self.offset
    }
}

pub fn choose(
    items: Rc<[Item]>,
    config: &Config,
    bindings: Bindings,
    prompt: &str,
    allow_custom: bool,
) -> Result<Option<Selection>> {
    glib::set_prgname(Some("fzf-menu"));
    gtk::init().context("could not open a graphical session")?;
    let display = gdk::Display::default().context("could not get the display")?;
    install_css(&display, &config.theme)?;

    let menu = &config.menu;
    let theme = &config.theme;
    let window = gtk::Window::builder()
        .title("fzf-menu")
        .decorated(false)
        .resizable(false)
        .default_width(menu.width)
        .build();
    window.add_css_class("fzf-menu");
    configure_surface(&window, menu);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, theme.spacing);
    panel.add_css_class("menu-panel");
    window.set_child(Some(&panel));

    let entry = gtk::Entry::builder()
        .placeholder_text(prompt)
        .text(&menu.query)
        .build();
    entry.add_css_class("menu-input");
    panel.append(&entry);

    let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let line_height = i32::from(theme.font_size) + 8;
    let icon_height = if menu.show_icons { theme.icon_size } else { 0 };
    let row_height = line_height.max(icon_height) + 2 * i32::from(theme.row_padding);
    results.set_height_request(row_height * menu.lines as i32);
    panel.append(&results);

    let list = gtk::ListBox::new();
    list.add_css_class("menu-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_activate_on_single_click(menu.activate_on_single_click);
    list.set_focusable(false);
    results.append(&list);

    let empty = gtk::Label::new(Some(&menu.empty_message));
    empty.add_css_class("menu-empty");
    empty.set_ellipsize(pango::EllipsizeMode::End);
    results.append(&empty);

    let event_loop = glib::MainLoop::new(None, false);
    let selected = Rc::new(RefCell::new(None));
    let cursor = RefCell::new(Cursor::new(&items, &menu.query, &config.search));
    // GTK signal handlers own their context until the window is destroyed.
    let view = Rc::new(View {
        items,
        search: Search {
            matching: config.search.matching,
            case_sensitive: config.search.case_sensitive,
            sort: config.search.sort,
        },
        lines: menu.lines,
        wrap_navigation: menu.wrap_navigation,
        icon_size: theme.icon_size,
        show_icons: menu.show_icons,
        show_descriptions: menu.show_descriptions,
        list: list.downgrade(),
        empty: empty.downgrade(),
        cursor,
    });

    list.connect_row_selected({
        let view = view.clone();
        move |_, row| {
            if let Some(row) = row {
                let mut cursor = view.cursor.borrow_mut();
                cursor.selected = cursor.offset + row.index() as usize;
            }
        }
    });
    list.connect_row_activated({
        let view = view.clone();
        let window = window.downgrade();
        let selected = selected.clone();
        move |_, row| {
            let index = {
                let cursor = view.cursor.borrow();
                cursor
                    .matches
                    .get(cursor.offset + row.index() as usize)
                    .copied()
            };
            if let Some(index) = index {
                *selected.borrow_mut() = Some(Selection::Item(index));
                if let Some(window) = window.upgrade() {
                    window.close();
                }
            }
        }
    });

    entry.connect_changed({
        let view = view.clone();
        move |entry| {
            *view.cursor.borrow_mut() =
                Cursor::new(&view.items, entry.text().as_str(), &view.search);
            view.render();
        }
    });

    let keyboard = gtk::EventControllerKey::new();
    keyboard.set_propagation_phase(gtk::PropagationPhase::Capture);
    keyboard.connect_key_pressed({
        let entry = entry.clone();
        let view = view.clone();
        let window = window.downgrade();
        let selected = selected.clone();
        move |_, key, _, modifiers| {
            let Some(action) = bindings.action(key, modifiers) else {
                return glib::Propagation::Proceed;
            };
            match action {
                Action::Up => view.move_by(-1),
                Action::Down => view.move_by(1),
                Action::PageUp => view.move_by(-(view.lines as isize)),
                Action::PageDown => view.move_by(view.lines as isize),
                Action::Cancel => {
                    if let Some(window) = window.upgrade() {
                        window.close();
                    }
                }
                Action::Accept | Action::AcceptCustom => {
                    let cursor = view.cursor.borrow();
                    let matched = cursor.matches.get(cursor.selected).copied();
                    let text = entry.text();
                    let choice = match action {
                        Action::Accept if matched.is_some() => matched.map(Selection::Item),
                        _ if allow_custom && !text.trim().is_empty() => {
                            Some(Selection::Custom(text.to_string()))
                        }
                        _ => None,
                    };
                    drop(cursor);
                    if let Some(choice) = choice {
                        *selected.borrow_mut() = Some(choice);
                        if let Some(window) = window.upgrade() {
                            window.close();
                        }
                    }
                }
            }
            glib::Propagation::Stop
        }
    });
    window.add_controller(keyboard);

    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll({
        let view = view.clone();
        move |_, _, y| {
            if y != 0.0 {
                view.move_by(if y > 0.0 { 1 } else { -1 });
            }
            glib::Propagation::Stop
        }
    });
    results.add_controller(scroll);

    window.connect_close_request({
        let event_loop = event_loop.clone();
        move |_| {
            event_loop.quit();
            glib::Propagation::Proceed
        }
    });
    if menu.close_on_focus_loss {
        let was_active = Cell::new(false);
        window.connect_is_active_notify(move |window| {
            if window.is_active() {
                was_active.set(true);
            } else if was_active.get() {
                window.close();
            }
        });
    }

    view.render();
    entry.grab_focus();
    entry.set_position(-1);
    window.present();
    event_loop.run();
    Ok(selected.take())
}

struct View {
    items: Rc<[Item]>,
    search: Search,
    lines: usize,
    wrap_navigation: bool,
    icon_size: i32,
    show_icons: bool,
    show_descriptions: bool,
    list: glib::WeakRef<gtk::ListBox>,
    empty: glib::WeakRef<gtk::Label>,
    cursor: RefCell<Cursor>,
}

impl View {
    fn render(&self) {
        let (Some(list), Some(empty)) = (self.list.upgrade(), self.empty.upgrade()) else {
            return;
        };
        let visible = {
            let cursor = self.cursor.borrow();
            let end = (cursor.offset + self.lines).min(cursor.matches.len());
            cursor.matches[cursor.offset..end].to_vec()
        };
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        for index in visible {
            list.append(&row(
                &self.items[index],
                self.icon_size,
                self.show_icons,
                self.show_descriptions,
            ));
        }
        empty.set_visible(list.first_child().is_none());
        self.select_current();
    }

    fn select_current(&self) {
        if let Some(list) = self.list.upgrade() {
            let index = {
                let cursor = self.cursor.borrow();
                (cursor.selected - cursor.offset) as i32
            };
            list.select_row(list.row_at_index(index).as_ref());
        }
    }

    fn move_by(&self, delta: isize) {
        let viewport_changed =
            self.cursor
                .borrow_mut()
                .move_by(delta, self.lines, self.wrap_navigation);
        if viewport_changed {
            self.render();
        } else {
            self.select_current();
        }
    }
}

fn display_text(value: &str) -> std::borrow::Cow<'_, str> {
    if value.contains('\0') {
        value.replace('\0', "�").into()
    } else {
        value.into()
    }
}

fn row(item: &Item, icon_size: i32, show_icons: bool, show_descriptions: bool) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("menu-row");
    row.set_focusable(false);
    row.set_focus_on_click(false);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    if show_icons && let Some(icon) = &item.icon {
        let icon = gtk::Image::from_gicon(icon);
        icon.set_pixel_size(icon_size);
        icon.add_css_class("menu-icon");
        content.append(&icon);
    }

    let text = gtk::Label::builder()
        .label(display_text(&item.label).as_ref())
        .hexpand(true)
        .xalign(0.0)
        .single_line_mode(true)
        .ellipsize(pango::EllipsizeMode::End)
        .build();
    text.add_css_class("menu-label");
    content.append(&text);

    if show_descriptions && let Some(description) = &item.description {
        let description = gtk::Label::builder()
            .label(display_text(description).as_ref())
            .single_line_mode(true)
            .ellipsize(pango::EllipsizeMode::End)
            .max_width_chars(24)
            .build();
        description.add_css_class("menu-description");
        content.append(&description);
    }
    row.set_child(Some(&content));
    row
}

#[cfg(feature = "layer-shell")]
fn configure_surface(window: &gtk::Window, menu: &Menu) {
    use crate::config::{Layer, Placement};
    use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};

    if !gtk4_layer_shell::is_supported() {
        return;
    }
    window.init_layer_shell();
    window.set_namespace(Some("fzf-menu"));
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_layer(match menu.layer {
        Layer::Overlay => gtk4_layer_shell::Layer::Overlay,
        Layer::Top => gtk4_layer_shell::Layer::Top,
    });
    let edges: &[Edge] = match menu.placement {
        Placement::Center => &[],
        Placement::Top => &[Edge::Top],
        Placement::Bottom => &[Edge::Bottom],
        Placement::Left => &[Edge::Left],
        Placement::Right => &[Edge::Right],
        Placement::TopLeft => &[Edge::Top, Edge::Left],
        Placement::TopRight => &[Edge::Top, Edge::Right],
        Placement::BottomLeft => &[Edge::Bottom, Edge::Left],
        Placement::BottomRight => &[Edge::Bottom, Edge::Right],
    };
    for &edge in edges {
        window.set_anchor(edge, true);
        window.set_margin(edge, menu.margin);
    }
}

#[cfg(not(feature = "layer-shell"))]
fn configure_surface(_: &gtk::Window, menu: &Menu) {
    let _ = (menu.placement, menu.layer, menu.margin);
}

pub fn read_css(path: &Path) -> Result<String> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("could not read CSS {}", path.display()))?;
    ensure!(
        !source.contains('\0'),
        "CSS {} contains NUL",
        path.display()
    );
    Ok(source)
}

fn install_css(display: &gdk::Display, theme: &Theme) -> Result<()> {
    let family = theme.font_family.replace('\\', "\\\\").replace('"', "\\\"");
    let css = format!(
        r#"
        window.fzf-menu {{ background: transparent; }}
        .fzf-menu .menu-panel {{
            background: {background};
            color: {foreground};
            font-family: "{family}";
            font-size: {font_size}px;
            border: {border_width}px solid {border_color};
            border-radius: {radius}px;
            padding: {padding}px;
        }}
        .fzf-menu .menu-input {{
            background: {input_background};
            color: {foreground};
            border: {border_width}px solid {border_color};
            border-radius: {inner_radius}px;
            padding: {row_padding}px;
            box-shadow: none;
        }}
        .fzf-menu .menu-input placeholder,
        .fzf-menu .menu-description,
        .fzf-menu .menu-empty {{ color: {muted}; }}
        .fzf-menu .menu-list {{ background: transparent; color: {foreground}; }}
        .fzf-menu .menu-row {{
            padding: {row_padding}px;
            border-radius: {inner_radius}px;
        }}
        .fzf-menu .menu-row:hover {{ background: {input_background}; }}
        .fzf-menu .menu-row:selected,
        .fzf-menu .menu-row:selected .menu-description {{
            background: {selected_background};
            color: {selected_foreground};
        }}
        .fzf-menu .menu-empty {{ padding: {row_padding}px; }}
        "#,
        background = theme.background,
        foreground = theme.foreground,
        font_size = theme.font_size,
        border_width = theme.border_width,
        border_color = theme.border_color,
        radius = theme.radius,
        inner_radius = theme.radius.saturating_sub(4),
        padding = theme.padding,
        input_background = theme.input_background,
        row_padding = theme.row_padding,
        muted = theme.muted,
        selected_background = theme.selected_background,
        selected_foreground = theme.selected_foreground,
    );
    add_css(display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION)?;
    if let Some(path) = &theme.css {
        add_css(
            display,
            &read_css(path)?,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        )
        .with_context(|| format!("invalid CSS {}", path.display()))?;
    }
    Ok(())
}

fn add_css(display: &gdk::Display, css: &str, priority: u32) -> Result<()> {
    let provider = gtk::CssProvider::new();
    let error = Rc::new(RefCell::new(None));
    let handler = provider.connect_parsing_error({
        let error = error.clone();
        move |_, section, issue| {
            if issue.is::<gtk::CssParserError>() && error.borrow().is_none() {
                let start = section.start_location();
                *error.borrow_mut() = Some(format!(
                    "{}:{}: {issue}",
                    start.lines() + 1,
                    start.line_chars() + 1
                ));
            }
        }
    });
    provider.load_from_data(css);
    provider.disconnect(handler);
    if let Some(error) = error.take() {
        bail!("{error}");
    }
    gtk::style_context_add_provider_for_display(display, &provider, priority);
    Ok(())
}
