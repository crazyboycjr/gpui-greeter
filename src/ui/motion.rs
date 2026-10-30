//! The mockups' CSS animations, as GPUI animations over gpui-kit's
//! keyframes and easings.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::base::motion::{Easing, Keyframe, Keyframes};
use gpui_kit::{
    Animation, AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Window,
};

use crate::themes;

fn keyframes(frames: &[(f32, f32)], easing: Easing) -> Keyframes<f32> {
    Keyframes::try_new(
        frames
            .iter()
            .map(|&(offset, value)| Keyframe::new(offset, value).ease(easing.clone())),
    )
    .expect("keyframes run from 0 to 1")
}

fn css_ease() -> Easing {
    Easing::Ease
}

/// A wrong password: the field shakes sideways, in points, as every look
/// and theme folder shakes it.
pub struct Shake;

impl Shake {
    pub fn animation() -> Animation {
        Animation::new(themes::SHAKE_DURATION)
    }

    pub fn frames() -> Keyframes<f32> {
        let themes::Easing::Bezier([x1, y1, x2, y2]) = themes::SHAKE_EASING else {
            unreachable!("the shake eases by a curve")
        };
        let easing = Easing::cubic_bezier(x1, y1, x2, y2).unwrap_or_default();
        keyframes(&themes::SHAKE, easing)
    }
}

/// Content that appears fades in where it stands, quickly and then
/// settling, as on macOS's lock screen: GPUI lays out and paints on whole
/// pixels, so content that slid into place would step a pixel at a time at
/// the slow end of its easing.
pub fn appear(duration_ms: u64) -> Animation {
    Animation::new(Duration::from_millis(duration_ms)).with_easing(|t| Easing::EaseOut.sample(t))
}

/// The three dots after "Checking": each fades in and out, a fifth of a
/// second after the one before.
pub struct Pulse;

impl Pulse {
    pub const PERIOD: Duration = Duration::from_millis(1200);

    pub fn animation() -> Animation {
        Animation::new(Self::PERIOD).repeat().with_max_fps(30.0)
    }

    pub fn opacity(t: f32, delay: Duration) -> f32 {
        let phase = (t - delay.as_secs_f32() / Self::PERIOD.as_secs_f32()).rem_euclid(1.0);
        keyframes(
            &[(0.0, 0.2), (0.4, 1.0), (0.8, 0.2), (1.0, 0.2)],
            css_ease(),
        )
        .sample(phase)
    }
}

/// The skyline's antenna tips fade up and down together.
pub struct Beacon;

impl Beacon {
    pub fn animation() -> Animation {
        Animation::new(Duration::from_millis(2400))
            .repeat()
            .with_max_fps(30.0)
    }

    pub fn opacity(t: f32) -> f32 {
        keyframes(&[(0.0, 0.25), (0.5, 1.0), (1.0, 0.25)], Easing::EaseInOut).sample(t)
    }
}

/// Motion that runs evenly round a loop of `period`, such as the circuit's
/// pulses and the horizon's floor, smooth enough for fast movement.
pub fn run(period: Duration) -> Animation {
    Animation::new(period).repeat().with_max_fps(60.0)
}

/// A caret or a cursor block that blinks: drawn for the first half of each
/// `period` since it first showed under `id`, gone for the second. The
/// window draws again only when it turns on or off, twice a period, not
/// for every frame of a running animation.
pub fn blinking(id: impl Into<ElementId>, period: Duration, child: impl IntoElement) -> Blink {
    Blink {
        id: id.into(),
        period,
        child: child.into_any_element(),
    }
}

pub struct Blink {
    id: ElementId,
    period: Duration,
    child: AnyElement,
}

struct BlinkState {
    start: Instant,
    /// The turn a redraw is waiting for.
    awaited: Rc<Cell<Option<Instant>>>,
}

impl IntoElement for Blink {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Blink {
    type RequestLayoutState = ();
    type PrepaintState = bool;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
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
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.child.prepaint(window, cx);
        let half = self.period / 2;
        let id = id.expect("a blink has an id");
        window.with_element_state(id, |state: Option<BlinkState>, window| {
            let state = state.unwrap_or_else(|| BlinkState {
                start: Instant::now(),
                awaited: Rc::default(),
            });
            let halves = state.start.elapsed().as_nanos() / half.as_nanos().max(1);
            let turn = state.start + half * (halves as u32 + 1);
            if state.awaited.get() != Some(turn) {
                state.awaited.set(Some(turn));
                let awaited = state.awaited.clone();
                let view = window.current_view();
                window
                    .spawn(cx, async move |cx| {
                        let wait = turn.saturating_duration_since(Instant::now());
                        cx.background_executor()
                            .timer(wait + Duration::from_millis(1))
                            .await;
                        awaited.set(None);
                        cx.update(|_, cx| cx.notify(view)).ok();
                    })
                    .detach();
            }
            (halves.is_multiple_of(2), state)
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        on: &mut bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if *on {
            self.child.paint(window, cx);
        }
    }
}

/// Frost's wallpaper blurs and dims, and clears again, quickly and then
/// settling, as on macOS's lock screen.
pub fn veil() -> Animation {
    Animation::new(Duration::from_millis(300)).with_easing(|t| Easing::EaseOut.sample(t))
}

/// A new wallpaper fades in over the one before.
pub fn fade() -> Animation {
    Animation::new(Duration::from_millis(600)).with_easing(|t| css_ease().sample(t))
}

/// Neon's progress bar under AUTH.
pub fn fill() -> Animation {
    Animation::new(Duration::from_millis(900)).with_easing(|t| Easing::EaseOut.sample(t))
}

/// Neon's scan beam sweeps down the screen every seven seconds.
pub fn beam() -> Animation {
    Animation::new(Duration::from_secs(7))
        .repeat()
        .with_max_fps(30.0)
}

/// Neon's hostname glitches on a refusal: two half-second bursts of
/// jumps, each pose a sideways offset, a vertical one and a colour split,
/// in points.
pub struct Glitch;

impl Glitch {
    pub fn animation() -> Animation {
        Animation::new(Duration::from_millis(1000)).with_max_fps(30.0)
    }

    pub fn pose(t: f32) -> (f32, f32, f32) {
        const POSES: [(f32, f32, f32); 4] = [
            (0.0, 0.0, 3.0),
            (-3.0, 1.0, -4.0),
            (2.0, -1.0, 2.0),
            (-1.0, 2.0, -3.0),
        ];
        if t >= 1.0 {
            return (0.0, 0.0, 0.0);
        }
        // steps(2) over four keyframes, twice: a pose every eighth of the run.
        POSES[((t * 8.0) as usize) % POSES.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_shakes_by_the_shake_every_look_shares() {
        let frames = Shake::frames();
        for &(at, points) in &themes::SHAKE {
            assert!((frames.sample(at) - points).abs() < 1e-3, "at {at}");
        }
        assert_eq!(Shake::animation().duration, themes::SHAKE_DURATION);
        let widest = themes::SHAKE.iter().map(|&(_, points)| points.abs());
        assert!(widest.fold(0.0, f32::max) <= 6.0);
    }
}
