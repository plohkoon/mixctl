use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::glib::clone;
use gtk4::prelude::*;
use mixctl_core::OutputInfo;

use crate::UserAction;

pub(crate) struct OutputWidgets {
    pub scale: gtk4::Scale,
    pub volume_label: gtk4::Label,
    pub mute_button: gtk4::ToggleButton,
    pub name_label: gtk4::Label,
}

pub(crate) type OutputWidgetMap = Rc<RefCell<HashMap<u32, OutputWidgets>>>;

pub(crate) fn build_output_css(outputs: &[OutputInfo]) -> String {
    outputs
        .iter()
        .map(|out| {
            format!(
                ".output-color-{} {{ background-color: {}; min-width: 12px; min-height: 12px; border-radius: 6px; }}",
                out.id, out.color
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn rebuild_output_strips(
    strips_box: &gtk4::Box,
    outputs: &[OutputInfo],
    widget_map: &OutputWidgetMap,
    css_provider: &gtk4::CssProvider,
    action_tx: &Rc<tokio::sync::mpsc::UnboundedSender<UserAction>>,
    selected_output_id: &Rc<Cell<u32>>,
) {
    while let Some(child) = strips_box.first_child() {
        strips_box.remove(&child);
    }
    widget_map.borrow_mut().clear();

    let css = build_output_css(outputs);
    css_provider.load_from_data(&css);

    for out in outputs {
        build_output_strip(strips_box, out, widget_map, action_tx, selected_output_id);
    }
}

fn build_output_strip(
    strips_box: &gtk4::Box,
    out: &OutputInfo,
    widget_map: &OutputWidgetMap,
    action_tx: &Rc<tokio::sync::mpsc::UnboundedSender<UserAction>>,
    selected_output_id: &Rc<Cell<u32>>,
) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.set_margin_top(2);
    row.set_margin_bottom(2);

    // Color dot
    let color_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    color_dot.add_css_class(&format!("output-color-{}", out.id));
    color_dot.set_valign(gtk4::Align::Center);
    row.append(&color_dot);

    // Output name
    let name_label = gtk4::Label::new(Some(&out.name));
    name_label.set_width_request(100);
    name_label.set_xalign(0.0);
    row.append(&name_label);

    // Horizontal slider
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(out.volume as f64);
    scale.set_hexpand(true);

    let id = out.id;
    scale.connect_value_changed(clone!(
        #[strong] action_tx,
        move |scale| {
            let vol = scale.value() as u8;
            action_tx.send(UserAction::SetOutputVolume { id, volume: vol }).ok();
        }
    ));
    row.append(&scale);

    // Volume label
    let volume_label = gtk4::Label::new(Some(&format!("{:3}%", out.volume)));
    volume_label.set_width_request(40);
    row.append(&volume_label);

    // Mute toggle
    let mute_button = gtk4::ToggleButton::with_label(if out.muted { "Unmute" } else { "Mute" });
    mute_button.set_active(out.muted);
    mute_button.connect_toggled(clone!(
        #[strong] action_tx,
        move |btn| {
            let muted = btn.is_active();
            btn.set_label(if muted { "Unmute" } else { "Mute" });
            action_tx.send(UserAction::SetOutputMute { id, muted }).ok();
        }
    ));
    row.append(&mute_button);

    // Select button (→) to switch input mix view to this output
    let select_btn = gtk4::Button::with_label("→");
    select_btn.set_tooltip_text(Some("Show input mix for this output"));
    let is_selected = out.id == selected_output_id.get();
    if is_selected {
        select_btn.add_css_class("suggested-action");
    }
    select_btn.connect_clicked(clone!(
        #[strong] action_tx,
        move |_| {
            action_tx.send(UserAction::SelectOutput { output_id: id }).ok();
        }
    ));
    row.append(&select_btn);

    strips_box.append(&row);

    widget_map.borrow_mut().insert(out.id, OutputWidgets {
        scale,
        volume_label,
        mute_button,
        name_label,
    });
}

pub(crate) fn update_output_strip(out: &OutputInfo, widget_map: &OutputWidgetMap) {
    let map = widget_map.borrow();
    if let Some(widgets) = map.get(&out.id) {
        widgets.scale.set_value(out.volume as f64);
        widgets.volume_label.set_label(&format!("{:3}%", out.volume));
        widgets.mute_button.set_active(out.muted);
        widgets.mute_button.set_label(if out.muted { "Unmute" } else { "Mute" });
        widgets.name_label.set_label(&out.name);
    }
}
