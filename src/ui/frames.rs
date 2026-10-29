//! With `GPUI_GREETER_FRAMES=1`, each window tells on stderr how its frames
//! went in each burst of motion, such as a wake from rest or a switch of
//! looks: how long each took to lay out and paint, how long after the one
//! before it began, and the slowest of both, so that a stutter shows in
//! numbers on the screen it happens on.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Window,
};

/// Frames further apart than this begin a new burst.
const BURST_GAP: Duration = Duration::from_millis(250);

/// Frames slower than this, between one and the next, show as slow.
const SLOW: Duration = Duration::from_millis(25);

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("GPUI_GREETER_FRAMES")
            .is_some_and(|value| !value.is_empty() && value != "0")
    })
}

/// One window's frames since its last burst was told.
pub struct FrameLog {
    output: usize,
    frames: Vec<Frame>,
    telling: bool,
}

#[derive(Clone, Copy)]
struct Frame {
    start: Instant,
    drawing: Duration,
}

impl FrameLog {
    pub fn new(output: usize) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            output,
            frames: Vec::new(),
            telling: false,
        }))
    }

    fn record(this: &Rc<RefCell<Self>>, frame: Frame, cx: &mut App) {
        let mut log = this.borrow_mut();
        if log
            .frames
            .last()
            .is_some_and(|last| frame.start - last.start > BURST_GAP)
        {
            log.tell();
        }
        log.frames.push(frame);
        if log.telling {
            return;
        }
        log.telling = true;
        let this = this.clone();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(BURST_GAP).await;
                let mut log = this.borrow_mut();
                if log
                    .frames
                    .last()
                    .is_none_or(|last| last.start.elapsed() > BURST_GAP)
                {
                    log.tell();
                    log.telling = false;
                    break;
                }
            }
        })
        .detach();
    }

    /// Tells the burst so far, if it moved at all, and forgets it.
    fn tell(&mut self) {
        let frames = std::mem::take(&mut self.frames);
        if let Some(told) = told(self.output, &frames) {
            eprintln!("{told}");
        }
    }
}

/// A burst of frames in words: none for a lone frame, which is no motion.
fn told(output: usize, frames: &[Frame]) -> Option<String> {
    let (first, last) = (frames.first()?, frames.last()?);
    if frames.len() < 2 {
        return None;
    }
    let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
    let span = last.start - first.start;
    let gaps: Vec<(usize, Duration)> = frames
        .windows(2)
        .enumerate()
        .map(|(at, pair)| (at + 1, pair[1].start - pair[0].start))
        .collect();
    let drawing: Vec<(usize, Duration)> = frames
        .iter()
        .enumerate()
        .map(|(at, frame)| (at, frame.drawing))
        .collect();
    let median = |values: &[(usize, Duration)]| {
        let mut sorted: Vec<Duration> = values.iter().map(|&(_, value)| value).collect();
        sorted.sort();
        sorted[sorted.len() / 2]
    };
    let slowest = |values: &[(usize, Duration)]| {
        values
            .iter()
            .copied()
            .max_by_key(|&(_, value)| value)
            .unwrap_or_default()
    };
    let (gap_at, gap) = slowest(&gaps);
    let (draw_at, draw) = slowest(&drawing);
    let mut told = format!(
        "frames on monitor {output}: {} in {:.0} ms, {:.1} a second; between frames median {:.1} ms, slowest {:.1} (#{gap_at}); drawing median {:.1} ms, slowest {:.1} (#{draw_at})",
        frames.len(),
        ms(span),
        (frames.len() - 1) as f64 / span.as_secs_f64().max(1e-9),
        ms(median(&gaps)),
        ms(gap),
        ms(median(&drawing)),
        ms(draw),
    );
    let slow: Vec<String> = gaps
        .iter()
        .filter(|&&(_, gap)| gap > SLOW)
        .map(|&(at, gap)| {
            format!(
                "#{at} {:.1} ms after, drawn in {:.1}",
                ms(gap),
                ms(frames[at].drawing)
            )
        })
        .collect();
    if !slow.is_empty() {
        told.push_str(&format!("\n  slow: {}", slow.join("; ")));
    }
    Some(told)
}

/// Paints its child, then tells the log how long the frame took since the
/// window began to draw it, at `start`.
pub struct Timed {
    pub start: Instant,
    pub log: Rc<RefCell<FrameLog>>,
    pub child: AnyElement,
}

impl IntoElement for Timed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Timed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
        let frame = Frame {
            start: self.start,
            drawing: self.start.elapsed(),
        };
        FrameLog::record(&self.log, frame, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_names_its_slowest_frames() {
        let start = Instant::now();
        let at = |ms: u64, drawing: u64| Frame {
            start: start + Duration::from_millis(ms),
            drawing: Duration::from_millis(drawing),
        };
        assert_eq!(told(0, &[at(0, 3)]), None);
        let told = told(1, &[at(0, 3), at(17, 2), at(50, 30), at(67, 2)]).unwrap();
        assert!(
            told.starts_with("frames on monitor 1: 4 in 67 ms"),
            "{told}"
        );
        assert!(told.contains("slowest 33.0 (#2)"), "{told}");
        assert!(
            told.contains("drawing median 3.0 ms, slowest 30.0 (#2)"),
            "{told}"
        );
        assert!(
            told.contains("slow: #2 33.0 ms after, drawn in 30.0"),
            "{told}"
        );
    }
}
