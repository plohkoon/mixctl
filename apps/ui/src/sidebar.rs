use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::{InputInfo, OutputInfo};

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
        let btn = gtk4::Button::new();
        let btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);

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

        let output_id = out.id;
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

        sidebar_box.append(&btn);
    }
}
