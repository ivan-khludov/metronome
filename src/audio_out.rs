//! GStreamer output: `appsrc ! audioconvert ! audioresample ! <sink>`.
//!
//! The pipeline is fed with continuous audio rendered by `ClickEngine`. The
//! sink runs with `sync=false`: it writes data as fast as its own buffer
//! drains and blocks the source when the buffer is full. The sink's audio
//! device therefore paces everything, and clicks sit at exact sample
//! positions in the stream. Nothing is ever dropped as "late".

use crate::engine::{ClickEngine, Clicks, Control};
use gst::prelude::*;
use gstreamer_app as gst_app;
use std::error::Error;
use std::sync::{Arc, Mutex, OnceLock};

const RATE: u32 = 48_000;
/// 10 ms per buffer.
const CHUNK: usize = 480;
/// Sink buffer in microseconds. This is also how long a tempo or accent
/// change takes to become audible. Raise it if you hear underruns.
const SINK_BUFFER_US: u64 = 50_000;
const SINK_LATENCY_US: u64 = 10_000;

static CLICK_HIGH_URI: &str = "resource:///io/github/ivan_khludov/metronome/audio/clicker-high.ogg";
static CLICK_LOW_URI: &str = "resource:///io/github/ivan_khludov/metronome/audio/clicker-low.ogg";

static CLICKS: OnceLock<Arc<Clicks>> = OnceLock::new();

type BoxError = Box<dyn Error + Send + Sync>;

/// Decodes the bundled .ogg files once (mono, 48 kHz, f32). Falls back to
/// synthesized clicks if decoding fails.
fn clicks() -> Arc<Clicks> {
    CLICKS
        .get_or_init(|| match (decode(CLICK_HIGH_URI), decode(CLICK_LOW_URI)) {
            (Ok(high), Ok(low)) if !high.is_empty() && !low.is_empty() => {
                log::info!(
                    "decoded click files: high {} samples, low {} samples",
                    high.len(),
                    low.len()
                );
                Arc::new(Clicks { high, low })
            }
            (high, low) => {
                log::warn!(
                    "could not decode click files ({:?} / {:?}), using synthesized clicks",
                    high.as_ref().err(),
                    low.as_ref().err()
                );
                Arc::new(Clicks::synthesized(RATE))
            }
        })
        .clone()
}

fn decode(uri: &str) -> Result<Vec<f32>, BoxError> {
    let desc = format!(
        "uridecodebin uri=\"{uri}\" ! audioconvert ! audioresample ! \
         audio/x-raw,format=F32LE,layout=interleaved,rate={RATE},channels=1 ! \
         appsink name=sink sync=false"
    );
    let pipeline = gst::parse::launch(&desc)?
        .downcast::<gst::Pipeline>()
        .map_err(|_| box_err("decoder is not a pipeline"))?;
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| box_err("decoder has no appsink"))?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| box_err("decoder sink is not an appsink"))?;
    pipeline.set_state(gst::State::Playing)?;

    let mut out = Vec::new();
    let timeout = gst::ClockTime::from_seconds(2);
    while let Some(sample) = sink.try_pull_sample(timeout) {
        let Some(buffer) = sample.buffer() else {
            continue;
        };
        let map = buffer.map_readable()?;
        out.extend(
            map.as_slice()
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
    }
    let _ = pipeline.set_state(gst::State::Null);
    if out.is_empty() {
        return Err(box_err("decoder produced no samples"));
    }
    Ok(out)
}

fn box_err(message: &'static str) -> BoxError {
    message.into()
}

struct Stream {
    engine: ClickEngine,
    frames_sent: u64,
}

/// Dropping this stops the sound.
#[derive(Debug)]
pub struct AudioOut {
    pipeline: gst::Pipeline,
}

