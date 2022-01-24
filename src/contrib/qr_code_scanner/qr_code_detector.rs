use std::convert::AsRef;

use glib::Sender;
use gst_video::{video_frame::VideoFrameRef, VideoInfo};
use log::debug;
use matrix_sdk::encryption::verification::QrVerificationData;

use super::*;
use crate::contrib::qr_code_scanner::camera_paintable::Action;

mod imp {
    use std::sync::Mutex;

    use gst::subclass::prelude::*;
    use gst_base::subclass::prelude::*;
    use gst_video::subclass::prelude::*;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Default)]
    pub struct QrCodeDetector {
        pub info: Mutex<Option<VideoInfo>>,
        pub sender: Mutex<Option<Sender<Action>>>,
        pub code: Mutex<Option<QrVerificationData>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for QrCodeDetector {
        const NAME: &'static str = "QrCodeDetector";
        type Type = super::QrCodeDetector;
        type ParentType = gst_video::VideoSink;
    }

    impl ObjectImpl for QrCodeDetector {}
    impl GstObjectImpl for QrCodeDetector {}
    impl ElementImpl for QrCodeDetector {
        fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
            static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                gst::subclass::ElementMetadata::new(
                    "Matrix Qr Code detector Sink",
                    "Sink/Video/QrCode/Matrix",
                    "A Qr code detector for Matrix",
                    "Julian Sparber <julian@sparber.net>",
                )
            });

            Some(&*ELEMENT_METADATA)
        }

        fn pad_templates() -> &'static [gst::PadTemplate] {
            static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
                let caps = gst_video::video_make_raw_caps(&[gst_video::VideoFormat::Gray8])
                    .any_features()
                    .build();

                vec![gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &caps,
                )
                .unwrap()]
            });

            PAD_TEMPLATES.as_ref()
        }
    }
    impl BaseSinkImpl for QrCodeDetector {
        fn set_caps(
            &self,
            _element: &Self::Type,
            caps: &gst::Caps,
        ) -> Result<(), gst::LoggableError> {
            let video_info = gst_video::VideoInfo::from_caps(caps).unwrap();
            let mut info = self.info.lock().unwrap();
            info.replace(video_info);

            Ok(())
        }
    }
    impl VideoSinkImpl for QrCodeDetector {
        fn show_frame(
            &self,
            _element: &Self::Type,
            buffer: &gst::Buffer,
        ) -> Result<gst::FlowSuccess, gst::FlowError> {
            let now = std::time::Instant::now();

            if let Some(info) = &*self.info.lock().unwrap() {
                let frame = VideoFrameRef::from_buffer_ref_readable(buffer, info).unwrap();

                let mut samples = image::FlatSamples::<Vec<u8>> {
                    samples: frame.plane_data(0).unwrap().to_vec(),
                    layout: image::flat::SampleLayout {
                        channels: 1,
                        channel_stride: 1,
                        width: frame.width(),
                        width_stride: 1,
                        height: frame.height(),
                        height_stride: frame.plane_stride()[0] as usize,
                    },
                    color_hint: Some(image::ColorType::L8),
                };

                let image = samples.as_view_mut::<image::Luma<u8>>().unwrap();

                if let Ok(code) = QrVerificationData::from_luma(image) {
                    let mut previous_code = self.code.lock().unwrap();
                    if previous_code.as_ref() != Some(&code) {
                        previous_code.replace(code.clone());
                        let sender = self.sender.lock().unwrap();
                        sender
                            .as_ref()
                            .unwrap()
                            .send(Action::QrCodeDetected(code))
                            .unwrap();
                    }
                }
            }
            debug!("Spend {}ms to detect qr code", now.elapsed().as_millis());

            Ok(gst::FlowSuccess::Ok)
        }
    }
}

glib::wrapper! {
    pub struct QrCodeDetector(ObjectSubclass<imp::QrCodeDetector>) @extends gst_video::VideoSink, gst_base::BaseSink, gst::Element, gst::Object;
}
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for QrCodeDetector {}
unsafe impl Sync for QrCodeDetector {}

impl QrCodeDetector {
    pub fn new(sender: Sender<Action>) -> Self {
        let sink = glib::Object::new::<Self>(&[]).expect("Failed to create a QrCodeDetector");
        sink.imp().sender.lock().unwrap().replace(sender);
        sink
    }
}
