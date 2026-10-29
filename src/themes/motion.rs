//! Time in a theme: easings as CSS writes them, transitions from one state's
//! look to the next, and motions, which move a layer by keyframes.

use std::time::Duration;

/// How a change runs over its time: `t` from 0 to 1 is mapped to progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    /// CSS `cubic-bezier(x1, y1, x2, y2)`.
    Bezier([f32; 4]),
    /// CSS `steps(count)`, or `steps(count, start)` when `start`: the value
    /// jumps in `count` equal steps.
    Steps {
        count: u32,
        start: bool,
    },
}

impl Easing {
    pub const EASE: Easing = Easing::Bezier([0.25, 0.1, 0.25, 1.0]);

    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let named = match text {
            "linear" => Some(Easing::Linear),
            "ease" => Some(Easing::EASE),
            "ease-in" => Some(Easing::Bezier([0.42, 0.0, 1.0, 1.0])),
            "ease-out" => Some(Easing::Bezier([0.0, 0.0, 0.58, 1.0])),
            "ease-in-out" => Some(Easing::Bezier([0.42, 0.0, 0.58, 1.0])),
            "step-start" => Some(Easing::Steps {
                count: 1,
                start: true,
            }),
            "step-end" => Some(Easing::Steps {
                count: 1,
                start: false,
            }),
            _ => None,
        };
        if let Some(easing) = named {
            return Ok(easing);
        }
        let wrong = || {
            format!(
                "{text:?} is not an easing: linear, ease, ease-in, ease-out, ease-in-out, \
                 cubic-bezier(x1, y1, x2, y2) or steps(n)"
            )
        };
        let arguments = |name: &str| {
            text.strip_prefix(name)
                .and_then(|rest| rest.trim_start().strip_prefix('('))
                .and_then(|rest| rest.strip_suffix(')'))
                .map(|inside| inside.split(',').map(str::trim).collect::<Vec<_>>())
        };
        if let Some(args) = arguments("cubic-bezier") {
            let numbers: Vec<f32> = args
                .iter()
                .map(|arg| arg.parse::<f32>().ok().filter(|n| n.is_finite()))
                .collect::<Option<_>>()
                .ok_or_else(wrong)?;
            let [x1, y1, x2, y2] = numbers[..] else {
                return Err(wrong());
            };
            if !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
                return Err(format!("{text:?}: x1 and x2 must lie between 0 and 1"));
            }
            return Ok(Easing::Bezier([x1, y1, x2, y2]));
        }
        if let Some(args) = arguments("steps") {
            let count = args
                .first()
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|count| (1..=10_000).contains(count))
                .ok_or_else(wrong)?;
            let start = match args.get(1).copied() {
                None | Some("end" | "jump-end") => false,
                Some("start" | "jump-start") => true,
                Some(_) => return Err(wrong()),
            };
            if args.len() > 2 {
                return Err(wrong());
            }
            return Ok(Easing::Steps { count, start });
        }
        Err(wrong())
    }

    pub fn sample(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match *self {
            Easing::Linear => t,
            Easing::Bezier([x1, y1, x2, y2]) => bezier(x1, y1, x2, y2, t),
            Easing::Steps { count, start } => {
                let count = count as f32;
                let step = if start {
                    (t * count).ceil()
                } else {
                    (t * count).floor()
                };
                (step / count).min(1.0)
            }
        }
    }

    /// Where the next step lies after `t`, for stepped easings.
    fn next_step(&self, t: f32) -> Option<f32> {
        let Easing::Steps { count, .. } = *self else {
            return None;
        };
        let count = count as f32;
        Some(((t * count).floor() + 1.0) / count)
    }
}

