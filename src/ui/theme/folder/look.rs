//! A layer's look at this moment: its properties in the state the login is
//! in, changing from the last state's over its transition, and moved by its
//! motions; and when the window next needs drawing.

use std::cell::Cell;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::config::Language;
use crate::themes::{Animated, Color, Condition, Layer, Length, Props, State, Transition};

/// How many state changes a window remembers, for transitions that start
/// while others are still running.
const HISTORY: usize = 6;

/// The states a window has shown, with when each began; the oldest first.
pub struct Timeline {
    changes: Vec<(State, Instant)>,
    /// When the window opened: motions outside a state run from then.
    pub started: Instant,
}

impl Timeline {
    pub fn new(now: Instant) -> Self {
        Self {
            changes: Vec::new(),
            started: now,
        }
    }

    pub fn observe(&mut self, state: State, now: Instant) {
        if self.changes.last().is_some_and(|(last, _)| *last == state) {
            return;
        }
        self.changes.push((state, now));
        if self.changes.len() > HISTORY {
            self.changes.remove(0);
        }
    }

    pub fn changes(&self) -> &[(State, Instant)] {
        &self.changes
    }

    pub fn current(&self) -> Option<(State, Instant)> {
        self.changes.last().copied()
    }
}

/// When a window next needs drawing: at once while something moves
/// smoothly, or at the next change; and whether it is still settling.
#[derive(Default)]
pub struct Pace {
    pub smooth: Cell<bool>,
    pub wake: Cell<Option<Instant>>,
    /// A transition or a motion that ends is still running.
    pub settling: Cell<bool>,
}

impl Pace {
    pub fn wake_at(&self, at: Instant) {
        if self.wake.get().is_none_or(|wake| at < wake) {
            self.wake.set(Some(at));
        }
    }
}

/// What a layer is under besides the state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    pub current: bool,
    pub focused: bool,
    pub armed: bool,
    pub fired: bool,
    pub hover: bool,
}

/// The layer's properties in `state`, its `on` tables applied in order.
pub fn props_at(
    layer: &Layer,
    state: State,
    flags: Flags,
    language: Language,
    failures: u32,
) -> Props {
    let mut props = layer.base.clone();
    apply_overrides(layer, &mut props, state, flags, language, failures);
    props
}

/// What the layer's `on` tables alone set in `state`: for a tiles layer,
/// whose own `layout` and `focus` are where it starts, not what it shows.
pub fn overridden(
    layer: &Layer,
    state: State,
    flags: Flags,
    language: Language,
    failures: u32,
) -> Props {
    let mut props = Props::default();
    apply_overrides(layer, &mut props, state, flags, language, failures);
    props
}

fn apply_overrides(
    layer: &Layer,
    props: &mut Props,
    state: State,
    flags: Flags,
    language: Language,
    failures: u32,
) {
    for change in &layer.overrides {
        let applies = match change.condition {
            Condition::Language(_) => change.condition.as_language() == Some(language),
            Condition::Failed => failures > 0 && state != State::Welcome,
            Condition::Current => flags.current,
            Condition::Focused => flags.focused,
            Condition::State(_) => change.condition.as_state() == Some(state),
            Condition::Armed => flags.armed,
            Condition::Fired => flags.fired,
            Condition::Hover => flags.hover,
        };
        if applies {
            props.apply(&change.props);
        }
    }
}

/// The values that change smoothly from one state's look to the next, with
/// the rest, which switch as the transition starts.
#[derive(Debug, Clone)]
pub struct Look {
    /// 1 when shown, 0 when hidden, between while fading.
    pub shown: f32,
    pub opacity: f32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotate: f32,
    pub clip: [f32; 4],
    pub width: Option<Length>,
    pub height: Option<Length>,
    pub color: Color,
    pub fill: Color,
    pub border_color: Color,
    pub caret_color: Color,
    pub placeholder_color: Color,
    pub blurred: f32,
    pub props: Props,
}

impl Look {
    pub fn of(props: Props) -> Self {
        let color = props.color.unwrap_or(Color::WHITE);
        Self {
            shown: if props.visible == Some(false) {
                0.0
            } else {
                1.0
            },
            opacity: props.opacity.unwrap_or(1.0),
            x: props.x.unwrap_or(0.0),
            y: props.y.unwrap_or(0.0),
            scale: props.scale.unwrap_or(1.0),
            rotate: props.rotate.unwrap_or(0.0),
            clip: props.clip.unwrap_or([0.0; 4]),
            width: props.width,
            height: props.height,
            color,
            fill: props.fill.unwrap_or(Color::TRANSPARENT),
            border_color: props.border_color.unwrap_or(color),
            caret_color: props.caret_color.unwrap_or(color),
            placeholder_color: props
                .placeholder_color
                .unwrap_or(color.with_alpha(color.a * 0.5)),
            blurred: props.blurred.unwrap_or(0.0),
            props,
        }
    }

