//! A tiling container's panes laid out as xmonad lays out windows: Tall,
//! with the master on the left and the others stacked on the right, Mirror
//! Tall, the same turned on its side, and Full, the focused pane alone.

/// A rectangle in frame pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Tall,
    Mirror,
    Full,
}

impl Layout {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "tall" => Ok(Layout::Tall),
            "mirror" => Ok(Layout::Mirror),
            "full" => Ok(Layout::Full),
            _ => Err(format!(
                "{text:?} is not a layout: \"tall\", \"mirror\" or \"full\""
            )),
        }
    }

    /// The name xmonad's status bars show.
    pub fn title(self) -> &'static str {
        match self {
            Layout::Tall => "Tall",
            Layout::Mirror => "Mirror Tall",
            Layout::Full => "Full",
        }
    }
}

/// Where each of `panes` panes goes in `area`, in the order given, the
/// first being the master; `None` for a pane Full hides. `ratio` is the
/// master's share, and `gap` the space between panes and around them.
pub fn arrange(
    area: Rect,
    panes: usize,
    focused: usize,
    layout: Layout,
    ratio: f32,
    gap: f32,
) -> Vec<Option<Rect>> {
    let inner = Rect {
        x: area.x + gap / 2.0,
        y: area.y + gap / 2.0,
        width: (area.width - gap).max(0.0),
        height: (area.height - gap).max(0.0),
    };
    let shrink = |rect: Rect| Rect {
        x: rect.x + gap / 2.0,
        y: rect.y + gap / 2.0,
        width: (rect.width - gap).max(0.0),
        height: (rect.height - gap).max(0.0),
    };
    let stack = panes.saturating_sub(1);
    (0..panes)
        .map(|index| {
            let rect = match layout {
                Layout::Full => (index == focused).then_some(inner)?,
                _ if panes == 1 => inner,
                Layout::Tall => {
                    let master = (inner.width * ratio).floor();
                    if index == 0 {
                        Rect {
                            width: master,
                            ..inner
                        }
                    } else {
                        let (top, bottom) = split(inner.height, index - 1, stack);
                        Rect {
                            x: inner.x + master,
                            y: inner.y + top,
                            width: inner.width - master,
                            height: bottom - top,
                        }
                    }
                }
                Layout::Mirror => {
                    let master = (inner.height * ratio).floor();
                    if index == 0 {
                        Rect {
                            height: master,
                            ..inner
                        }
                    } else {
                        let (left, right) = split(inner.width, index - 1, stack);
                        Rect {
                            x: inner.x + left,
                            y: inner.y + master,
                            width: right - left,
                            height: inner.height - master,
                        }
                    }
                }
            };
            Some(shrink(rect))
        })
        .collect()
}

/// The `index`th of `count` equal parts of `length`, rounded down as
/// xmonad rounds.
fn split(length: f32, index: usize, count: usize) -> (f32, f32) {
    let count = count.max(1) as f32;
    let at = |i: usize| (length * i as f32 / count).floor();
    (at(index), at(index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1600.0,
        height: 876.0,
    };

    #[test]
    fn tall_stacks_beside_the_master() {
        let rects = arrange(AREA, 4, 0, Layout::Tall, 0.5, 0.0);
        let rect = |i: usize| rects[i].unwrap();
        assert_eq!(
            rect(0),
            Rect {
                x: 0.0,
                y: 0.0,
                width: 800.0,
                height: 876.0
            }
        );
        assert_eq!(
            rect(1),
            Rect {
                x: 800.0,
                y: 0.0,
                width: 800.0,
                height: 292.0
            }
        );
        assert_eq!(
            rect(3),
            Rect {
                x: 800.0,
                y: 584.0,
                width: 800.0,
                height: 292.0
            }
        );
    }

    #[test]
    fn mirror_turns_it_on_its_side() {
        let rects = arrange(AREA, 4, 2, Layout::Mirror, 0.5, 0.0);
        assert_eq!(rects[0].unwrap().height, 438.0);
        assert_eq!(
            rects[2].unwrap(),
            Rect {
                x: 533.0,
                y: 438.0,
                width: 533.0,
                height: 438.0
            }
        );
    }

    #[test]
    fn full_shows_the_focused_pane_alone() {
        let rects = arrange(AREA, 3, 1, Layout::Full, 0.5, 20.0);
        assert_eq!(rects[0], None);
        assert_eq!(
            rects[1],
            Some(Rect {
                x: 20.0,
                y: 20.0,
                width: 1560.0,
                height: 836.0
            })
        );
    }

    #[test]
    fn gaps_part_the_panes_evenly() {
        let rects = arrange(AREA, 2, 0, Layout::Tall, 0.5, 20.0);
        let (master, other) = (rects[0].unwrap(), rects[1].unwrap());
        assert_eq!(master.x, 20.0);
        assert_eq!(other.x - (master.x + master.width), 20.0);
        assert_eq!(1600.0 - (other.x + other.width), 20.0);
    }
}