/// The y of a CSS cubic Bézier curve at the x of `t`.
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let curve = |a: f32, b: f32, s: f32| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * s * a + 3.0 * inverse * s * s * b + s * s * s
    };
    let slope = |a: f32, b: f32, s: f32| {
        let inverse = 1.0 - s;
        3.0 * inverse * inverse * a + 6.0 * inverse * s * (b - a) + 3.0 * s * s * (1.0 - b)
    };
    let mut s = t;
    for _ in 0..8 {
        let error = curve(x1, x2, s) - t;
        if error.abs() < 1e-5 {
            return curve(y1, y2, s);
        }
        let d = slope(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - error / d).clamp(0.0, 1.0);
    }
    let (mut low, mut high) = (0.0f32, 1.0f32);
    s = t;
    for _ in 0..30 {
        let x = curve(x1, x2, s);
        if (x - t).abs() < 1e-5 {
            break;
        }
        if x < t {
            low = s;
        } else {
            high = s;
        }
        s = (low + high) / 2.0;
    }
    curve(y1, y2, s)
}

/// How a layer changes from one state's look to the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            duration: Duration::ZERO,
            easing: Easing::EASE,
            delay: Duration::ZERO,
        }
    }
}

impl Transition {
    /// The progress `elapsed` after the change: 0 until the delay is over,
    /// 1 once done.
    pub fn progress(&self, elapsed: Duration, extra_delay: Duration) -> f32 {
        let delay = self.delay + extra_delay;
        if elapsed < delay {
            return 0.0;
        }
        if self.duration.is_zero() {
            return 1.0;
        }
        let t = (elapsed - delay).as_secs_f32() / self.duration.as_secs_f32();
        if t >= 1.0 { 1.0 } else { self.easing.sample(t) }
    }

    pub fn length(&self, extra_delay: Duration) -> Duration {
        self.delay + extra_delay + self.duration
    }
}

/// What a motion moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Animated {
    /// Sideways, in frame pixels, added to the layer's own offset.
    X,
    Y,
    /// Degrees clockwise, added.
    Rotate,
    /// A factor the layer's scale is multiplied by.
    Scale,
    /// A factor the layer's opacity is multiplied by.
    Opacity,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// Where in the run, from 0 to 1.
    pub at: f32,
    pub value: f32,
    /// The easing to the next keyframe, over the motion's own.
    pub easing: Option<Easing>,
}

/// A refused password's shake, as macOS shakes its password field: three
/// swings sideways that die away, 6, 4 and 2 points, each easing in and
/// out. Frost and Paper shake their field by it, and a theme folder's
/// layer by `motion = "shake"`. Keyframes as (at, points).
pub const SHAKE: [(f32, f32); 5] = [
    (0.0, 0.0),
    (1.0 / 6.0, -6.0),
    (0.5, 4.0),
    (5.0 / 6.0, -2.0),
    (1.0, 0.0),
];
pub const SHAKE_DURATION: Duration = Duration::from_millis(360);
pub const SHAKE_EASING: Easing = Easing::Bezier([0.42, 0.0, 0.58, 1.0]);

/// A layer moved by keyframes, as a CSS animation moves an element.
#[derive(Debug, Clone, PartialEq)]
pub struct Motion {
    pub property: Animated,
    /// From `at` 0 to `at` 1, in order.
    pub frames: Vec<Keyframe>,
    /// One run.
    pub duration: Duration,
    /// Between keyframes, unless a keyframe names its own.
    pub easing: Easing,
    /// How many runs; forever when `None`.
    pub repeat: Option<u32>,
    /// Every other run goes backwards.
    pub alternate: bool,
    pub delay: Duration,
    /// Runs by the wall clock rather than from when it starts: a run of a
    /// minute is at the same point at the same second of every minute.
    pub clock: bool,
}

impl Motion {
    /// The shake, in the frame pixels of a theme drawn at `scale`, of which
    /// a point is 1.5 at scale 1, so that it is as large in every theme.
    pub fn shake(scale: f32) -> Self {
        Motion {
            property: Animated::X,
            frames: SHAKE
                .iter()
                .map(|&(at, points)| Keyframe {
                    at,
                    value: points * 1.5 / scale,
                    easing: None,
                })
                .collect(),
            duration: SHAKE_DURATION,
            easing: SHAKE_EASING,
            repeat: Some(1),
            alternate: false,
            delay: Duration::ZERO,
            clock: false,
        }
    }