    /// `t` of the way from `from` to `to`; what cannot change smoothly is
    /// `to`'s as soon as the change starts.
    pub fn mix(from: &Look, to: &Look, t: f32) -> Look {
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        let color = |a: Color, b: Color| {
            // A colour fading in or out keeps its hue.
            let a = if a.a == 0.0 { b.with_alpha(0.0) } else { a };
            let b = if b.a == 0.0 { a.with_alpha(0.0) } else { b };
            a.mix(b, t)
        };
        let length = |a: Option<Length>, b: Option<Length>| match (a, b) {
            (Some(a), Some(b)) => Some(a.mix(b, t)),
            _ => b,
        };
        let mut clip = [0.0; 4];
        for (side, (a, b)) in clip.iter_mut().zip(from.clip.iter().zip(to.clip.iter())) {
            *side = lerp(*a, *b);
        }
        Look {
            shown: lerp(from.shown, to.shown).clamp(0.0, 1.0),
            opacity: lerp(from.opacity, to.opacity).clamp(0.0, 1.0),
            x: lerp(from.x, to.x),
            y: lerp(from.y, to.y),
            scale: lerp(from.scale, to.scale).max(0.0),
            rotate: lerp(from.rotate, to.rotate),
            clip,
            width: length(from.width, to.width),
            height: length(from.height, to.height),
            color: color(from.color, to.color),
            fill: color(from.fill, to.fill),
            border_color: color(from.border_color, to.border_color),
            caret_color: color(from.caret_color, to.caret_color),
            placeholder_color: color(from.placeholder_color, to.placeholder_color),
            blurred: lerp(from.blurred, to.blurred).clamp(0.0, 1.0),
            props: if t > 0.0 {
                to.props.clone()
            } else {
                from.props.clone()
            },
        }
    }
}

/// The transition a layer makes on entering `state`.
fn transition_into(layer: &Layer, state: State) -> Transition {
    layer
        .overrides
        .iter()
        .rev()
        .find(|change| change.condition.as_state() == Some(state) && change.transition.is_some())
        .and_then(|change| change.transition)
        .unwrap_or(layer.transition)
}

/// The layer's look now, from `statics`, its properties in each state of
/// `timeline` in turn; `delay` holds it back further, for staggered layers.
pub fn look(
    layer: &Layer,
    statics: &[Props],
    timeline: &Timeline,
    now: Instant,
    delay: Duration,
    pace: &Pace,
) -> Look {
    let last = statics.len().min(timeline.changes().len());
    if last == 0 {
        return Look::of(statics.first().cloned().unwrap_or_default());
    }
    look_at(layer, statics, timeline, last - 1, now, now, delay, pace)
}

#[allow(clippy::too_many_arguments)]
fn look_at(
    layer: &Layer,
    statics: &[Props],
    timeline: &Timeline,
    index: usize,
    at: Instant,
    now: Instant,
    delay: Duration,
    pace: &Pace,
) -> Look {
    let target = Look::of(statics[index].clone());
    if index == 0 {
        return target;
    }
    let (state, since) = timeline.changes()[index];
    let transition = transition_into(layer, state);
    let elapsed = at.saturating_duration_since(since);
    if elapsed >= transition.length(delay) {
        return target;
    }
    let t = transition.progress(elapsed, delay);
    if at == now {
        pace.settling.set(true);
        let starts = since + transition.delay + delay;
        if now < starts {
            pace.wake_at(starts);
        } else {
            pace.smooth.set(true);
        }
    }
    let from = look_at(layer, statics, timeline, index - 1, since, now, delay, pace);
    if t <= 0.0 && elapsed < transition.delay + delay {
        return from;
    }
    Look::mix(&from, &target, t)
}

/// Moves the look by the layer's motions: its own, which run from when the
/// window opened, and the current state's, which run from when it began.
pub fn moved(
    layer: &Layer,
    look: &mut Look,
    timeline: &Timeline,
    now: Instant,
    utc_offset: i32,
    delay: Duration,
    pace: &Pace,
) {
    let wall = local_time(utc_offset);
    let mut apply = |motion: &crate::themes::Motion, elapsed: Option<Duration>| {
        let Some(elapsed) = elapsed else {
            return;
        };
        if let Some(value) = motion.value_at(elapsed) {
            match motion.property {
                Animated::X => look.x += value,
                Animated::Y => look.y += value,
                Animated::Rotate => look.rotate += value,
                Animated::Scale => look.scale *= value.max(0.0),
                Animated::Opacity => look.opacity *= value.clamp(0.0, 1.0),
            }
        }
        match motion.next_change(elapsed) {
            Some(wait) if wait.is_zero() => pace.smooth.set(true),
            Some(wait) => pace.wake_at(now + wait),
            None => {}
        }
        if motion.end().is_some_and(|end| elapsed < end) {
            pace.settling.set(true);
        }
    };
    for motion in &layer.motions {
        let elapsed = if motion.clock {
            Some(wall)
        } else {
            Some(now.saturating_duration_since(timeline.started))
        };
        apply(motion, elapsed);
    }
    let Some((state, since)) = timeline.current() else {
        return;
    };
    let since_state = now.saturating_duration_since(since);
    for change in &layer.overrides {
        if change.condition.as_state() != Some(state) {
            continue;
        }
        for motion in &change.motions {
            let elapsed = if motion.clock {
                Some(wall)
            } else if since_state < delay {
                pace.wake_at(since + delay);
                None
            } else {
                Some(since_state - delay)
            };
            apply(motion, elapsed);
        }
    }
}

