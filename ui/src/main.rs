mod dbus;
mod sidebar;
mod strips;

use std::cell::Cell;
use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;

use strips::WidgetMap;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("dev.greghuber.MixCtlUi")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    // -- Sidebar: output list --
    let sidebar_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    sidebar_box.set_margin_start(8);
    sidebar_box.set_margin_end(8);
    sidebar_box.set_margin_top(8);
    sidebar_box.set_margin_bottom(8);
    sidebar_box.set_width_request(180);

    let sidebar_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&sidebar_box)
        .build();

    // -- Main area: input route sliders --
    let strips_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    strips_box.set_margin_start(12);
    strips_box.set_margin_end(12);
    strips_box.set_margin_top(12);
    strips_box.set_margin_bottom(12);
    strips_box.set_halign(gtk4::Align::Start);

    let main_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&strips_box)
        .build();

    // -- Layout --
    let paned = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let separator = gtk4::Separator::new(gtk4::Orientation::Vertical);
    paned.append(&sidebar_scrolled);
    paned.append(&separator);
    paned.append(&main_scrolled);

    let header = adw::HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new("MixCtl", ""))
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&paned);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MixCtl")
        .default_width(750)
        .default_height(400)
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
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] css_provider,
        async move {
            dbus::connect_and_subscribe(
                strips_box,
                sidebar_box,
                widget_map,
                selected_output_id,
                css_provider,
            ).await;
        }
    ));

    window.present();
}
