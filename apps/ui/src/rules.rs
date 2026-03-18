use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;
use mixctl_core::dbus::MixCtlProxy;
use mixctl_core::InputInfo;

pub(crate) fn show_rules_dialog(parent: &adw::ApplicationWindow, proxy: &MixCtlProxy<'static>) {
    let dialog = gtk4::Window::builder()
        .modal(true)
        .title("App Rules")
        .default_width(400)
        .default_height(350)
        .build();
    dialog.set_transient_for(Some(parent));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let rules_list = gtk4::ListBox::new();
    rules_list.set_selection_mode(gtk4::SelectionMode::None);
    rules_list.add_css_class("boxed-list");

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&rules_list)
        .build();
    content.append(&scrolled);

    // Add rule row
    let add_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    add_box.set_margin_top(4);
    let entry = gtk4::Entry::builder()
        .placeholder_text("App name")
        .hexpand(true)
        .build();
    let input_dropdown = gtk4::DropDown::from_strings(&[]);
    let add_btn = gtk4::Button::with_label("Add");
    add_box.append(&entry);
    add_box.append(&input_dropdown);
    add_box.append(&add_btn);
    content.append(&add_box);

    dialog.set_child(Some(&content));

    // Load initial data
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] rules_list,
        #[weak] input_dropdown,
        async move {
            let inputs = proxy.list_inputs().await.unwrap_or_default();
            let input_names: Vec<&str> = inputs.iter().map(|i| i.name.as_str()).collect();
            let string_list = gtk4::StringList::new(&input_names);
            input_dropdown.set_model(Some(&string_list));
            rebuild_rules_list(&rules_list, &proxy, &inputs).await;
        }
    ));

    // Add button handler
    add_btn.connect_clicked(clone!(
        #[strong] proxy,
        #[weak] entry,
        #[weak] input_dropdown,
        #[weak] rules_list,
        move |_| {
            let app_name = entry.text().to_string();
            if app_name.is_empty() {
                return;
            }
            let selected = input_dropdown.selected();
            glib::spawn_future_local(clone!(
                #[strong] proxy,
                #[weak] entry,
                #[weak] rules_list,
                async move {
                    let inputs = proxy.list_inputs().await.unwrap_or_default();
                    if let Some(input) = inputs.get(selected as usize) {
                        proxy.set_app_rule(&app_name, input.id).await.ok();
                        entry.set_text("");
                        rebuild_rules_list(&rules_list, &proxy, &inputs).await;
                    }
                }
            ));
        }
    ));

    // Subscribe to app_rules_changed signal
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] rules_list,
        async move {
            use futures_lite::StreamExt;
            let mut stream = proxy.receive_app_rules_changed().await.unwrap();
            while let Some(_) = stream.next().await {
                let inputs = proxy.list_inputs().await.unwrap_or_default();
                rebuild_rules_list(&rules_list, &proxy, &inputs).await;
            }
        }
    ));

    dialog.present();
}

async fn rebuild_rules_list(
    list_box: &gtk4::ListBox,
    proxy: &MixCtlProxy<'static>,
    inputs: &[InputInfo],
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let rules = proxy.list_app_rules().await.unwrap_or_default();

    if rules.is_empty() {
        let empty = gtk4::Label::new(Some("No rules configured"));
        empty.add_css_class("dim-label");
        empty.set_margin_top(12);
        empty.set_margin_bottom(12);
        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&empty));
        list_box.append(&row);
        return;
    }

    for rule in &rules {
        let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row_box.set_margin_start(8);
        row_box.set_margin_end(8);
        row_box.set_margin_top(4);
        row_box.set_margin_bottom(4);

        let app_label = gtk4::Label::new(Some(&rule.app_name));
        app_label.set_hexpand(true);
        app_label.set_xalign(0.0);
        row_box.append(&app_label);

        let input_name = inputs
            .iter()
            .find(|i| i.id == rule.input_id)
            .map(|i| i.name.as_str())
            .unwrap_or("?");
        let arrow = gtk4::Label::new(Some(&format!("▸ {}", input_name)));
        arrow.add_css_class("dim-label");
        row_box.append(&arrow);

        let app_name = rule.app_name.clone();
        let delete_btn = gtk4::Button::from_icon_name("edit-delete-symbolic");
        delete_btn.add_css_class("flat");
        delete_btn.connect_clicked(clone!(
            #[strong] proxy,
            #[weak] list_box,
            move |_| {
                let app_name = app_name.clone();
                glib::spawn_future_local(clone!(
                    #[strong] proxy,
                    #[weak] list_box,
                    async move {
                        proxy.remove_app_rule(&app_name).await.ok();
                        let inputs = proxy.list_inputs().await.unwrap_or_default();
                        rebuild_rules_list(&list_box, &proxy, &inputs).await;
                    }
                ));
            }
        ));
        row_box.append(&delete_btn);

        let row = gtk4::ListBoxRow::new();
        row.set_child(Some(&row_box));
        list_box.append(&row);
    }
}
