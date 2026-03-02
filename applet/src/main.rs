use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use ksni::TrayMethods;
use libadwaita as adw;
use mixctl_core::ChannelInfo;

// -- Messages between threads --

/// Tokio thread → GTK thread
enum AppletMsg {
    TogglePopup,
    Quit,
    ChannelsUpdated(Vec<ChannelInfo>),
    ChannelStateUpdated(ChannelInfo),
}

/// GTK thread → Tokio thread
enum UserAction {
    SetVolume { id: u32, volume: u8 },
    SetMute { id: u32, muted: bool },
}

// -- System tray (runs on tokio thread) --

struct MixCtlTray {
    msg_tx: Mutex<std::sync::mpsc::Sender<AppletMsg>>,
}

impl ksni::Tray for MixCtlTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "mixctl-applet".into()
    }

    fn title(&self) -> String {
        "MixCtl".into()
    }

    fn icon_name(&self) -> String {
        "audio-volume-medium".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.msg_tx.lock().unwrap().send(AppletMsg::TogglePopup).ok();
    }

    fn menu(&self) -> Vec<ksni::menu::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem {
                label: "Open Mixer".into(),
                activate: Box::new(|_: &mut Self| {
                    std::process::Command::new("mixctl-ui").spawn().ok();
                }),
                ..Default::default()
            }
            .into(),
            ksni::menu::MenuItem::Separator,
            ksni::menu::StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.msg_tx.lock().unwrap().send(AppletMsg::Quit).ok();
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

// -- Widget tracking --

struct ChannelWidgets {
    scale: gtk4::Scale,
    volume_label: gtk4::Label,
    mute_button: gtk4::ToggleButton,
    name_label: gtk4::Label,
}

type WidgetMap = Rc<RefCell<HashMap<u32, ChannelWidgets>>>;

// -- GTK application --

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("dev.greghuber.MixCtlApplet")
        .build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    // Keep the app alive even when the popup is hidden.
    // Dropping this guard allows the app to quit.
    let hold_guard = Rc::new(RefCell::new(Some(app.hold())));

    // Channels: tokio→GTK messages, GTK→tokio actions
    let (msg_tx, msg_rx) = std::sync::mpsc::channel::<AppletMsg>();
    let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<UserAction>();
    let action_tx = Rc::new(action_tx);

    // -- Spawn tokio thread for ksni + D-Bus --
    let bg_msg_tx = msg_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            run_background(bg_msg_tx, action_rx).await;
        });
    });

    // -- Build popup window --
    let strips_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    strips_box.set_margin_start(8);
    strips_box.set_margin_end(8);
    strips_box.set_margin_top(8);
    strips_box.set_margin_bottom(8);

    let open_button = gtk4::Button::with_label("Open Mixer");
    open_button.connect_clicked(|_| {
        std::process::Command::new("mixctl-ui").spawn().ok();
    });
    open_button.set_margin_top(4);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.append(&strips_box);
    content.append(&open_button);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MixCtl")
        .default_width(340)
        .resizable(false)
        .content(&content)
        .build();

    let css_provider = gtk4::CssProvider::new();
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css_provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let widget_map: WidgetMap = Rc::new(RefCell::new(HashMap::new()));

    // -- Poll messages from tokio thread --
    glib::timeout_add_local(
        Duration::from_millis(30),
        clone!(
            #[weak] window,
            #[strong] widget_map,
            #[strong] css_provider,
            #[strong] action_tx,
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
                        AppletMsg::ChannelsUpdated(channels) => {
                            rebuild_all_strips(
                                &strips_box,
                                &channels,
                                &widget_map,
                                &css_provider,
                                &action_tx,
                            );
                        }
                        AppletMsg::ChannelStateUpdated(ch) => {
                            update_channel_strip(&ch, &widget_map);
                        }
                    }
                }
                glib::ControlFlow::Continue
            }
        ),
    );

    // Don't present yet — popup appears on tray click
}

// -- Background tokio task: ksni + D-Bus + signals --

