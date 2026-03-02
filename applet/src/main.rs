mod dbus;
mod output_strips;
mod route_strips;
mod tray;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;
use mixctl_core::{InputInfo, OutputInfo, RouteInfo};

use output_strips::{rebuild_output_strips, update_output_strip, OutputWidgetMap};
use route_strips::{rebuild_route_strips, update_route_strip, RouteWidgetMap};

// -- Messages between threads --

/// Tokio thread → GTK thread
pub(crate) enum AppletMsg {
    TogglePopup,
    Quit,
    FullUpdate {
        outputs: Vec<OutputInfo>,
        inputs: Vec<InputInfo>,
        routes: Vec<RouteInfo>,
    },
    OutputStateUpdated(OutputInfo),
    RouteUpdated(RouteInfo),
}

/// GTK thread → Tokio thread
pub(crate) enum UserAction {
    SetOutputVolume { id: u32, volume: u8 },
    SetOutputMute { id: u32, muted: bool },
    SetRouteVolume { input_id: u32, output_id: u32, volume: u8 },
    SetRouteMute { input_id: u32, output_id: u32, muted: bool },
    SelectOutput { output_id: u32 },
}

// -- GTK application --

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("dev.greghuber.MixCtlApplet")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let hold_guard = Rc::new(RefCell::new(Some(app.hold())));

    let (msg_tx, msg_rx) = std::sync::mpsc::channel::<AppletMsg>();
    let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<UserAction>();
    let action_tx = Rc::new(action_tx);

    // -- Spawn tokio thread for ksni + D-Bus --
    let bg_msg_tx = msg_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            dbus::run_background(bg_msg_tx, action_rx).await;
        });
    });

    // -- Build popup window --

    // Output master section
    let output_section_label = gtk4::Label::new(Some("Output levels"));
    output_section_label.add_css_class("heading");
    output_section_label.set_xalign(0.0);
    output_section_label.set_margin_start(4);

    let output_strips_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    output_strips_box.set_margin_start(8);
    output_strips_box.set_margin_end(8);
    output_strips_box.set_margin_top(4);

    let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    separator.set_margin_top(4);
    separator.set_margin_bottom(4);

    // Input route section
    let route_section_label = Rc::new(gtk4::Label::new(Some("Input mix for: (none)")));
    route_section_label.add_css_class("heading");
    route_section_label.set_xalign(0.0);
    route_section_label.set_margin_start(4);

    let route_strips_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    route_strips_box.set_margin_start(8);
    route_strips_box.set_margin_end(8);
    route_strips_box.set_margin_bottom(4);

    let open_button = gtk4::Button::with_label("Open Mixer");
    open_button.connect_clicked(|_| {
        std::process::Command::new("mixctl-ui").spawn().ok();
    });
    open_button.set_margin_top(4);
    open_button.set_margin_start(8);
    open_button.set_margin_end(8);
    open_button.set_margin_bottom(8);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_margin_top(8);
    content.append(&output_section_label);
    content.append(&output_strips_box);
    content.append(&separator);
    content.append(&*route_section_label);
    content.append(&route_strips_box);
    content.append(&open_button);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MixCtl")
        .default_width(380)
        .resizable(false)
        .content(&content)
        .build();

    let css_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let output_widget_map: OutputWidgetMap = Rc::new(RefCell::new(HashMap::new()));
    let route_widget_map: RouteWidgetMap = Rc::new(RefCell::new(HashMap::new()));
    let selected_output_id: Rc<Cell<u32>> = Rc::new(Cell::new(0));

    // -- Poll messages from tokio thread --
    glib::timeout_add_local(
        Duration::from_millis(30),
        clone!(
            #[weak] window,
            #[strong] output_widget_map,
            #[strong] route_widget_map,
            #[strong] selected_output_id,
            #[strong] css_provider,
            #[strong] action_tx,
            #[strong] route_section_label,
            #[upgrade_or] glib::ControlFlow::Break,
            move || {
                while let Ok(msg) = msg_rx.try_recv() {
                    match msg {
                        AppletMsg::TogglePopup => {
                            if window.is_visible() {
                                window.set_visible(false);
                            } else {
                                window.present();
                            }
                        }
                        AppletMsg::Quit => {
                            hold_guard.borrow_mut().take();
                            window.close();
                            return glib::ControlFlow::Break;
                        }
                        AppletMsg::FullUpdate { outputs, inputs, routes } => {
                            // Pick selected output if not set
                            if selected_output_id.get() == 0 {
                                if let Some(first) = outputs.first() {
                                    selected_output_id.set(first.id);
                                }
                            }
                            // If selected output was removed, pick first
                            if !outputs.iter().any(|o| o.id == selected_output_id.get()) {
                                if let Some(first) = outputs.first() {
                                    selected_output_id.set(first.id);
                                }
                            }
                            rebuild_output_strips(
                                &output_strips_box,
                                &outputs,
                                &output_widget_map,
                                &css_provider,
                                &action_tx,
                                &selected_output_id,
                            );
                            // Update route section label
                            let sel_name = outputs.iter()
                                .find(|o| o.id == selected_output_id.get())
                                .map(|o| o.name.as_str())
                                .unwrap_or("(none)");
                            route_section_label.set_label(&format!("Input mix for: {}", sel_name));
                            // Filter routes for selected output
                            let sel_routes: Vec<&RouteInfo> = routes.iter()
                                .filter(|r| r.output_id == selected_output_id.get())
                                .collect();
                            rebuild_route_strips(
                                &route_strips_box,
                                &inputs,
                                &sel_routes,
                                &route_widget_map,
                                &css_provider,
                                &action_tx,
                                selected_output_id.get(),
                            );
                        }
                        AppletMsg::OutputStateUpdated(out) => {
                            update_output_strip(&out, &output_widget_map);
                        }
                        AppletMsg::RouteUpdated(route) => {
                            if route.output_id == selected_output_id.get() {
                                update_route_strip(&route, &route_widget_map);
                            }
                        }
                    }
                }
                glib::ControlFlow::Continue
            }
        ),
    );

    // Don't present yet — popup appears on tray click
}
