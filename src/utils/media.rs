//! Collection of methods for media files.

use gettextrs::gettext;
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
        .map(|extension| format!("{}.{}", name, extension))
        .unwrap_or(name)
}