async fn run_background(
    msg_tx: std::sync::mpsc::Sender<AppletMsg>,
    mut action_rx: tokio::sync::mpsc::UnboundedReceiver<UserAction>,
) {
    use futures_lite::StreamExt;
    use mixctl_core::dbus::MixCtlProxy;

    let conn = zbus::Connection::session().await.unwrap();
    let proxy = MixCtlProxy::new(&conn).await.unwrap();

    // Send initial channel list
    let channels = proxy.list_channels().await.unwrap();
    msg_tx.send(AppletMsg::ChannelsUpdated(channels)).ok();

    // Spawn ksni tray
    let tray = MixCtlTray {
        msg_tx: Mutex::new(msg_tx.clone()),
    };
    let _tray_handle: ksni::Handle<MixCtlTray> = tray.spawn().await.unwrap();

    // Spawn signal listener: channel state changed
    let state_proxy = proxy.clone();
    let state_tx = msg_tx.clone();
    tokio::spawn(async move {
        let mut stream = state_proxy.receive_channel_state_changed().await.unwrap();
        while let Some(signal) = stream.next().await {
            let args = signal.args().unwrap();
            if let Ok(ch) = state_proxy.get_channel(args.id).await {
                state_tx.send(AppletMsg::ChannelStateUpdated(ch)).ok();
            }
        }
    });

    // Spawn signal listener: channels config changed
    let config_proxy = proxy.clone();
    let config_tx = msg_tx.clone();
    tokio::spawn(async move {
        let mut stream = config_proxy.receive_channels_config_changed().await.unwrap();
        while let Some(_) = stream.next().await {
            if let Ok(channels) = config_proxy.list_channels().await {
                config_tx.send(AppletMsg::ChannelsUpdated(channels)).ok();
            }
        }
    });

    // Process user actions from GTK thread
    while let Some(action) = action_rx.recv().await {
        match action {
            UserAction::SetVolume { id, volume } => {
                proxy.set_channel_volume(id, volume).await.ok();
            }
            UserAction::SetMute { id, muted } => {
                proxy.set_channel_mute(id, muted).await.ok();
            }
        }
    }
}

// -- Widget building --

fn build_channel_css(channels: &[ChannelInfo]) -> String {
    channels
        .iter()
        .map(|ch| {
            format!(
                ".channel-color-{} {{ background-color: {}; min-width: 12px; min-height: 12px; border-radius: 6px; }}",
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
    action_tx: &Rc<tokio::sync::mpsc::UnboundedSender<UserAction>>,
) {
    while let Some(child) = strips_box.first_child() {
        strips_box.remove(&child);
    }
    widget_map.borrow_mut().clear();

    css_provider.load_from_data(&build_channel_css(channels));

    for ch in channels {
        build_channel_strip(strips_box, ch, widget_map, action_tx);
    }
}

fn build_channel_strip(
    strips_box: &gtk4::Box,
    ch: &ChannelInfo,
    widget_map: &WidgetMap,
    action_tx: &Rc<tokio::sync::mpsc::UnboundedSender<UserAction>>,
) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.set_margin_top(2);
    row.set_margin_bottom(2);

    // Color dot
    let color_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    color_dot.add_css_class(&format!("channel-color-{}", ch.id));
    color_dot.set_valign(gtk4::Align::Center);
    row.append(&color_dot);

    // Channel name
    let name_label = gtk4::Label::new(Some(&ch.name));
    name_label.set_width_request(64);
    name_label.set_xalign(0.0);
    row.append(&name_label);

    // Horizontal slider
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(ch.volume as f64);
    scale.set_hexpand(true);

    let id = ch.id;
    scale.connect_value_changed(clone!(
        #[strong] action_tx,
        move |scale| {
            let vol = scale.value() as u8;
            action_tx.send(UserAction::SetVolume { id, volume: vol }).ok();
        }
    ));
    row.append(&scale);

    // Volume label
    let volume_label = gtk4::Label::new(Some(&format!("{:3}%", ch.volume)));
    volume_label.set_width_request(40);
    row.append(&volume_label);

    // Mute toggle
    let mute_button = gtk4::ToggleButton::with_label(if ch.muted { "Unmute" } else { "Mute" });
    mute_button.set_active(ch.muted);
    mute_button.connect_toggled(clone!(
        #[strong] action_tx,
        move |btn| {
            let muted = btn.is_active();
            btn.set_label(if muted { "Unmute" } else { "Mute" });
            action_tx.send(UserAction::SetMute { id, muted }).ok();
        }
    ));
    row.append(&mute_button);

    strips_box.append(&row);

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
        widgets.volume_label.set_label(&format!("{:3}%", ch.volume));
        widgets.mute_button.set_active(ch.muted);
        widgets.mute_button.set_label(if ch.muted { "Unmute" } else { "Mute" });
        widgets.name_label.set_label(&ch.name);
    }
}