    /// The value `elapsed` after the motion started, or `None` while it
    /// waits for its delay and once it is over.
    pub fn value_at(&self, elapsed: Duration) -> Option<f32> {
        let (iteration, phase) = self.phase(elapsed)?;
        let phase = if self.alternate && iteration % 2 == 1 {
            1.0 - phase
        } else {
            phase
        };
        Some(self.sample(phase))
    }

    /// Which run `elapsed` falls in and how far into it.
    fn phase(&self, elapsed: Duration) -> Option<(u64, f32)> {
        let elapsed = elapsed.checked_sub(self.delay)?;
        let duration = self.duration.as_nanos().max(1);
        let iteration = (elapsed.as_nanos() / duration) as u64;
        if self.repeat.is_some_and(|runs| iteration >= u64::from(runs)) {
            return None;
        }
        let phase = (elapsed.as_nanos() % duration) as f32 / duration as f32;
        Some((iteration, phase))
    }

    fn sample(&self, phase: f32) -> f32 {
        let frames = &self.frames;
        let Some(first) = frames.first() else {
            return 0.0;
        };
        if phase <= first.at {
            return first.value;
        }
        for pair in frames.windows(2) {
            let (from, to) = (&pair[0], &pair[1]);
            if phase <= to.at {
                let span = to.at - from.at;
                let local = if span > 0.0 {
                    (phase - from.at) / span
                } else {
                    1.0
                };
                let eased = from.easing.unwrap_or(self.easing).sample(local);
                return from.value + (to.value - from.value) * eased;
            }
        }
        frames.last().map_or(0.0, |last| last.value)
    }

    /// When the motion next changes what it shows, `elapsed` after it
    /// started: at once for a smooth motion, at the next step for a stepped
    /// one, or never once it is over.
    pub fn next_change(&self, elapsed: Duration) -> Option<Duration> {
        if elapsed < self.delay {
            return Some(self.delay - elapsed);
        }
        let (iteration, phase) = self.phase(elapsed)?;
        if self.alternate && iteration % 2 == 1 {
            return Some(Duration::ZERO);
        }
        let Some(pair) = self.frames.windows(2).find(|pair| phase <= pair[1].at) else {
            return Some(Duration::ZERO);
        };
        let (from, to) = (&pair[0], &pair[1]);
        let span = (to.at - from.at).max(f32::EPSILON);
        let local = ((phase - from.at) / span).clamp(0.0, 1.0);
        let next = if from.value == to.value {
            // Nothing moves until the next keyframe.
            to.at
        } else {
            let Some(step) = from.easing.unwrap_or(self.easing).next_step(local) else {
                return Some(Duration::ZERO);
            };
            from.at + step.min(1.0) * span
        };
        let wait = (next - phase).max(0.0) * self.duration.as_secs_f32();
        Some(Duration::from_secs_f32(wait.max(0.001)))
    }

