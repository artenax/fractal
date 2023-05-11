//! Collection of methods for media files.

use std::{cell::Cell, str::FromStr, sync::Mutex};

use gettextrs::gettext;
use gtk::{gio, glib, prelude::*};
use log::{debug, error};
use matrix_sdk::attachment::{BaseAudioInfo, BaseImageInfo, BaseVideoInfo};
use mime::Mime;

use crate::toast;

/// Get a default filename for a mime type.
///
/// Tries to guess the file extension, but it might not find it.
///
/// If the mime type is unknown, it uses the name for `fallback`. The fallback
/// mime types that are recognized are `mime::IMAGE`, `mime::VIDEO` and
/// `mime::AUDIO`, other values will behave the same as `None`.
pub fn filename_for_mime(mime_type: Option<&str>, fallback: Option<mime::Name>) -> String {
    let (type_, extension) =
        if let Some(mime) = mime_type.and_then(|m| m.parse::<mime::Mime>().ok()) {
            let extension =
                mime_guess::get_mime_extensions(&mime).map(|extensions| extensions[0].to_owned());

            (Some(mime.type_().as_str().to_owned()), extension)
        } else {
            (fallback.map(|type_| type_.as_str().to_owned()), None)
        };

    let name = match type_.as_deref() {
        // Translators: Default name for image files.
        Some("image") => gettext("image"),
        // Translators: Default name for video files.
        Some("video") => gettext("video"),
        // Translators: Default name for audio files.
        Some("audio") => gettext("audio"),
        // Translators: Default name for files.
        _ => gettext("file"),
    };

    extension
        .map(|extension| format!("{name}.{extension}"))
        .unwrap_or(name)
}

/// Information about a file
pub struct FileInfo {
    /// The mime type of the file.
    pub mime: Mime,
    /// The name of the file.
    pub filename: String,
    /// The size of the file in bytes.
    pub size: Option<u32>,
}

/// Load a file and return its content and some information
pub async fn load_file(file: &gio::File) -> Result<(Vec<u8>, FileInfo), glib::Error> {
    let attributes: &[&str] = &[
        gio::FILE_ATTRIBUTE_STANDARD_CONTENT_TYPE,
        gio::FILE_ATTRIBUTE_STANDARD_DISPLAY_NAME,
        gio::FILE_ATTRIBUTE_STANDARD_SIZE,
    ];

    // Read mime type.
    let info = file
        .query_info_future(
            &attributes.join(","),
            gio::FileQueryInfoFlags::NONE,
            glib::PRIORITY_DEFAULT,
        )
        .await?;

    let mime = info
        .content_type()
        .and_then(|content_type| Mime::from_str(&content_type).ok())
        .unwrap_or(mime::APPLICATION_OCTET_STREAM);

    let filename = info.display_name().to_string();

    let raw_size = info.size();
    let size = if raw_size >= 0 {
        Some(raw_size as u32)
    } else {
        None
    };

    let (data, _) = file.load_contents_future().await?;

    Ok((
        data,
        FileInfo {
            mime,
            filename,
            size,
        },
    ))
}

pub async fn get_image_info(file: &gio::File) -> BaseImageInfo {
    let mut info = BaseImageInfo {
        width: None,
        height: None,
        size: None,
        blurhash: None,
    };

    let path = match file.path() {
        Some(path) => path,
        None => return info,
    };

    if let Some((w, h)) = image::io::Reader::open(path)
        .ok()
        .and_then(|reader| reader.into_dimensions().ok())
    {
        info.width = Some(w.into());
        info.height = Some(h.into());
    }

    info
}

async fn get_gstreamer_media_info(file: &gio::File) -> Option<gst_pbutils::DiscovererInfo> {
    let timeout = gst::ClockTime::from_seconds(15);
    let discoverer = gst_pbutils::Discoverer::new(timeout).ok()?;

    let (sender, receiver) = futures::channel::oneshot::channel();
    let sender = Mutex::new(Cell::new(Some(sender)));
    discoverer.connect_discovered(move |_, info, _| {
        if let Some(sender) = sender.lock().unwrap().take() {
            sender.send(info.clone()).unwrap();
        }
    });

    discoverer.start();
    discoverer.discover_uri_async(&file.uri()).ok()?;

    let media_info = receiver.await.unwrap();
    discoverer.stop();

    Some(media_info)
}

pub async fn get_video_info(file: &gio::File) -> BaseVideoInfo {
    let mut info = BaseVideoInfo {
        duration: None,
        width: None,
        height: None,
        size: None,
        blurhash: None,
    };

    let media_info = match get_gstreamer_media_info(file).await {
        Some(media_info) => media_info,
        None => return info,
    };

    info.duration = media_info.duration().map(Into::into);

    if let Some(stream_info) = media_info
        .video_streams()
        .get(0)
        .and_then(|s| s.downcast_ref::<gst_pbutils::DiscovererVideoInfo>())
    {
        info.width = Some(stream_info.width().into());
        info.height = Some(stream_info.height().into());
    }

    info
}

pub async fn get_audio_info(file: &gio::File) -> BaseAudioInfo {
    let mut info = BaseAudioInfo {
        duration: None,
        size: None,
    };

    let media_info = match get_gstreamer_media_info(file).await {
        Some(media_info) => media_info,
        None => return info,
    };

    info.duration = media_info.duration().map(Into::into);
    info
}

/// Save the given data to a file with the given filename.
pub async fn save_to_file(obj: &impl IsA<gtk::Widget>, data: Vec<u8>, filename: String) {
    let dialog = gtk::FileDialog::builder()
        .title(gettext("Save File"))
        .modal(true)
        .accept_label(gettext("Save"))
        .initial_name(filename)
        .build();

    match dialog
        .save_future(
            obj.root()
                .as_ref()
                .and_then(|r| r.downcast_ref::<gtk::Window>()),
        )
        .await
    {
        Ok(file) => {
            if let Err(error) = file.replace_contents(
                &data,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
                gio::Cancellable::NONE,
            ) {
                error!("Could not save file: {error}");
                toast!(obj, gettext("Could not save file"));
            }
        }
        Err(error) => {
            if error.matches(gtk::DialogError::Dismissed) {
                debug!("File dialog dismissed by user");
            } else {
                error!("Could not access file: {error}");
                toast!(obj, gettext("Could not access file"));
            }
        }
    };
}
