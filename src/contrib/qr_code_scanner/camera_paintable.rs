// SPDX-License-Identifier: GPL-3.0-or-later
//
// Fancy Camera with QR code detection
//
// Pipeline:
//                            queue -- videoconvert -- QrCodeDetector sink
//                         /
//     pipewiresrc -- tee
//                         \
//                            queue -- videoconvert -- gst paintable sink

use std::{
    cell::Cell,
    os::unix::io::AsRawFd,
    sync::{Arc, Mutex},
};

use ashpd::desktop::camera;
use gst::{bus::BusWatchGuard, prelude::*};
use gtk::{
    gdk, glib,
    glib::{clone, subclass::prelude::*},
    graphene,
    prelude::*,
    subclass::prelude::*,
};
use matrix_sdk::encryption::verification::QrVerificationData;
use tracing::error;

use crate::contrib::qr_code_scanner::{qr_code_detector::QrCodeDetector, QrVerificationDataBoxed};

pub enum Action {
    QrCodeDetected(QrVerificationData),
}

mod imp {
    use std::cell::RefCell;

    use glib::subclass;
    use once_cell::sync::Lazy;

    use super::*;

    #[derive(Debug, Default)]
    pub struct CameraPaintable {
        pub pipeline: RefCell<Option<(gst::Pipeline, BusWatchGuard)>>,
        pub sink_paintable: RefCell<Option<gdk::Paintable>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CameraPaintable {
        const NAME: &'static str = "CameraPaintable";
        type Type = super::CameraPaintable;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for CameraPaintable {
        fn dispose(&self) {
            self.obj().set_pipeline(None);
        }

        fn signals() -> &'static [subclass::Signal] {
            static SIGNALS: Lazy<Vec<subclass::Signal>> = Lazy::new(|| {
                vec![subclass::Signal::builder("code-detected")
                    .param_types([QrVerificationDataBoxed::static_type()])
                    .run_first()
                    .build()]
            });
            SIGNALS.as_ref()
        }
    }

    impl PaintableImpl for CameraPaintable {
        fn intrinsic_height(&self) -> i32 {
            if let Some(paintable) = self.sink_paintable.borrow().as_ref() {
                paintable.intrinsic_height()
            } else {
                0
            }
        }

        fn intrinsic_width(&self) -> i32 {
            if let Some(paintable) = self.sink_paintable.borrow().as_ref() {
                paintable.intrinsic_width()
            } else {
                0
            }
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let snapshot = snapshot.downcast_ref::<gtk::Snapshot>().unwrap();

            let paintable = self.sink_paintable.borrow();
            let Some(image) = paintable.as_ref() else {
                return;
            };

            // Transformation to avoid stretching the camera. We translate and scale the
            // image.
            let aspect = width / height.max(std::f64::EPSILON); // Do not divide by zero.
            let image_aspect = image.intrinsic_aspect_ratio();

            if image_aspect == 0.0 {
                image.snapshot(snapshot, width, height);
                return;
            };

            let (new_width, new_height) = match aspect <= image_aspect {
                true => (height * image_aspect, height), // Mobile view
                false => (width, width / image_aspect),  // Landscape
            };

            let p = graphene::Point::new(
                ((width - new_width) / 2.0) as f32,
                ((height - new_height) / 2.0) as f32,
            );
            snapshot.translate(&p);

            image.snapshot(snapshot, new_width, new_height);
        }
    }
}

glib::wrapper! {
    pub struct CameraPaintable(ObjectSubclass<imp::CameraPaintable>) @implements gdk::Paintable;
}

impl CameraPaintable {
    pub async fn new<F: AsRawFd>(fd: F, streams: Vec<camera::Stream>) -> Self {
        let self_: Self = glib::Object::new();

        self_.set_pipewire_fd(fd, streams).await;
        self_
    }

