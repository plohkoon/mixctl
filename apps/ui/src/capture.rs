use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;
use mixctl_core::dbus::MixCtlProxy;

pub(crate) fn show_capture_dialog(
    parent: &adw::ApplicationWindow,
    proxy: &MixCtlProxy<'static>,
) {
    let dialog = gtk4::Window::builder()
        .modal(true)
        .title("Capture Devices")
        .default_width(450)
        .default_height(350)
        .build();
    dialog.set_transient_for(Some(parent));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let devices_list = gtk4::ListBox::new();
    devices_list.set_selection_mode(gtk4::SelectionMode::None);
    devices_list.add_css_class("boxed-list");

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&devices_list)
        .build();
    content.append(&scrolled);

    dialog.set_child(Some(&content));

    // Load initial data
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] devices_list,
        async move {
            rebuild_devices_list(&devices_list, &proxy).await;
        }
    ));

    // Subscribe to capture_devices_changed signal
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] devices_list,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_capture_devices_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                rebuild_devices_list(&devices_list, &proxy).await;
            }
        }
    ));

    dialog.present();
}

async fn rebuild_devices_list(list_box: &gtk4::ListBox, proxy: &MixCtlProxy<'static>) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let devices = proxy.list_capture_devices().await.unwrap_or_default();

    if devices.is_empty() {
        let empty = gtk4::Label::new(Some("No capture devices found"));
        empty.add_css_class("dim-label");
        empty.set_margin_top(12);
        empty.set_margin_bottom(12);
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&empty));
        list_box.append(&row);
        return;
    }

    for dev in &devices {
        let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let info = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let name_label = gtk4::Label::new(Some(&dev.name));
        name_label.set_xalign(0.0);
        name_label.add_css_class("heading");
        let device_label = gtk4::Label::new(Some(&dev.device_name));
        device_label.set_xalign(0.0);
        device_label.add_css_class("dim-label");
        info.append(&name_label);
        info.append(&device_label);
        info.set_hexpand(true);
        row_box.append(&info);

        if dev.is_added {
            let added_label = gtk4::Label::new(Some("Added"));
            added_label.add_css_class("dim-label");
            row_box.append(&added_label);
        } else {
            let add_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            let name_entry = gtk4::Entry::builder()
                .text(&dev.name)
                .width_chars(12)
                .build();
            let color_entry = gtk4::Entry::builder()
                .text("#4A90D9")
                .width_chars(8)
                .build();
            let add_btn = gtk4::Button::with_label("Add");

            let pw_node_id = dev.pw_node_id;
            add_btn.connect_clicked(clone!(
                #[strong] proxy,
                #[weak] list_box,
                #[weak] name_entry,
                #[weak] color_entry,
                move |_| {
                    let name = name_entry.text().to_string();
                    let color = color_entry.text().to_string();
                    glib::spawn_future_local(clone!(
                        #[strong] proxy,
                        #[weak] list_box,
                        async move {
                            proxy
                                .add_capture_input(pw_node_id, &name, &color)
                                .await
                                .ok();
                            rebuild_devices_list(&list_box, &proxy).await;
                        }
                    ));
                }
            ));

            add_box.append(&name_entry);
            add_box.append(&color_entry);
            add_box.append(&add_btn);
            row_box.append(&add_box);
        }

        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&row_box));
        list_box.append(&row);
    }
}
