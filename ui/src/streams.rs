use gtk4::prelude::*;
use mixctl_core::{InputInfo, StreamInfo};

pub(crate) fn rebuild_streams_panel(
    streams_box: &gtk4::Box,
    streams: &[StreamInfo],
    inputs: &[InputInfo],
) {
    while let Some(child) = streams_box.first_child() {
        streams_box.remove(&child);
    }

    let label = gtk4::Label::new(Some("Streams"));
    label.add_css_class("heading");
    streams_box.append(&label);

    if streams.is_empty() {
        let empty = gtk4::Label::new(Some("No active streams"));
        empty.add_css_class("dim-label");
        streams_box.append(&empty);
        return;
    }

    for stream in streams {
        let card = build_stream_card(stream, inputs);
        streams_box.append(&card);
    }
}

fn build_stream_card(stream: &StreamInfo, inputs: &[InputInfo]) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    card.add_css_class("card");
    card.set_margin_start(2);
    card.set_margin_end(2);
    card.set_margin_top(4);
    card.set_margin_bottom(4);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.set_margin_top(4);
    content.set_margin_bottom(4);

    let app_label = gtk4::Label::new(Some(&stream.app_name));
    app_label.add_css_class("heading");
    app_label.set_xalign(0.0);
    content.append(&app_label);

    let input_name = inputs
        .iter()
        .find(|i| i.id == stream.input_id)
        .map(|i| i.name.as_str())
        .unwrap_or("?");
    let assign_label = gtk4::Label::new(Some(&format!("▸ {}", input_name)));
    assign_label.add_css_class("dim-label");
    assign_label.set_xalign(0.0);
    if let Some(inp) = inputs.iter().find(|i| i.id == stream.input_id) {
        assign_label.add_css_class(&format!("channel-text-{}", inp.id));
    }
    content.append(&assign_label);

    card.append(&content);

    // Drag source for DnD
    let drag_source = gtk4::DragSource::new();
    drag_source.set_actions(gtk4::gdk::DragAction::MOVE);
    let pw_node_id = stream.pw_node_id;
    drag_source.connect_prepare(move |_, _, _| {
        Some(gtk4::gdk::ContentProvider::for_value(
            &pw_node_id.to_string().to_value(),
        ))
    });
    card.add_controller(drag_source);

    card
}
