use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;
use mixctl_core::ChannelInfo;
use mixctl_core::dbus::MixCtlProxy;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("dev.greghuber.MixCtlUi")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

/// Map of channel id → widget references for targeted updates.
struct ChannelWidgets {
    scale: gtk4::Scale,
    volume_label: gtk4::Label,
    mute_button: gtk4::ToggleButton,
    name_label: gtk4::Label,
}

type WidgetMap = Rc<RefCell<HashMap<u32, ChannelWidgets>>>;

fn build_ui(app: &adw::Application) {
    let strips_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    strips_box.set_margin_start(12);
    strips_box.set_margin_end(12);
    strips_box.set_margin_top(12);
    strips_box.set_margin_bottom(12);
    strips_box.set_halign(gtk4::Align::Start);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .hexpand(true)
        .vexpand(true)
        .child(&strips_box)
        .build();

    let header = adw::HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new("MixCtl", ""))
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&scrolled);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MixCtl")
        .default_width(600)
        .default_height(400)
        .content(&content)
        .build();

    let widget_map: WidgetMap = Rc::new(RefCell::new(HashMap::new()));
    let css_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Initial channel load + signal subscriptions
    glib::spawn_future_local(clone!(
        #[weak] strips_box,
        #[strong] widget_map,
        #[strong] css_provider,
        async move {
            let conn = zbus::Connection::session().await.unwrap();
            let proxy = MixCtlProxy::new(&conn).await.unwrap();
            let channels = proxy.list_channels().await.unwrap();
            rebuild_all_strips(&strips_box, &channels, &widget_map, &css_provider, &proxy);

            // Listen for state changes (volume/mute)
            glib::spawn_future_local(clone!(
                #[strong] widget_map,
                #[strong] proxy,
                async move {
                    use futures_lite::StreamExt;
                    let mut stream = proxy.receive_channel_state_changed().await.unwrap();
                    while let Some(signal) = stream.next().await {
                        let args = signal.args().unwrap();
                        let id = args.id;
                        if let Ok(ch) = proxy.get_channel(id).await {
                            update_channel_strip(&ch, &widget_map);
                        }
                    }
                }
            ));

            // Listen for config changes (add/remove/reorder/rename/color)
            glib::spawn_future_local(clone!(
                #[weak] strips_box,
                #[strong] widget_map,
                #[strong] css_provider,
                #[strong] proxy,
                async move {
                    use futures_lite::StreamExt;
                    let mut stream = proxy.receive_channels_config_changed().await.unwrap();
                    while let Some(_) = stream.next().await {
                        if let Ok(channels) = proxy.list_channels().await {
                            rebuild_all_strips(&strips_box, &channels, &widget_map, &css_provider, &proxy);
                        }
                    }
                }
            ));
        }
    ));

    window.present();
}

fn build_channel_css(channels: &[ChannelInfo]) -> String {
    channels
        .iter()
        .map(|ch| {
            format!(
                ".channel-color-{} {{ background-color: {}; min-height: 6px; border-radius: 3px; }}",
                ch.id, ch.color
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rebuild_all_strips(
    strips_box: &gtk4::Box,
    channels: &[ChannelInfo],
    widget_map: &WidgetMap,
    css_provider: &gtk4::CssProvider,
    proxy: &MixCtlProxy<'static>,
) {
    // Remove all existing strips
    while let Some(child) = strips_box.first_child() {
        strips_box.remove(&child);
    }
    widget_map.borrow_mut().clear();

    // Update CSS
    css_provider.load_from_data(&build_channel_css(channels));

    // Build strips
    for ch in channels {
        build_channel_strip(strips_box, ch, widget_map, proxy);
    }
}

fn build_channel_strip(
    strips_box: &gtk4::Box,
    ch: &ChannelInfo,
    widget_map: &WidgetMap,
    proxy: &MixCtlProxy<'static>,
) {
    let strip = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    strip.set_width_request(80);
    strip.set_valign(gtk4::Align::Fill);

    // Color bar
    let color_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    color_bar.add_css_class(&format!("channel-color-{}", ch.id));
    color_bar.set_hexpand(true);
    strip.append(&color_bar);

    // Vertical volume slider
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Vertical, 0.0, 100.0, 1.0);
    scale.set_inverted(true);
    scale.set_value(ch.volume as f64);
    scale.set_vexpand(true);

    // Wire slider → D-Bus
    let id = ch.id;
    scale.connect_value_changed(clone!(
        #[strong] proxy,
        move |scale| {
            let vol = scale.value() as u8;
            let proxy = proxy.clone();
            glib::spawn_future_local(async move {
                proxy.set_channel_volume(id, vol).await.ok();
            });
        }
    ));
    strip.append(&scale);

    // Volume label
    let volume_label = gtk4::Label::new(Some(&format!("{}%", ch.volume)));
    strip.append(&volume_label);

    // Mute toggle
    let mute_button = gtk4::ToggleButton::with_label(if ch.muted { "Unmute" } else { "Mute" });
    mute_button.set_active(ch.muted);
    mute_button.connect_toggled(clone!(
        #[strong] proxy,
        move |btn| {
            let muted = btn.is_active();
            btn.set_label(if muted { "Unmute" } else { "Mute" });
            let proxy = proxy.clone();
            glib::spawn_future_local(async move {
                proxy.set_channel_mute(id, muted).await.ok();
            });
        }
    ));
    strip.append(&mute_button);

    // Channel name
    let name_label = gtk4::Label::new(Some(&ch.name));
    name_label.add_css_class("heading");
    strip.append(&name_label);

    strips_box.append(&strip);

    widget_map.borrow_mut().insert(ch.id, ChannelWidgets {
        scale,
        volume_label,
        mute_button,
        name_label,
    });
}

fn update_channel_strip(ch: &ChannelInfo, widget_map: &WidgetMap) {
    let map = widget_map.borrow();
    if let Some(widgets) = map.get(&ch.id) {
        widgets.scale.set_value(ch.volume as f64);
        widgets.volume_label.set_label(&format!("{}%", ch.volume));
        widgets.mute_button.set_active(ch.muted);
        widgets.mute_button.set_label(if ch.muted { "Unmute" } else { "Mute" });
        widgets.name_label.set_label(&ch.name);
    }
}
