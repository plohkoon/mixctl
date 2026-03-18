use gtk4::glib;
use gtk4::glib::clone;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use mixctl_core::config_sections::BeacnConfig;
use mixctl_core::dbus::MixCtlProxy;

pub(crate) fn show_settings_dialog(
    parent: &adw::ApplicationWindow,
    proxy: &MixCtlProxy<'static>,
) {
    let dialog = gtk4::Window::builder()
        .modal(true)
        .title("Settings")
        .default_width(400)
        .default_height(300)
        .build();
    dialog.set_transient_for(Some(parent));

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);

    let heading = gtk4::Label::new(Some("Beacn Device"));
    heading.add_css_class("title-3");
    heading.set_xalign(0.0);
    content.append(&heading);

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");

    // Layout picker
    let layout_row = adw::ActionRow::builder()
        .title("Layout")
        .build();
    let layout_dropdown = gtk4::DropDown::from_strings(&["column", "grid2x2", "dial4"]);
    layout_dropdown.set_valign(gtk4::Align::Center);
    layout_row.add_suffix(&layout_dropdown);
    list.append(&layout_row);

    // Dial sensitivity
    let sensitivity_row = adw::ActionRow::builder()
        .title("Dial Sensitivity")
        .build();
    let sensitivity_spin = gtk4::SpinButton::with_range(1.0, 10.0, 1.0);
    sensitivity_spin.set_valign(gtk4::Align::Center);
    sensitivity_row.add_suffix(&sensitivity_spin);
    list.append(&sensitivity_row);

    // Level decay
    let decay_row = adw::ActionRow::builder()
        .title("Level Decay")
        .build();
    let decay_scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.05);
    decay_scale.set_width_request(150);
    decay_scale.set_valign(gtk4::Align::Center);
    decay_row.add_suffix(&decay_scale);
    list.append(&decay_row);

    content.append(&list);

    // Save button
    let save_btn = gtk4::Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    save_btn.set_halign(gtk4::Align::End);
    save_btn.set_margin_top(8);
    content.append(&save_btn);

    dialog.set_child(Some(&content));

    // Load current config
    glib::spawn_future_local(clone!(
        #[strong] proxy,
        #[weak] layout_dropdown,
        #[weak] sensitivity_spin,
        #[weak] decay_scale,
        async move {
            if let Ok(json) = proxy.get_config_section("beacn").await {
                if let Ok(config) = serde_json::from_str::<BeacnConfig>(&json) {
                    let idx = match config.layout.as_str() {
                        "column" => 0,
                        "grid2x2" => 1,
                        "dial4" => 2,
                        _ => 0,
                    };
                    layout_dropdown.set_selected(idx);
                    sensitivity_spin.set_value(config.dial_sensitivity as f64);
                    decay_scale.set_value(config.level_decay);
                }
            }
        }
    ));

    // Save handler
    save_btn.connect_clicked(clone!(
        #[strong] proxy,
        #[weak] layout_dropdown,
        #[weak] sensitivity_spin,
        #[weak] decay_scale,
        #[weak] dialog,
        move |_| {
            let layouts = ["column", "grid2x2", "dial4"];
            let layout = layouts[layout_dropdown.selected() as usize].to_string();
            let dial_sensitivity = sensitivity_spin.value() as u32;
            let level_decay = decay_scale.value();

            let config = BeacnConfig {
                layout,
                dial_sensitivity,
                level_decay,
            };

            let proxy = proxy.clone();
            glib::spawn_future_local(clone!(
                #[weak] dialog,
                async move {
                    if let Ok(json) = serde_json::to_string(&config) {
                        proxy.set_config_section("beacn", &json).await.ok();
                    }
                    dialog.close();
                }
            ));
        }
    ));

    dialog.present();
}