    /// When a motion that ends is over, `None` for one that runs forever.
    pub fn end(&self) -> Option<Duration> {
        self.repeat
            .map(|runs| self.delay + self.duration.saturating_mul(runs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_css_easings() {
        assert_eq!(Easing::parse("linear").unwrap(), Easing::Linear);
        assert_eq!(
            Easing::parse("steps(32)").unwrap(),
            Easing::Steps {
                count: 32,
                start: false
            }
        );
        assert_eq!(
            Easing::parse("cubic-bezier(.36, .07, .19, .97)").unwrap(),
            Easing::Bezier([0.36, 0.07, 0.19, 0.97])
        );
        for wrong in [
            "bounce",
            "steps(0)",
            "cubic-bezier(1, 2)",
            "cubic-bezier(2, 0, 0, 1)",
        ] {
            assert!(Easing::parse(wrong).is_err(), "{wrong}");
        }
    }

    #[test]
    fn easings_run_from_zero_to_one() {
        for easing in [
            "linear",
            "ease",
            "ease-in-out",
            "steps(4)",
            "cubic-bezier(.3,.7,.4,1)",
        ] {
            let easing = Easing::parse(easing).unwrap();
            assert_eq!(easing.sample(0.0), 0.0, "{easing:?}");
            assert!((easing.sample(1.0) - 1.0).abs() < 1e-4, "{easing:?}");
        }
        let ease = Easing::EASE.sample(0.5);
        assert!((ease - 0.8024).abs() < 1e-3, "{ease}");
        assert_eq!(Easing::parse("steps(4)").unwrap().sample(0.49), 0.25);
    }

    fn motion(frames: &[(f32, f32)], duration_ms: u64) -> Motion {
        Motion {
            property: Animated::X,
            frames: frames
                .iter()
                .map(|&(at, value)| Keyframe {
                    at,
                    value,
                    easing: None,
                })
                .collect(),
            duration: Duration::from_millis(duration_ms),
            easing: Easing::Linear,
            repeat: Some(1),
            alternate: false,
            delay: Duration::ZERO,
            clock: false,
        }
    }

    #[test]
    fn samples_keyframes_and_stops() {
        let shake = motion(&[(0.0, 0.0), (0.5, 10.0), (1.0, 0.0)], 400);
        assert_eq!(shake.value_at(Duration::from_millis(100)), Some(5.0));
        assert_eq!(shake.value_at(Duration::from_millis(200)), Some(10.0));
        assert_eq!(shake.value_at(Duration::from_millis(400)), None);
        assert_eq!(shake.end(), Some(Duration::from_millis(400)));
    }

    #[test]
    fn alternates_and_waits() {
        let scuttle = Motion {
            repeat: None,
            alternate: true,
            delay: Duration::from_millis(100),
            ..motion(&[(0.0, -14.0), (1.0, 14.0)], 500)
        };
        assert_eq!(scuttle.value_at(Duration::from_millis(50)), None);
        assert_eq!(scuttle.value_at(Duration::from_millis(350)), Some(0.0));
        // The second run goes back: 0.2 of it is 0.8 of the way out.
        assert_eq!(scuttle.value_at(Duration::from_millis(700)), Some(8.4));
        assert_eq!(
            scuttle.next_change(Duration::from_millis(40)),
            Some(Duration::from_millis(60))
        );
        assert_eq!(scuttle.end(), None);
    }

    #[test]
    fn steps_wait_for_the_next_step() {
        let tooth = Motion {
            easing: Easing::parse("steps(32)").unwrap(),
            repeat: None,
            ..motion(&[(0.0, 0.0), (1.0, 360.0)], 60_000)
        };
        assert_eq!(tooth.value_at(Duration::from_millis(1_874)), Some(0.0));
        assert_eq!(tooth.value_at(Duration::from_millis(1_876)), Some(11.25));
        let wait = tooth.next_change(Duration::from_millis(1_000)).unwrap();
        assert!((wait.as_secs_f32() - 0.875).abs() < 0.01, "{wait:?}");
    }

    #[test]
    fn a_blink_waits_for_its_turn() {
        let blink = Motion {
            repeat: None,
            ..motion(&[(0.0, 1.0), (0.5, 1.0), (0.5, 0.0), (1.0, 0.0)], 1000)
        };
        assert_eq!(blink.value_at(Duration::from_millis(499)), Some(1.0));
        assert_eq!(blink.value_at(Duration::from_millis(501)), Some(0.0));
        let wait = blink.next_change(Duration::from_millis(200)).unwrap();
        assert!((wait.as_secs_f32() - 0.3).abs() < 0.01, "{wait:?}");
    }

    #[test]
    fn transitions_wait_then_ease() {
        let fade = Transition {
            duration: Duration::from_millis(500),
            easing: Easing::Linear,
            delay: Duration::from_millis(900),
        };
        assert_eq!(
            fade.progress(Duration::from_millis(800), Duration::ZERO),
            0.0
        );
        assert_eq!(
            fade.progress(Duration::from_millis(1150), Duration::ZERO),
            0.5
        );
        assert_eq!(
            fade.progress(Duration::from_millis(1150), Duration::from_millis(250)),
            0.0
        );
        assert_eq!(fade.progress(Duration::from_secs(9), Duration::ZERO), 1.0);
    }
}
