use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib::clone;
use gtk4::prelude::*;
use mixctl_core::{InputInfo, RouteInfo};

use crate::UserAction;

#[allow(dead_code)]
pub(crate) struct RouteWidgets {
    pub scale: gtk4::Scale,
    pub volume_label: gtk4::Label,
    pub mute_button: gtk4::ToggleButton,
    pub name_label: gtk4::Label,
}

pub(crate) type RouteWidgetMap = Rc<RefCell<HashMap<u32, RouteWidgets>>>;

pub(crate) fn build_route_css(inputs: &[InputInfo]) -> String {
    inputs
        .iter()
        .map(|inp| {
            format!(
                ".route-color-{} {{ background-color: {}; min-width: 12px; min-height: 12px; border-radius: 6px; }}",
                inp.id, inp.color
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn rebuild_route_strips(
    strips_box: &gtk4::Box,
    inputs: &[InputInfo],
    routes: &[&RouteInfo],
    widget_map: &RouteWidgetMap,
    css_provider: &gtk4::CssProvider,
    action_tx: &Rc<tokio::sync::mpsc::UnboundedSender<UserAction>>,
    output_id: u32,
) {
    while let Some(child) = strips_box.first_child() {
        strips_box.remove(&child);
    }
    widget_map.borrow_mut().clear();

    // Reload CSS with both output and route colors
    let css = build_route_css(inputs);
    let existing = css_provider.to_str().to_string();
    let merged = format!("{}\n{}", existing, css);
    css_provider.load_from_data(&merged);

    let route_map: HashMap<u32, &&RouteInfo> = routes.iter().map(|r| (r.input_id, r)).collect();

    for inp in inputs {
        if let Some(route) = route_map.get(&inp.id) {
            build_route_strip(strips_box, inp, route, widget_map, action_tx, output_id);
        }
    }
}

fn build_route_strip(
    strips_box: &gtk4::Box,
    inp: &InputInfo,
    route: &RouteInfo,
    widget_map: &RouteWidgetMap,
    action_tx: &Rc<tokio::sync::mpsc::UnboundedSender<UserAction>>,
    output_id: u32,
) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.set_margin_top(2);
    row.set_margin_bottom(2);

    // Color dot
    let color_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    color_dot.add_css_class(&format!("route-color-{}", inp.id));
    color_dot.set_valign(gtk4::Align::Center);
    row.append(&color_dot);

    // Input name
    let name_label = gtk4::Label::new(Some(&inp.name));
    name_label.set_width_request(64);
    name_label.set_xalign(0.0);
    row.append(&name_label);

    // Horizontal slider
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(route.volume as f64);
    scale.set_hexpand(true);

    let input_id = inp.id;
    scale.connect_value_changed(clone!(
        #[strong] action_tx,
        move |scale| {
            let vol = scale.value() as u8;
            action_tx.send(UserAction::SetRouteVolume { input_id, output_id, volume: vol }).ok();
        }
    ));
    row.append(&scale);

    // Volume label
    let volume_label = gtk4::Label::new(Some(&format!("{:3}%", route.volume)));
    volume_label.set_width_request(40);
    row.append(&volume_label);

    // Mute toggle
    let mute_button = gtk4::ToggleButton::with_label(if route.muted { "Unmute" } else { "Mute" });
    mute_button.set_active(route.muted);
    mute_button.connect_toggled(clone!(
        #[strong] action_tx,
        move |btn| {
            let muted = btn.is_active();
            btn.set_label(if muted { "Unmute" } else { "Mute" });
            action_tx.send(UserAction::SetRouteMute { input_id, output_id, muted }).ok();
        }
    ));
    row.append(&mute_button);

    strips_box.append(&row);

    widget_map.borrow_mut().insert(inp.id, RouteWidgets {
        scale,
        volume_label,
        mute_button,
        name_label,
    });
}

pub(crate) fn update_route_strip(route: &RouteInfo, widget_map: &RouteWidgetMap) {
    let map = widget_map.borrow();
    if let Some(widgets) = map.get(&route.input_id) {
        widgets.scale.set_value(route.volume as f64);
        widgets.volume_label.set_label(&format!("{:3}%", route.volume));
        widgets.mute_button.set_active(route.muted);
        widgets.mute_button.set_label(if route.muted { "Unmute" } else { "Mute" });
    }
}