impl AudioOut {
    pub fn start(ctl: Arc<Control>) -> Result<Self, BoxError> {
        gst::init()?;
        let stream = Mutex::new(Stream {
            engine: ClickEngine::new(ctl, clicks(), RATE),
            frames_sent: 0,
        });

        let caps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("layout", "interleaved")
            .field("rate", RATE as i32)
            .field("channels", 1i32)
            .build();

        let appsrc = gst::ElementFactory::make("appsrc")
            .build()?
            .downcast::<gst_app::AppSrc>()
            .map_err(|_| box_err("not an appsrc"))?;
        appsrc.set_caps(Some(&caps));
        appsrc.set_format(gst::Format::Time);
        appsrc.set_is_live(false); // the sink paces us, not the pipeline clock
        appsrc.set_do_timestamp(false);
        appsrc.set_max_bytes((CHUNK * 4 * 4) as u64);

        appsrc.set_callbacks(
            gst_app::AppSrcCallbacks::builder()
                .need_data(move |src, _length| {
                    let mut stream = stream.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    let mut mono = [0f32; CHUNK];
                    stream.engine.render(&mut mono);

                    let mut bytes = [0u8; CHUNK * 4];
                    for (index, sample) in mono.iter().enumerate() {
                        bytes[index * 4..index * 4 + 4].copy_from_slice(&sample.to_le_bytes());
                    }
                    let mut buffer = gst::Buffer::from_mut_slice(bytes);
                    {
                        let buffer = buffer.get_mut().unwrap();
                        let frames = stream.frames_sent;
                        buffer.set_pts(gst::ClockTime::from_nseconds(
                            frames * 1_000_000_000 / u64::from(RATE),
                        ));
                        buffer.set_duration(gst::ClockTime::from_nseconds(
                            CHUNK as u64 * 1_000_000_000 / u64::from(RATE),
                        ));
                    }
                    stream.frames_sent += CHUNK as u64;
                    let _ = src.push_buffer(buffer);
                })
                .build(),
        );

        let convert = gst::ElementFactory::make("audioconvert").build()?;
        let resample = gst::ElementFactory::make("audioresample").build()?;
        let sink = audio_sink()?;
        let src_el: &gst::Element = appsrc.upcast_ref();

        let pipeline = gst::Pipeline::new();
        pipeline.add_many([src_el, &convert, &resample, &sink])?;
        gst::Element::link_many([src_el, &convert, &resample, &sink])?;

        if let Some(bus) = pipeline.bus() {
            bus.set_sync_handler(|_bus, msg| {
                match msg.view() {
                    gst::MessageView::Error(e) => {
                        log::error!("audio pipeline error: {} ({:?})", e.error(), e.debug())
                    }
                    gst::MessageView::Warning(w) => {
                        log::warn!("audio pipeline warning: {} ({:?})", w.error(), w.debug())
                    }
                    _ => {}
                }
                gst::BusSyncReply::Pass
            });
        }

        pipeline.set_state(gst::State::Playing)?;
        Ok(Self { pipeline })
    }
}

fn audio_sink() -> Result<gst::Element, BoxError> {
    for name in ["pulsesink", "pipewiresink", "autoaudiosink"] {
        let Ok(sink) = gst::ElementFactory::make(name).build() else {
            continue;
        };
        if sink.find_property("sync").is_some() {
            sink.set_property("sync", false);
        }
        if sink.find_property("buffer-time").is_some() {
            sink.set_property("buffer-time", SINK_BUFFER_US as i64);
        }
        if sink.find_property("latency-time").is_some() {
            sink.set_property("latency-time", SINK_LATENCY_US as i64);
        }
        // Some sinks (e.g. pipewiresink without PipeWire access in a sandbox)
        // can be created but cannot open the device. Skip them.
        if let Err(err) = sink.set_state(gst::State::Ready) {
            log::warn!("audio sink {name} cannot start ({err}), trying the next one");
            let _ = sink.set_state(gst::State::Null);
            continue;
        }
        log::info!("audio sink: {name}");
        return Ok(sink);
    }
    Err(box_err("no audio sink"))
}

impl Drop for AudioOut {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
