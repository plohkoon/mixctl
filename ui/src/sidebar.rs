use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::OutputInfo;

use crate::strips::{load_routes_for_output, WidgetMap};

pub(crate) fn rebuild_sidebar(
    sidebar_box: &gtk4::Box,
    outputs: &[OutputInfo],
    selected_output_id: &Rc<Cell<u32>>,
    strips_box: &gtk4::Box,
    widget_map: &WidgetMap,
    css_provider: &gtk4::CssProvider,
    proxy: &MixCtlProxy<'static>,
) {
    while let Some(child) = sidebar_box.first_child() {
        sidebar_box.remove(&child);
    }

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
                // Rebuild sidebar to update indicators
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
                            rebuild_sidebar(&sidebar_box, &outputs, &selected_output_id, &strips_box, &widget_map, &css_provider, &proxy_clone);
                        }
                        load_routes_for_output(&strips_box, &widget_map, &css_provider, &proxy_clone, selected_output_id.get()).await;
                    }
                ));
            }
        ));

        sidebar_box.append(&btn);
    }
}
