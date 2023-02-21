use crate::globals::CACHE_PATH;
use anyhow::anyhow;
use anyhow::Error as FailError;
use gdk::prelude::*;
use gdk_pixbuf::Pixbuf;
use gio::{Settings, SettingsExt, SettingsSchemaSource};
use html2pango::{html_escape, markup_links};
use log::error;
use std::fs::create_dir_all;
use std::io::Error as IoError;
use std::path::PathBuf;
use std::sync::mpsc::SendError;

pub fn cache_dir_path(dir: Option<&str>, name: &str) -> Result<PathBuf, IoError> {
    let path = CACHE_PATH.join(dir.unwrap_or_default());

    if !path.is_dir() {
        create_dir_all(&path)?;
    }

    Ok(path.join(name))
}

pub fn get_pixbuf_data(pb: &Pixbuf) -> Result<Vec<u8>, FailError> {
    let image = cairo::ImageSurface::create(cairo::Format::ARgb32, pb.get_width(), pb.get_height())
        .or_else(|_| Err(anyhow!("Cairo Error")))?;

    let g = cairo::Context::new(&image);
    g.set_source_pixbuf(pb, 0.0, 0.0);
    g.paint();

    let mut buf: Vec<u8> = Vec::new();
    image.write_to_png(&mut buf)?;
    Ok(buf)
}

pub fn markup_text(s: &str) -> String {
    markup_links(&html_escape(s))
}

pub fn get_markdown_schema() -> bool {
    SettingsSchemaSource::get_default()
        .and_then(|s| s.lookup("org.gnome.Fractal", true))
        .and_then(|_| {
            let settings: Settings = Settings::new("org.gnome.Fractal");
            Some(settings.get_boolean("markdown-active"))
        })
        .unwrap_or_default()
}

pub fn set_markdown_schema(md: bool) {
    if SettingsSchemaSource::get_default()
        .and_then(|s| s.lookup("org.gnome.Fractal", true))
        .is_some()
    {
        let settings: Settings = Settings::new("org.gnome.Fractal");
        if let Err(err) = settings.set_boolean("markdown-active", md) {
            error!("Can't save markdown active state: {:?}", err);
        }
    }
}

macro_rules! unwrap_or_unit_return {
    ($x:expr) => {
        match $x {
            Some(a) => a,
            None => return,
        }
    };
}

pub trait ResultExpectLog {
    fn expect_log(&self, log: &str);
}

impl<T> ResultExpectLog for Result<(), SendError<T>> {
    fn expect_log(&self, log: &str) {
        if self.is_err() {
            error!("{}", log);
        }
    }
}
