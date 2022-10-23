use std::hash::Hasher;

use djb_hash::{x33a_u32::X33aU32, HasherU32};
use gtk::{gdk, glib, graphene, gsk, pango, prelude::*};

/// The notification icon size, according GNOME Shell's code.
const NOTIFICATION_ICON_SIZE: i32 = 48;

/// The colors for avatars, according to libadwaita.
const AVATAR_COLOR_LIST: [(&str, &str, &str); 14] = [
    ("#cfe1f5", "#83b6ec", "#337fdc"), // blue
    ("#caeaf2", "#7ad9f1", "#0f9ac8"), // cyan
    ("#cef8d8", "#8de6b1", "#29ae74"), // green
    ("#e6f9d7", "#b5e98a", "#6ab85b"), // lime
    ("#f9f4e1", "#f8e359", "#d29d09"), // yellow
    ("#ffead1", "#ffcb62", "#d68400"), // gold
    ("#ffe5c5", "#ffa95a", "#ed5b00"), // orange
    ("#f8d2ce", "#f78773", "#e62d42"), // raspberry
    ("#fac7de", "#e973ab", "#e33b6a"), // magenta
    ("#e7c2e8", "#cb78d4", "#9945b5"), // purple
    ("#d5d2f5", "#9e91e8", "#7a59ca"), // violet
    ("#f2eade", "#e3cf9c", "#b08952"), // beige
    ("#e5d6ca", "#be916d", "#785336"), // brown
    ("#d8d7d3", "#c0bfbc", "#6e6d71"), // gray
];

/// Generate a notification icon from the given paintable.
pub fn paintable_as_notification_icon(
    paintable: &gdk::Paintable,
    helper_widget: &gtk::Widget,
) -> Result<gdk::Texture, glib::Error> {
    let img_width = paintable.intrinsic_width() as f64;
    let img_height = paintable.intrinsic_height() as f64;

    let mut icon_size = (NOTIFICATION_ICON_SIZE * helper_widget.scale_factor()) as f64;
    let mut snap_width = img_width;
    let mut snap_height = img_height;
    let mut x_pos = 0.0;
    let mut y_pos = 0.0;

    if img_width > img_height {
        // Make the height fit the icon size without distorting the image, but
        // don't upscale it.
        if img_height > icon_size {
            snap_height = icon_size;
            snap_width = img_width * icon_size / img_height;
        } else {
            icon_size = img_height;
        }

        // Center the clip horizontally.
        if snap_width > icon_size {
            x_pos = ((snap_width - icon_size) / 2.0) as f32;
        }
    } else {
        // Make the width fit the icon size without distorting the image, but
        // don't upscale it.
        if img_width > icon_size {
            snap_width = icon_size;
            snap_height = img_height * icon_size / img_width;
        } else {
            icon_size = img_width;
        }

        // Center the clip vertically.
        if snap_height > icon_size {
            y_pos = ((snap_height - icon_size) / 2.0) as f32;
        }
    }

    let icon_size = icon_size as f32;
    let snapshot = gtk::Snapshot::new();

    // Clip the avatar in a circle.
    let bounds = gsk::RoundedRect::from_rect(
        graphene::Rect::new(x_pos, y_pos, icon_size, icon_size),
        icon_size / 2.0,
    );
    snapshot.push_rounded_clip(&bounds);

    paintable.snapshot(snapshot.upcast_ref(), snap_width, snap_height);

    snapshot.pop();

    // Render the avatar.
    let renderer = gsk::GLRenderer::new();
    renderer.realize(None)?;

    let node = snapshot.to_node().unwrap();
    let texture = renderer.render_texture(node, None);

    renderer.unrealize();

    Ok(texture)
}

/// Generate a notification icon from a string.
///
/// This should match the behavior of `AdwAvatar`.
pub fn string_as_notification_icon(
    string: &str,
    helper_widget: &gtk::Widget,
) -> Result<gdk::Texture, glib::Error> {
    // Get the avatar colors from the string hash.
    let mut hasher = X33aU32::new();
    hasher.write(string.as_bytes());
    let color_nb = hasher.finish_u32() as usize % AVATAR_COLOR_LIST.len();
    let colors = AVATAR_COLOR_LIST[color_nb];

    let scale_factor = helper_widget.scale_factor();
    let icon_size = (NOTIFICATION_ICON_SIZE * scale_factor) as f32;
    let snapshot = gtk::Snapshot::new();

    // Clip the avatar in a circle.
    let bounds = gsk::RoundedRect::from_rect(
        graphene::Rect::new(0.0, 0.0, icon_size, icon_size),
        icon_size / 2.0,
    );
    snapshot.push_rounded_clip(&bounds);

    // Construct linear gradient background.
    snapshot.append_linear_gradient(
        &graphene::Rect::new(0.0, 0.0, icon_size, icon_size),
        &graphene::Point::new(0.0, 0.0),
        &graphene::Point::new(0.0, icon_size),
        &[
            gsk::ColorStop::new(0.0, gdk::RGBA::parse(colors.1).unwrap()),
            gsk::ColorStop::new(1.0, gdk::RGBA::parse(colors.2).unwrap()),
        ],
    );

    snapshot.pop();

    // Add initials.
    let initials = string
        .split(char::is_whitespace)
        .filter_map(|s| s.chars().next())
        .collect::<String>();
    let layout = helper_widget.create_pango_layout(Some(&initials));

    // Set the proper weight and size.
    if let Some(mut font_description) = layout.font_description().or_else(|| {
        layout
            .context()
            .and_then(|context| context.font_description())
    }) {
        font_description.set_weight(pango::Weight::Bold);
        font_description.set_size(18 * scale_factor * pango::SCALE);
        layout.set_font_description(Some(&font_description));
    }

    // Center the layout horizontally.
    layout.set_width(icon_size as i32 * pango::SCALE);
    layout.set_alignment(pango::Alignment::Center);

    // Center the layout vertically.
    let (_, lay_height) = layout.pixel_size();
    let lay_baseline = layout.baseline() / pango::SCALE;
    // This is not really a padding but the layout reports a bigger height than
    // it seems to take and this seems like a good approximation.
    let lay_padding = lay_height - lay_baseline;
    let pos_y = (icon_size - lay_height as f32 - lay_padding as f32) / 2.0;
    snapshot.translate(&graphene::Point::new(0.0, pos_y));

    snapshot.append_layout(&layout, &gdk::RGBA::parse(colors.0).unwrap());

    // Render the avatar.
    let renderer = gsk::GLRenderer::new();
    renderer.realize(None)?;

    let node = snapshot.to_node().unwrap();
    let texture = renderer.render_texture(node, None);

    renderer.unrealize();

    Ok(texture)
}
