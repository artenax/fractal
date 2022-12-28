//! Collection of methods for media files.

use std::{cell::Cell, sync::Mutex};

use gettextrs::gettext;
use gtk::{gio, prelude::*};
use matrix_sdk::attachment::{BaseAudioInfo, BaseImageInfo, BaseVideoInfo};
use ruma::events::room::MediaSource;

/// Get the unique id of the given `MediaSource`.
///
/// It is built from the underlying `MxcUri` and can be safely used in a
/// filename.
///
/// The id is not guaranteed to be unique for malformed `MxcUri`s.
pub fn media_type_uid(media_type: Option<MediaSource>) -> String {
    if let Some(mxc) = media_type
        .map(|media_type| match media_type {
            MediaSource::Plain(uri) => uri,
            MediaSource::Encrypted(file) => file.url,
        })
        .filter(|mxc| mxc.is_valid())
    {
        format!("{}_{}", mxc.server_name().unwrap(), mxc.media_id().unwrap())
    } else {
        "media_uid".to_owned()
    }
}

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
