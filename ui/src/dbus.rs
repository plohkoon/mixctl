use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use mixctl_core::dbus::MixCtlProxy;

use crate::sidebar::rebuild_sidebar;
use crate::strips::{load_routes_for_output, update_route_strip, WidgetMap};

pub(crate) async fn connect_and_subscribe(
    strips_box: gtk4::Box,
    sidebar_box: gtk4::Box,
    widget_map: WidgetMap,
    selected_output_id: Rc<Cell<u32>>,
    css_provider: gtk4::CssProvider,
) {
    let conn = zbus::Connection::session().await.unwrap();
    let proxy = MixCtlProxy::new(&conn).await.unwrap();

    // Initial load
    let outputs = proxy.list_outputs().await.unwrap();
    if !outputs.is_empty() {
        selected_output_id.set(outputs[0].id);
    }
    rebuild_sidebar(&sidebar_box, &outputs, &selected_output_id, &strips_box, &widget_map, &css_provider, &proxy);
    load_routes_for_output(&strips_box, &widget_map, &css_provider, &proxy, selected_output_id.get()).await;

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

    // Listen for output state changes (update sidebar)
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
                    rebuild_sidebar(&sidebar_box, &outputs, &selected_output_id, &strips_box, &widget_map, &css_provider, &proxy);
                }
            }
        }
    ));

    // Listen for inputs config changes
    glib::spawn_future_local(clone!(
        #[weak] strips_box,
        #[strong] widget_map,
        #[strong] selected_output_id,
        #[strong] css_provider,
        #[strong] proxy,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_inputs_config_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                load_routes_for_output(&strips_box, &widget_map, &css_provider, &proxy, selected_output_id.get()).await;
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
                    rebuild_sidebar(&sidebar_box, &outputs, &selected_output_id, &strips_box, &widget_map, &css_provider, &proxy);
                    load_routes_for_output(&strips_box, &widget_map, &css_provider, &proxy, selected_output_id.get()).await;
                }
            }
        }
    ));
}
