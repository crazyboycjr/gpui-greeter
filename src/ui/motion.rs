//! The mockups' CSS animations, as GPUI animations over gpui-kit's
//! keyframes and easings.

use std::time::Duration;

use gpui_kit::Animation;
use gpui_kit::base::motion::{Easing, Keyframe, Keyframes};

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

/// A wrong password: the field shakes sideways, in CSS pixels.
pub struct Shake;

impl Shake {
    pub fn animation() -> Animation {
        Animation::new(Duration::from_millis(450))
    }

    pub fn frames() -> Keyframes<f32> {
        let easing = Easing::cubic_bezier(0.36, 0.07, 0.19, 0.97).unwrap_or_default();
        keyframes(
            &[
                (0.0, 0.0),
                (0.1, -2.0),
                (0.2, 4.0),
                (0.3, -8.0),
                (0.4, 8.0),
                (0.5, -8.0),
                (0.6, 8.0),
                (0.7, -8.0),
                (0.8, 4.0),
                (0.9, -2.0),
                (1.0, 0.0),
            ],
            easing,
        )
    }
}

/// Content that appears rises into place as it fades in.
pub fn rise(duration_ms: u64) -> Animation {
    Animation::new(Duration::from_millis(duration_ms)).with_easing(|t| css_ease().sample(t))
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

/// Frost's "Click anywhere to log in" bobs up and brightens, and back.
pub struct Nudge;

impl Nudge {
    pub fn animation() -> Animation {
        Animation::new(Duration::from_millis(2400))
            .repeat()
            .with_max_fps(30.0)
    }

    /// The lift in CSS pixels and the opacity at `t`.
    pub fn at(t: f32) -> (f32, f32) {
        let wave = keyframes(&[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)], Easing::EaseInOut).sample(t);
        (-6.0 * wave, 0.55 + 0.35 * wave)
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

/// A caret or a cursor block that blinks: on for the first half.
pub fn blink(period_ms: u64) -> Animation {
    Animation::new(Duration::from_millis(period_ms))
        .repeat()
        .with_max_fps(8.0)
}

pub fn blink_opacity(t: f32) -> f32 {
    if t < 0.5 { 1.0 } else { 0.0 }
}

pub fn spin() -> Animation {
    Animation::new(Duration::from_millis(900))
        .repeat()
        .with_max_fps(60.0)
}

/// Frost's wallpaper blurs and dims, and clears again.
pub fn veil() -> Animation {
    Animation::new(Duration::from_millis(700)).with_easing(|t| css_ease().sample(t))
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
/// in CSS pixels.
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