    async fn set_pipewire_fd<F: AsRawFd>(&self, fd: F, streams: Vec<camera::Stream>) {
        // Make sure that the previous pipeline is closed so that we can be sure that it
        // doesn't use the webcam
        self.set_pipeline(None);

        let mut src_builder =
            gst::ElementFactory::make("pipewiresrc").property("fd", fd.as_raw_fd());
        if let Some(node_id) = streams.get(0).map(|s| s.node_id()) {
            src_builder = src_builder.property("path", node_id.to_string());
        }
        let pipewire_src = src_builder.build().unwrap();

        let pipeline = gst::Pipeline::new();
        let detector = QrCodeDetector::new(self.create_sender()).upcast();

        let tee = gst::ElementFactory::make("tee").build().unwrap();
        let queue = gst::ElementFactory::make("queue").build().unwrap();
        let videoconvert1 = gst::ElementFactory::make("videoconvert").build().unwrap();
        let videoconvert2 = gst::ElementFactory::make("videoconvert").build().unwrap();
        let src_pad = queue.static_pad("src").unwrap();

        // Reduce the number of frames we use to get the qrcode from
        let start = Arc::new(Mutex::new(std::time::Instant::now()));
        src_pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            let mut start = start.lock().unwrap();
            if start.elapsed() < std::time::Duration::from_millis(500) {
                gst::PadProbeReturn::Drop
            } else {
                *start = std::time::Instant::now();
                gst::PadProbeReturn::Ok
            }
        });

        let queue2 = gst::ElementFactory::make("queue").build().unwrap();
        let sink = gst::ElementFactory::make("gtk4paintablesink")
            .build()
            .unwrap();

        pipeline
            .add_many([
                &pipewire_src,
                &tee,
                &queue,
                &videoconvert1,
                &detector,
                &queue2,
                &videoconvert2,
                &sink,
            ])
            .unwrap();

        gst::Element::link_many([&pipewire_src, &tee, &queue, &videoconvert1, &detector]).unwrap();

        tee.link_pads(None, &queue2, None).unwrap();
        gst::Element::link_many([&queue2, &videoconvert2, &sink]).unwrap();

        let bus = pipeline.bus().unwrap();
        let bus_guard = bus.add_watch_local(
            clone!(@weak self as paintable => @default-return glib::ControlFlow::Break, move |_, msg| {
                if let gst::MessageView::Error(err) = msg.view() {
                    error!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                }
                glib::ControlFlow::Continue
            }),
        )
        .expect("Failed to add bus watch");

        let paintable = sink.property::<gdk::Paintable>("paintable");

        // Workaround: we wait for the first frame so that we don't show a black frame
        let (sender, receiver) = futures_channel::oneshot::channel();
        let sender = Cell::new(Some(sender));

        paintable.connect_invalidate_contents(move |_| {
            if let Some(sender) = sender.take() {
                sender.send(()).unwrap();
            }
        });

        self.set_sink_paintable(paintable);
        pipeline.set_state(gst::State::Playing).unwrap();
        self.set_pipeline(Some((pipeline, bus_guard)));
        receiver.await.unwrap();
    }

    fn set_sink_paintable(&self, paintable: gdk::Paintable) {
        let imp = self.imp();

        paintable.connect_invalidate_contents(clone!(@weak self as obj => move |_| {
            obj.invalidate_contents();
        }));

        paintable.connect_invalidate_size(clone!(@weak self as obj => move |_| {
            obj.invalidate_size();
        }));

        imp.sink_paintable.replace(Some(paintable));

        self.invalidate_contents();
        self.invalidate_size();
    }

    fn set_pipeline(&self, pipeline: Option<(gst::Pipeline, BusWatchGuard)>) {
        let imp = self.imp();

        if let Some((pipeline, _)) = imp.pipeline.take() {
            pipeline.set_state(gst::State::Null).unwrap();
        }

        if pipeline.is_none() {
            return;
        }

        imp.pipeline.replace(pipeline);
    }

    fn create_sender(&self) -> glib::Sender<Action> {
        let (sender, receiver) = glib::MainContext::channel(glib::Priority::DEFAULT);

        receiver.attach(
            None,
            glib::clone!(@weak self as obj => @default-return glib::ControlFlow::Break, move |action| {
                match action {
                    Action::QrCodeDetected(code) => {
                        obj.emit_by_name::<()>("code-detected", &[&QrVerificationDataBoxed(code)]);
                    }
                }
                glib::ControlFlow::Continue
            }),
        );

        sender
    }
}
