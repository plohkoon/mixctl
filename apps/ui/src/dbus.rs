use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;
use mixctl_core::dbus::MixCtlProxy;

use crate::capture;
use crate::rules;
use crate::sidebar::rebuild_sidebar;
use crate::streams::rebuild_streams_panel;
use crate::strips::{load_routes_for_output, update_route_strip, WidgetMap};

pub(crate) async fn connect_and_subscribe(
    strips_box: gtk4::Box,
    sidebar_box: gtk4::Box,
    streams_box: gtk4::Box,
    status_label: gtk4::Label,
    rules_button: gtk4::Button,
    devices_button: gtk4::Button,
    window: adw::ApplicationWindow,
    widget_map: WidgetMap,
    selected_output_id: Rc<Cell<u32>>,
    css_provider: gtk4::CssProvider,
) {
    let conn = zbus::Connection::session().await.unwrap();
    let proxy = MixCtlProxy::new(&conn).await.unwrap();

    // Initial audio status
    if let Ok(status) = proxy.get_audio_status().await {
        update_status_label(&status_label, &status);
    }

    // Initial load
    let outputs = proxy.list_outputs().await.unwrap_or_default();
    let inputs = proxy.list_inputs().await.unwrap_or_default();
    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
    if !outputs.is_empty() {
        selected_output_id.set(outputs[0].id);
    }
    rebuild_sidebar(
        &sidebar_box,
        &outputs,
        &inputs,
        default_input_id,
        &selected_output_id,
        &strips_box,
        &widget_map,
        &css_provider,
        &proxy,
    );
    load_routes_for_output(
        &strips_box,
        &widget_map,
        &css_provider,
        &proxy,
        selected_output_id.get(),
    )
    .await;

    // Initial streams
    if let Ok(streams) = proxy.list_streams().await {
        rebuild_streams_panel(&streams_box, &streams, &inputs);
    }

    // Header button handlers
    rules_button.connect_clicked(clone!(
        #[strong] proxy,
        #[weak] window,
        move |_| {
            rules::show_rules_dialog(&window, &proxy);
        }
    ));

    devices_button.connect_clicked(clone!(
        #[strong] proxy,
        #[weak] window,
        move |_| {
            capture::show_capture_dialog(&window, &proxy);
        }
    ));

    // Listen for audio status changes
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] status_label,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_audio_status_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                if let Ok(status) = proxy.get_audio_status().await {
                    update_status_label(&status_label, &status);
                }
            }
        }
    ));

    // Listen for streams changes
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] streams_box,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_streams_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                let inputs = proxy.list_inputs().await.unwrap_or_default();
                if let Ok(streams) = proxy.list_streams().await {
                    rebuild_streams_panel(&streams_box, &streams, &inputs);
                }
            }
        }
    ));

    // Listen for route changes
    glib::spawn_future_local(clone!(
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] proxy,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_route_changed().await.unwrap();
            while let Some(signal) = stream.next().await {
                let args = signal.args().unwrap();
                if args.output_id == selected_output_id.get() {
                    if let Ok(route) = proxy.get_route(args.input_id, args.output_id).await {
                        update_route_strip(&route, &widget_map);
                    }
                }
            }
        }
    ));

    // Listen for output state changes
    glib::spawn_future_local(clone!(
        #[weak] sidebar_box,
        #[weak] strips_box,
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] css_provider,
        #[strong] proxy,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_output_state_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                if let Ok(outputs) = proxy.list_outputs().await {
                    let inputs = proxy.list_inputs().await.unwrap_or_default();
                    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
                    rebuild_sidebar(
                        &sidebar_box,
                        &outputs,
                        &inputs,
                        default_input_id,
                        &selected_output_id,
                        &strips_box,
                        &widget_map,
                        &css_provider,
                        &proxy,
                    );
                }
            }
        }
    ));

    // Listen for inputs config changes
    glib::spawn_future_local(clone!(
        #[weak] strips_box,
        #[weak] sidebar_box,
        #[weak] streams_box,
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] css_provider,
        #[strong] proxy,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_inputs_config_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                load_routes_for_output(
                    &strips_box,
                    &widget_map,
                    &css_provider,
                    &proxy,
                    selected_output_id.get(),
                )
                .await;
                // Also refresh sidebar (input dropdown) and streams
                if let Ok(outputs) = proxy.list_outputs().await {
                    let inputs = proxy.list_inputs().await.unwrap_or_default();
                    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
                    rebuild_sidebar(
                        &sidebar_box,
                        &outputs,
                        &inputs,
                        default_input_id,
                        &selected_output_id,
                        &strips_box,
                        &widget_map,
                        &css_provider,
                        &proxy,
                    );
                    if let Ok(streams) = proxy.list_streams().await {
                        rebuild_streams_panel(&streams_box, &streams, &inputs);
                    }
                }
            }
        }
    ));

    // Listen for outputs config changes
    glib::spawn_future_local(clone!(
        #[weak] sidebar_box,
        #[weak] strips_box,
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] css_provider,
        #[strong] proxy,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_outputs_config_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                if let Ok(outputs) = proxy.list_outputs().await {
                    // If selected output was removed, pick first
                    if !outputs.iter().any(|o| o.id == selected_output_id.get()) {
                        if let Some(first) = outputs.first() {
                            selected_output_id.set(first.id);
                        }
                    }
                    let inputs = proxy.list_inputs().await.unwrap_or_default();
                    let default_input_id = proxy.get_default_input().await.unwrap_or(0);
                    rebuild_sidebar(
                        &sidebar_box,
                        &outputs,
                        &inputs,
                        default_input_id,
                        &selected_output_id,
                        &strips_box,
                        &widget_map,
                        &css_provider,
                        &proxy,
                    );
                    load_routes_for_output(
                        &strips_box,
                        &widget_map,
                        &css_provider,
                        &proxy,
                        selected_output_id.get(),
                    )
                    .await;
                }
            }
        }
    ));
}

fn update_status_label(label: &gtk4::Label, status: &str) {
    if status == "connected" {
        label.set_label("●");
        label.remove_css_class("error");
        label.add_css_class("success");
    } else {
        label.set_label("○");
        label.remove_css_class("success");
        label.add_css_class("error");
    }
}