/// The time since local midnight, so a motion synced to the clock is at the
/// same point at the same time of day.
fn local_time(utc_offset: i32) -> Duration {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let day = 86_400_000_000_000i128;
    let local = since_epoch.as_nanos() as i128 + i128::from(utc_offset) * 1_000_000_000;
    Duration::from_nanos(local.rem_euclid(day) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::{Easing, Override};

    fn layer(transition_ms: u64) -> Layer {
        let props = |opacity: f32| Props {
            opacity: Some(opacity),
            ..Props::default()
        };
        Layer {
            id: None,
            line: 1,
            kind: crate::themes::Kind::Text,
            parent: None,
            children: Vec::new(),
            anchor: crate::themes::Anchor::TopLeft,
            inset: [0.0; 4],
            grow: 0.0,
            monitors: crate::themes::Monitors::Form,
            base: props(1.0),
            overrides: vec![Override {
                condition: Condition::state(State::Rest),
                props: props(0.0),
                transition: None,
                motions: Vec::new(),
            }],
            transition: Transition {
                duration: Duration::from_millis(transition_ms),
                easing: Easing::Linear,
                delay: Duration::ZERO,
            },
            motions: Vec::new(),
            action: None,
            fired_by: Vec::new(),
            fired_for: Duration::ZERO,
            repeat: crate::themes::Repeat::Once,
            stagger: Duration::ZERO,
            pane: None,
        }
    }

    fn statics(layer: &Layer, timeline: &Timeline) -> Vec<Props> {
        timeline
            .changes()
            .iter()
            .map(|(state, _)| props_at(layer, *state, Flags::default(), Language::English, 0))
            .collect()
    }

    #[test]
    fn fades_between_states_and_settles() {
        let layer = layer(400);
        let start = Instant::now();
        let mut timeline = Timeline::new(start);
        timeline.observe(State::Rest, start);
        timeline.observe(State::Idle, start + Duration::from_secs(1));
        let statics = statics(&layer, &timeline);
        let pace = Pace::default();
        let at = |ms: u64| start + Duration::from_millis(1000 + ms);
        let halfway = look(&layer, &statics, &timeline, at(200), Duration::ZERO, &pace);
        assert!((halfway.opacity - 0.5).abs() < 1e-4, "{}", halfway.opacity);
        assert!(pace.settling.get() && pace.smooth.get());
        let pace = Pace::default();
        let done = look(&layer, &statics, &timeline, at(400), Duration::ZERO, &pace);
        assert_eq!(done.opacity, 1.0);
        assert!(!pace.settling.get());
    }

    #[test]
    fn an_interrupted_transition_starts_from_where_it_was() {
        let layer = layer(400);
        let start = Instant::now();
        let mut timeline = Timeline::new(start);
        timeline.observe(State::Rest, start);
        timeline.observe(State::Idle, start + Duration::from_millis(1000));
        // Back to rest halfway through the fade in.
        timeline.observe(State::Rest, start + Duration::from_millis(1200));
        let statics = statics(&layer, &timeline);
        let pace = Pace::default();
        let at_change = look(
            &layer,
            &statics,
            &timeline,
            start + Duration::from_millis(1200),
            Duration::ZERO,
            &pace,
        );
        assert!(
            (at_change.opacity - 0.5).abs() < 1e-4,
            "{}",
            at_change.opacity
        );
        let later = look(
            &layer,
            &statics,
            &timeline,
            start + Duration::from_millis(1400),
            Duration::ZERO,
            &pace,
        );
        assert!((later.opacity - 0.25).abs() < 1e-4, "{}", later.opacity);
    }

    #[test]
    fn a_delay_holds_the_old_look_and_wakes_at_its_end() {
        let layer = layer(400);
        let start = Instant::now();
        let mut timeline = Timeline::new(start);
        timeline.observe(State::Rest, start);
        timeline.observe(State::Idle, start + Duration::from_secs(1));
        let statics = statics(&layer, &timeline);
        let pace = Pace::default();
        let now = start + Duration::from_millis(1100);
        let held = look(
            &layer,
            &statics,
            &timeline,
            now,
            Duration::from_millis(300),
            &pace,
        );
        assert_eq!(held.opacity, 0.0);
        assert_eq!(pace.wake.get(), Some(start + Duration::from_millis(1300)));
        assert!(!pace.smooth.get());
    }
}
