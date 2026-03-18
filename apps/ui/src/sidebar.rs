use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{InputInfo, OutputInfo, PlaybackDeviceInfo};

use crate::strips::{load_routes_for_output, WidgetMap};

pub(crate) fn rebuild_sidebar(
    sidebar_box: &gtk4::Box,
    outputs: &[OutputInfo],
    inputs: &[InputInfo],
    default_input_id: u32,
    selected_output_id: &Rc<Cell<u32>>,
    strips_box: &gtk4::Box,
    widget_map: &WidgetMap,
    css_provider: &gtk4::CssProvider,
    proxy: &MixCtlProxy<'static>,
    playback_devices: &[PlaybackDeviceInfo],
) {
    while let Some(child) = sidebar_box.first_child() {
        sidebar_box.remove(&child);
    }

    // Default input selector
    let default_label = gtk4::Label::new(Some("Default Input"));
    default_label.add_css_class("heading");
    default_label.set_margin_bottom(4);
    sidebar_box.append(&default_label);

    if !inputs.is_empty() {
        let input_names: Vec<&str> = inputs.iter().map(|i| i.name.as_str()).collect();
        let dropdown = gtk4::DropDown::from_strings(&input_names);
        let default_idx = inputs
            .iter()
            .position(|i| i.id == default_input_id)
            .unwrap_or(0);
        dropdown.set_selected(default_idx as u32);

        let inputs_vec = inputs.to_vec();
        dropdown.connect_selected_notify(clone!(
            #[strong] proxy,
            move |dd| {
                let selected = dd.selected() as usize;
                if let Some(input) = inputs_vec.get(selected) {
                    let input_id = input.id;
                    let proxy = proxy.clone();
                    glib::spawn_future_local(async move {
                        proxy.set_default_input(input_id).await.ok();
                    });
                }
            }
        ));
        sidebar_box.append(&dropdown);
    }

    let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
    sep.set_margin_top(8);
    sep.set_margin_bottom(8);
    sidebar_box.append(&sep);

    // Outputs list
    let label = gtk4::Label::new(Some("Outputs"));
    label.add_css_class("heading");
    label.set_margin_bottom(4);
    sidebar_box.append(&label);

    for out in outputs {
        let is_selected = out.id == selected_output_id.get();

        let output_row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);

        let btn = gtk4::Button::new();
        let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

        // Color picker button
        let color_dialog = gtk4::ColorDialog::new();
        let rgba = gtk4::gdk::RGBA::parse(&out.color)
            .unwrap_or_else(|_| gtk4::gdk::RGBA::new(0.5, 0.5, 0.5, 1.0));
        let color_btn = gtk4::ColorDialogButton::new(Some(color_dialog));
        color_btn.set_rgba(&rgba);
        btn_box.append(&color_btn);

        let output_id = out.id;
        color_btn.connect_rgba_notify(clone!(
            #[strong] proxy,
            move |btn: &gtk4::ColorDialogButton| {
                let rgba = btn.rgba();
                let hex = format!(
                    "#{:02X}{:02X}{:02X}",
                    (rgba.red() * 255.0) as u8,
                    (rgba.green() * 255.0) as u8,
                    (rgba.blue() * 255.0) as u8,
                );
                let proxy = proxy.clone();
                glib::spawn_future_local(async move {
                    proxy.set_output_color(output_id, &hex).await.ok();
                });
            }
        ));

        let name_label = gtk4::Label::new(Some(&out.name));
        name_label.set_hexpand(true);
        name_label.set_xalign(0.0);
        btn_box.append(&name_label);

        let indicator = gtk4::Label::new(Some(if is_selected { "●" } else { "○" }));
        btn_box.append(&indicator);

        btn.set_child(Some(&btn_box));

        if is_selected {
            btn.add_css_class("suggested-action");
        }

        btn.connect_clicked(clone!(
            #[strong] selected_output_id,
            #[weak] strips_box,
            #[strong] widget_map,
            #[strong] css_provider,
            #[strong] proxy,
            #[weak] sidebar_box,
            move |_| {
                selected_output_id.set(output_id);
                let proxy_clone = proxy.clone();
                glib::spawn_future_local(clone!(
                    #[weak] sidebar_box,
                    #[weak] strips_box,
                    #[strong] selected_output_id,
                    #[strong] widget_map,
                    #[strong] css_provider,
                    #[strong] proxy_clone,
                    async move {
                        let playback_devices = proxy_clone.list_playback_devices().await.unwrap_or_default();
                        if let Ok(outputs) = proxy_clone.list_outputs().await {
                            let inputs = proxy_clone.list_inputs().await.unwrap_or_default();
                            let default_input_id =
                                proxy_clone.get_default_input().await.unwrap_or(0);
                            rebuild_sidebar(
                                &sidebar_box,
                                &outputs,
                                &inputs,
                                default_input_id,
                                &selected_output_id,
                                &strips_box,
                                &widget_map,
                                &css_provider,
                                &proxy_clone,
                                &playback_devices,
                            );
                        }
                        load_routes_for_output(
                            &strips_box,
                            &widget_map,
                            &css_provider,
                            &proxy_clone,
                            selected_output_id.get(),
                        )
                        .await;
                    }
                ));
            }
        ));

        output_row.append(&btn);

        // Hardware target dropdown
        let mut device_names: Vec<String> = vec!["None".to_string()];
        device_names.extend(playback_devices.iter().map(|d| d.name.clone()));
        let device_strs: Vec<&str> = device_names.iter().map(|s| s.as_str()).collect();
        let target_dropdown = gtk4::DropDown::from_strings(&device_strs);

        // Pre-select current target
        let current_target = &out.target_device;
        if !current_target.is_empty() {
            if let Some(pos) = playback_devices.iter().position(|d| d.device_name == *current_target) {
                target_dropdown.set_selected((pos + 1) as u32); // +1 for "None" entry
            }
        }

        let playback_devices_clone: Vec<PlaybackDeviceInfo> = playback_devices.to_vec();
        target_dropdown.connect_selected_notify(clone!(
            #[strong] proxy,
            move |dd| {
                let selected = dd.selected() as usize;
                let device_name = if selected == 0 {
                    String::new()
                } else {
                    playback_devices_clone.get(selected - 1)
                        .map(|d| d.device_name.clone())
                        .unwrap_or_default()
                };
                let proxy = proxy.clone();
                glib::spawn_future_local(async move {
                    proxy.set_output_target(output_id, &device_name).await.ok();
                });
            }
        ));
        output_row.append(&target_dropdown);

        sidebar_box.append(&output_row);
    }
}
