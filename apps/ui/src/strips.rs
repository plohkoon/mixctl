use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{InputInfo, RouteInfo};

/// Map of input id → widget references for targeted route updates.
#[allow(dead_code)]
pub(crate) struct RouteWidgets {
    pub scale: gtk4::Scale,
    pub volume_label: gtk4::Label,
    pub mute_button: gtk4::ToggleButton,
    pub name_label: gtk4::Label,
    pub updating: Rc<Cell<bool>>,
}

pub(crate) type WidgetMap = Rc<RefCell<HashMap<u32, RouteWidgets>>>;

pub(crate) async fn load_routes_for_output(
    strips_box: &gtk4::Box,
    widget_map: &WidgetMap,
    css_provider: &gtk4::CssProvider,
    proxy: &MixCtlProxy<'static>,
    output_id: u32,
) {
    if output_id == 0 {
        return;
    }
    let routes = match proxy.list_routes_for_output(output_id).await {
        Ok(r) => r,
        Err(_) => return,
    };
    let inputs = match proxy.list_inputs().await {
        Ok(i) => i,
        Err(_) => return,
    };
    rebuild_route_strips(strips_box, &inputs, &routes, widget_map, css_provider, proxy, output_id);
}

fn build_route_css(inputs: &[InputInfo]) -> String {
    let mut css = String::from(
        ".drop-highlight { outline: 2px dashed @accent_color; outline-offset: -2px; border-radius: 6px; }\n",
    );
    for inp in inputs {
        css.push_str(&format!(
            ".channel-color-{} {{ background-color: {}; min-height: 6px; border-radius: 3px; }}\n",
            inp.id, inp.color
        ));
        css.push_str(&format!(
            ".channel-text-{} {{ color: {}; }}\n",
            inp.id, inp.color
        ));
    }
    css
}

pub(crate) fn rebuild_route_strips(
    strips_box: &gtk4::Box,
    inputs: &[InputInfo],
    routes: &[RouteInfo],
    widget_map: &WidgetMap,
    css_provider: &gtk4::CssProvider,
    proxy: &MixCtlProxy<'static>,
    output_id: u32,
) {
    while let Some(child) = strips_box.first_child() {
        strips_box.remove(&child);
    }
    widget_map.borrow_mut().clear();

    css_provider.load_from_data(&build_route_css(inputs));

    // Build a map of input_id → route for quick lookup
    let route_map: HashMap<u32, &RouteInfo> = routes.iter().map(|r| (r.input_id, r)).collect();

    for inp in inputs {
        if let Some(route) = route_map.get(&inp.id) {
            build_route_strip(strips_box, inp, route, widget_map, proxy, output_id);
        }
    }
}

fn build_route_strip(
    strips_box: &gtk4::Box,
    inp: &InputInfo,
    route: &RouteInfo,
    widget_map: &WidgetMap,
    proxy: &MixCtlProxy<'static>,
    output_id: u32,
) {
    let strip = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    strip.set_width_request(80);
    strip.set_valign(gtk4::Align::Fill);

    // Color bar
    let color_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    color_bar.add_css_class(&format!("channel-color-{}", inp.id));
    color_bar.set_hexpand(true);
    strip.append(&color_bar);

    // Volume label (created before scale so we can reference it in the callback)
    let volume_label = gtk4::Label::new(Some(&format!("{}%", route.volume)));

    // Guard flag to prevent feedback loop when updating from D-Bus signals
    let updating = Rc::new(Cell::new(false));

    // Vertical volume slider
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Vertical, 0.0, 100.0, 1.0);
    scale.set_inverted(true);
    scale.set_value(route.volume as f64);
    scale.set_vexpand(true);

    let input_id = inp.id;
    scale.connect_value_changed(clone!(
        #[strong] proxy,
        #[strong] updating,
        #[weak] volume_label,
        move |scale| {
            // Skip D-Bus call if this change was triggered by update_route_strip
            if updating.get() {
                return;
            }
            let vol = scale.value() as u8;
            volume_label.set_label(&format!("{}%", vol));
            let proxy = proxy.clone();
            glib::spawn_future_local(async move {
                proxy.set_route_volume(input_id, output_id, vol).await.ok();
            });
        }
    ));
    strip.append(&scale);

    // Volume label
    strip.append(&volume_label);

    // Mute toggle
    let mute_button = gtk4::ToggleButton::with_label(if route.muted { "Unmute" } else { "Mute" });
    mute_button.set_active(route.muted);
    mute_button.connect_toggled(clone!(
        #[strong] proxy,
        #[strong] updating,
        move |btn| {
            if updating.get() {
                return;
            }
            let muted = btn.is_active();
            btn.set_label(if muted { "Unmute" } else { "Mute" });
            let proxy = proxy.clone();
            glib::spawn_future_local(async move {
                proxy.set_route_mute(input_id, output_id, muted).await.ok();
            });
        }
    ));
    strip.append(&mute_button);

    // Input name
    let name_label = gtk4::Label::new(Some(&inp.name));
    name_label.add_css_class("heading");
    strip.append(&name_label);

    // Drop target for stream DnD
    let drop_target = gtk4::DropTarget::new(String::static_type(), gtk4::gdk::DragAction::MOVE);
    drop_target.connect_drop(clone!(
        #[strong] proxy,
        move |_, value, _, _| {
            if let Ok(pw_node_id_str) = value.get::<String>() {
                if let Ok(pw_node_id) = pw_node_id_str.parse::<u32>() {
                    let proxy = proxy.clone();
                    glib::spawn_future_local(async move {
                        proxy.assign_stream(pw_node_id, input_id, false).await.ok();
                    });
                    return true;
                }
            }
            false
        }
    ));
    drop_target.connect_enter(clone!(
        #[weak] strip,
        #[upgrade_or] gtk4::gdk::DragAction::empty(),
        move |_, _, _| {
            strip.add_css_class("drop-highlight");
            gtk4::gdk::DragAction::MOVE
        }
    ));
    drop_target.connect_leave(clone!(
        #[weak] strip,
        move |_| {
            strip.remove_css_class("drop-highlight");
        }
    ));
    strip.add_controller(drop_target);

    strips_box.append(&strip);

    widget_map.borrow_mut().insert(inp.id, RouteWidgets {
        scale,
        volume_label,
        mute_button,
        name_label,
        updating,
    });
}

pub(crate) fn update_route_strip(route: &RouteInfo, widget_map: &WidgetMap) {
    let map = widget_map.borrow();
    if let Some(widgets) = map.get(&route.input_id) {
        widgets.updating.set(true);
        widgets.scale.set_value(route.volume as f64);
        widgets.volume_label.set_label(&format!("{}%", route.volume));
        widgets.mute_button.set_active(route.muted);
        widgets.mute_button.set_label(if route.muted { "Unmute" } else { "Mute" });
        widgets.updating.set(false);
    }
}
