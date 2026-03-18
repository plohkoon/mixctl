mod capture;
mod dbus;
mod rules;
mod settings;
mod sidebar;
mod streams;
mod strips;

use std::cell::Cell;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;

use mixctl_core::config_sections::UiConfig;
use strips::WidgetMap;

fn fetch_ui_config() -> UiConfig {
    match zbus::blocking::Connection::session() {
        Ok(conn) => {
            match conn.call_method(
                Some("dev.greghuber.MixCtl"),
                "/dev/greghuber/MixCtl1",
                Some("dev.greghuber.MixCtl1"),
                "GetConfigSection",
                &("ui",),
            ) {
                Ok(reply) => {
                    let json: String = reply.body().deserialize().unwrap_or_default();
                    serde_json::from_str(&json).unwrap_or_default()
                }
                Err(_) => UiConfig::default(),
            }
        }
        Err(_) => UiConfig::default(),
    }
}

fn main() -> glib::ExitCode {
    let config = fetch_ui_config();
    let app = adw::Application::builder()
        .application_id("dev.greghuber.MixCtlUi")
        .build();
    app.connect_activate(move |app| build_ui(app, &config));
    app.run()
}

fn build_ui(app: &adw::Application, config: &UiConfig) {
    // -- Status indicator --
    let status_label = gtk4::Label::new(Some("○"));
    status_label.add_css_class("error");

    // -- Header bar --
    let title_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    title_box.set_halign(gtk4::Align::Center);
    let title = adw::WindowTitle::new("MixCtl", "");
    title_box.append(&title);
    title_box.append(&status_label);

    let rules_button = gtk4::Button::with_label("Rules");
    let devices_button = gtk4::Button::with_label("Devices");
    let settings_button = gtk4::Button::with_label("Settings");

    let header = adw::HeaderBar::builder()
        .title_widget(&title_box)
        .build();
    header.pack_end(&settings_button);
    header.pack_end(&devices_button);
    header.pack_end(&rules_button);

    // -- Sidebar: output list --
    let sidebar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    sidebar_box.set_margin_start(config.margin);
    sidebar_box.set_margin_end(config.margin);
    sidebar_box.set_margin_top(config.margin);
    sidebar_box.set_margin_bottom(config.margin);
    sidebar_box.set_width_request(180);

    let sidebar_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(false)
        .child(&sidebar_box)
        .build();

    // -- Main area: input route sliders --
    let strips_box = gtk4::Box::new(gtk4::Orientation::Horizontal, config.margin);
    strips_box.set_margin_start(config.margin);
    strips_box.set_margin_end(config.margin);
    strips_box.set_margin_top(config.margin);
    strips_box.set_margin_bottom(config.margin);
    strips_box.set_halign(gtk4::Align::Start);

    let main_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&strips_box)
        .build();

    // -- Streams panel --
    let streams_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    streams_box.set_margin_start(config.margin);
    streams_box.set_margin_end(config.margin);
    streams_box.set_margin_top(config.margin);
    streams_box.set_margin_bottom(config.margin);
    streams_box.set_halign(gtk4::Align::Start);

    let streams_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .child(&streams_box)
        .build();

    // -- Main vertical layout (strips + streams) --
    let main_vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    main_vbox.set_hexpand(true);
    main_vbox.append(&main_scrolled);
    let streams_sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    main_vbox.append(&streams_sep);
    main_vbox.append(&streams_scrolled);

    // -- Layout --
    let paned = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
    paned.append(&sidebar_scrolled);
    paned.append(&separator);
    paned.append(&main_vbox);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&paned);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MixCtl")
        .default_width(config.window_width)
        .default_height(config.window_height)
        .content(&content)
        .build();

    let widget_map: WidgetMap = Rc::new(RefCell::new(HashMap::new()));
    let selected_output_id: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let css_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    glib::spawn_future_local(clone!(
        #[weak] strips_box,
        #[weak] sidebar_box,
        #[weak] streams_box,
        #[weak] status_label,
        #[weak] rules_button,
        #[weak] devices_button,
        #[weak] settings_button,
        #[weak] window,
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] css_provider,
        async move {
            dbus::connect_and_subscribe(
                strips_box,
                sidebar_box,
                streams_box,
                status_label,
                rules_button,
                devices_button,
                settings_button,
                window,
                widget_map,
                selected_output_id,
                css_provider,
            ).await;
        }
    ));

    window.present();
}
