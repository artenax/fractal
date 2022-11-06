// Taken from https://gitlab.gnome.org/msandova/trinket/-/blob/master/src/qr_code.rs
// All credit goes to Maximiliano

use gtk::{glib, prelude::*, subclass::prelude::*};

pub(crate) mod imp {
    use std::cell::{Cell, RefCell};

    use gtk::{gdk, graphene};
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct QRCode {
        pub picture: gtk::Picture,
        pub data: RefCell<QRCodeData>,
        pub block_size: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QRCode {
        const NAME: &'static str = "TriQRCode";
        type Type = super::QRCode;
        type ParentType = gtk::Widget;

        fn new() -> Self {
            Self {
                block_size: Cell::new(6),
                ..Self::default()
            }
        }
    }

    impl ObjectImpl for QRCode {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().add_css_class("qrcode");
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecUInt::builder("block-size")
                    .minimum(1)
                    .default_value(6)
                    .explicit_notify()
                    .build()]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "block-size" => self.obj().block_size().to_value(),
                _ => unreachable!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "block-size" => self.obj().set_block_size(value.get().unwrap()),
                _ => unreachable!(),
            }
        }
    }
    impl WidgetImpl for QRCode {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let square_width = obj.width() as f32 / self.data.borrow().width as f32;
            let square_height = obj.height() as f32 / self.data.borrow().height as f32;

            self.data
                .borrow()
                .items
                .iter()
                .enumerate()
                .for_each(|(y, line)| {
                    line.iter().enumerate().for_each(|(x, is_dark)| {
                        let color = if *is_dark {
                            obj.style_context().color()
                        } else {
                            obj.style_context()
                                .lookup_color("background")
                                .unwrap_or_else(|| gdk::RGBA::new(0.0, 0.0, 0.0, 0.0))
                        };
                        let position = graphene::Rect::new(
                            (x as f32) * square_width,
                            (y as f32) * square_height,
                            square_width,
                            square_height,
                        );

                        snapshot.append_color(&color, &position);
                    });
                });
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let stride = self.obj().block_size() as i32;

            let minimum = match orientation {
                gtk::Orientation::Horizontal => self.data.borrow().width * stride,
                gtk::Orientation::Vertical => self.data.borrow().height * stride,
                _ => unreachable!(),
            };
            let natural = std::cmp::max(for_size, minimum);
            (minimum, natural, -1, -1)
        }
    }
}

glib::wrapper! {
    /// A widget that display a QR Code.
    ///
    /// The QR code of [`QRCode`] is set with the [QRCodeExt::set_bytes()]
    /// method. It is recommended for a QR Code to have a quiet zone, i.e. a margin of
    /// four times the value of [`QRCodeExt::block_size()`], in most contexts, widgets
    /// already count with such a margin.
    ///
    /// The code can be themed via css, where a recommended quiet-zone
    /// can be as a padding:
    ///
    /// ```css
    /// .qrcode {
    ///     color: black;
    ///     background: white;
    ///     padding: 24px;  /* 4 ⨉ block-size */
    /// }
    /// ```
    ///
    /// **Implements**: [QRCodeExt].
    pub struct QRCode(ObjectSubclass<imp::QRCode>)
        @extends gtk::Widget;
}

impl QRCode {
    /// Creates a new [`QRCode`].
    pub fn new() -> Self {
        glib::Object::new(&[])
    }

    /// Creates a new [`QRCode`] with a QR code generated from `bytes`.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let qrcode = Self::new();
        qrcode.set_bytes(bytes);

        qrcode
    }
}

pub trait QRCodeExt {
    /// Sets the displayed code of `self` to a QR code generated from `bytes`.
    fn set_bytes(&self, bytes: &[u8]);

    /// Gets the block size of `self`. This determines the size of the widget.
    fn block_size(&self) -> u32;

    /// Sets the block size of `self`.
    fn set_block_size(&self, block_size: u32);

    fn connect_block_size_notify<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId;

    /// Set the `QrCode` to be displayed
    fn set_qrcode(&self, qrcode: qrcode::QrCode);
}

impl<W: IsA<QRCode>> QRCodeExt for W {
    fn set_bytes(&self, bytes: &[u8]) {
        let data = QRCodeData::try_from(bytes).unwrap_or_else(|_| {
            glib::g_warning!(None, "Failed to load QRCode from bytes");
            Default::default()
        });
        self.as_ref().imp().data.replace(data);

        self.as_ref().queue_draw();
        self.as_ref().queue_resize();
    }

    fn set_qrcode(&self, qrcode: qrcode::QrCode) {
        self.as_ref().imp().data.replace(QRCodeData::from(qrcode));

        self.as_ref().queue_draw();
        self.as_ref().queue_resize();
    }

    fn block_size(&self) -> u32 {
        self.as_ref().imp().block_size.get()
    }

    fn set_block_size(&self, block_size: u32) {
        self.as_ref()
            .imp()
            .block_size
            .set(std::cmp::max(block_size, 1));
        self.notify("block-size");
        self.as_ref().queue_draw();
        self.as_ref().queue_resize();
    }

    fn connect_block_size_notify<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_notify_local(Some("block-size"), move |this, _| {
            f(this);
        })
    }
}

impl Default for QRCodeData {
    fn default() -> Self {
        Self::try_from("".as_bytes()).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct QRCodeData {
    pub width: i32,
    pub height: i32,
    pub items: Vec<Vec<bool>>,
}

impl TryFrom<&[u8]> for QRCodeData {
    type Error = qrcode::types::QrError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let code = qrcode::QrCode::new(data)?;
        let items = code
            .render::<char>()
            .quiet_zone(false)
            .module_dimensions(1, 1)
            .build()
            .split('\n')
            .into_iter()
            .map(|line| {
                line.chars()
                    .into_iter()
                    .map(|c| !c.is_whitespace())
                    .collect::<Vec<bool>>()
            })
            .collect::<Vec<Vec<bool>>>();

        let height = items.len() as i32;
        let width = items.len() as i32;
        let data = Self {
            width,
            height,
            items,
        };

        Ok(data)
    }
}

impl From<qrcode::QrCode> for QRCodeData {
    fn from(code: qrcode::QrCode) -> Self {
        let items = code
            .render::<char>()
            .quiet_zone(false)
            .module_dimensions(1, 1)
            .build()
            .split('\n')
            .into_iter()
            .map(|line| {
                line.chars()
                    .into_iter()
                    .map(|c| !c.is_whitespace())
                    .collect::<Vec<bool>>()
            })
            .collect::<Vec<Vec<bool>>>();

        let height = items.len() as i32;
        let width = items.len() as i32;
        Self {
            width,
            height,
            items,
        }
    }
}
