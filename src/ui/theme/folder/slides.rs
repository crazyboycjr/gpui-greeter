//! Slideshows: which slide a layer shows when, its slides made ready in the
//! background at the size the layer is drawn, and which of them are kept:
//! only the slide on screen, the one it replaces while it fades, and the
//! next, so a show of any length holds two or three pictures. The windows
//! drop the others from their GPU textures.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use gpui_kit::RenderImage;

use super::look::Pace;
use super::pictures::shrink;
use crate::config::Pick;
use crate::themes::{self, FRAME, Fit, Folder, Kind};
use crate::wallpaper::{self, Backdrop, Picture, render_image};

/// Which slide shows at each step of a slideshow: in order from `first`,
/// or shuffled, `first` and then all of them in a new order each round,
/// never the same one twice running.
#[derive(Debug, Clone, Copy)]
pub struct Schedule {
    pub count: usize,
    pub first: usize,
    pub shuffle: bool,
    pub seed: u64,
}

impl Schedule {
    /// The slide shown at `step`, and its number in its round, from 1.
    pub fn at(&self, step: u64) -> Option<(usize, usize)> {
        let count = self.count as u64;
        if count == 0 {
            return None;
        }
        let place = (step % count) as usize;
        if !self.shuffle || self.count < 3 {
            let slide = (self.first as u64 % count + place as u64) % count;
            return Some((slide as usize, place + 1));
        }
        let round = step / count;
        let mut order = self.round(round);
        if round > 0 && self.round(round - 1).last() == order.first() {
            order.swap(0, 1);
        }
        Some((order[place], place + 1))
    }

    /// A round's order before its first slide is kept from repeating the
    /// last of the round before, which moves only its first two: so the
    /// last slide of a round is always this order's.
    fn round(&self, round: u64) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.count).collect();
        let mut random = SplitMix(self.seed ^ round.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let fixed = if round == 0 {
            order.swap(0, self.first.min(self.count - 1));
            1
        } else {
            0
        };
        for end in (fixed + 1..self.count).rev() {
            let pick = fixed + (random.next() % (end - fixed + 1) as u64) as usize;
            order.swap(end, pick);
        }
        order
    }
}

/// SplitMix64: random enough to shuffle slides.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// Where a slide's picture is read from.
#[derive(Debug, Clone)]
enum Source {
    /// A picture in the theme folder, read within the theme's limits.
    Theme { dir: PathBuf, path: String },
    /// A picture in the wallpaper folder.
    Wallpaper(PathBuf),
}

/// A slide made ready to draw.
#[derive(Clone)]
pub enum Made {
    Picture(Arc<RenderImage>),
    /// A wallpaper with its blurred forms.
    Backdrop(Backdrop),
}

impl Made {
    pub fn images(&self) -> impl Iterator<Item = &Arc<RenderImage>> {
        let (first, rest) = match self {
            Made::Picture(image) => (image, None),
            Made::Backdrop(backdrop) => {
                (&backdrop.sharp, Some([&backdrop.soft, &backdrop.blurred]))
            }
        };
        std::iter::once(first).chain(rest.into_iter().flatten())
    }
}

/// A slide to make in the background.
pub struct Request {
    generation: u64,
    layer: usize,
    slide: usize,
    source: Source,
    /// The box it is drawn in, in device pixels.
    made_for: (u32, u32),
    fit: Fit,
    /// A wallpaper's blur, in device pixels, for its blurred forms.
    blur: Option<f32>,
}

impl Request {
    /// The slide, as large as its fit draws it in its box and never larger
    /// than its picture.
    pub fn make(&self) -> Result<Made> {
        match &self.source {
            Source::Theme { dir, path } => {
                let bytes = themes::read_file(dir, path).map_err(anyhow::Error::msg)?;
                let image = themes::decode_picture(&bytes)?;
                let (size, _) = fitted(image.dimensions(), self.made_for, self.fit);
                let image = if size == image.dimensions() {
                    image
                } else {
                    shrink(&image, size)?
                };
                Ok(Made::Picture(render_image(image)))
            }
            Source::Wallpaper(path) => {
                let picture = Picture::open(path)?;
                let (size, shrunk) = fitted(picture.dimensions(), self.made_for, self.fit);
                Ok(match self.blur {
                    Some(sigma) => {
                        Made::Backdrop(Backdrop::new(&picture, size, sigma * shrunk, 1.0)?)
                    }
                    None => Made::Picture(render_image(picture.cover(size)?)),
                })
            }
        }
    }
}

/// The size to make a picture of `natural` pixels for a box of `made_for`
/// as `fit` draws it there, never larger than the picture, and what that
/// size is to the size it is drawn at.
fn fitted(natural: (u32, u32), made_for: (u32, u32), fit: Fit) -> ((u32, u32), f32) {
    let across = made_for.0 as f32 / natural.0.max(1) as f32;
    let down = made_for.1 as f32 / natural.1.max(1) as f32;
    let drawn = match fit {
        Fit::Contain => across.min(down),
        Fit::Cover | Fit::Stretch => across.max(down),
    };
    let scale = drawn.min(1.0);
    let side = |pixels: u32| ((pixels as f32 * scale).round() as u32).max(1);
    (
        (side(natural.0), side(natural.1)),
        scale / drawn.max(f32::EPSILON),
    )
}

/// A slide on screen.
#[derive(Debug, Clone, Copy)]
struct On {
    slide: usize,
    /// Its number in its round, from 1.
    number: usize,
    /// When it began to fade in.
    since: Instant,
}

/// Where one slideshow stands.
struct Show {
    /// Each slide's name, for `{slide}`, and where it is read from.
    slides: Vec<(String, Source)>,
    schedule: Schedule,
    interval: Duration,
    crossfade: Duration,
    fit: Fit,
    blur: Option<f32>,
    on: Option<On>,
    /// The slide `on` replaces, while it fades in.
    leaving: Option<On>,
    /// The slides made, with the box each was made for.
    made: Vec<(usize, Made, (u32, u32))>,
    /// The slides asked for and not made yet.
    asked: Vec<usize>,
    /// The slides that cannot be made: their turns go to the slide before.
    failed: Vec<usize>,
    /// The largest box the layer has been drawn in, in device pixels.
    drawn: Rc<Cell<(u32, u32)>>,
}

impl Show {
    /// The slideshow of layer `index` of `folder`. A wallpaper's shows the
    /// pictures in the wallpaper folder from the one picked, blurred, if it
    /// blurs them, by its blur times `unit`, the largest monitor's pixels
    /// to a frame pixel.
    fn of(
        folder: &Folder,
        index: usize,
        (wallpapers, pick): (&Path, &Pick),
        unit: f32,
        seed: u64,
    ) -> Option<Self> {
        let layer = &folder.layers[index];
        let show = layer.kind.slideshow()?;
        let name = |path: &Path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let (picture, blur, slides, first) = match &layer.kind {
            Kind::Wallpaper { picture, blur, .. } => {
                let pictures = wallpaper::candidates(wallpapers);
                let first = wallpaper::picked(&pictures, pick)
                    .and_then(|picked| pictures.iter().position(|path| *path == picked))
                    .unwrap_or(0);
                let slides: Vec<(String, Source)> = pictures
                    .into_iter()
                    .map(|path| (name(&path), Source::Wallpaper(path)))
                    .collect();
                (
                    picture,
                    (*blur > 0.0).then_some(*blur * unit),
                    slides,
                    first,
                )
            }
            Kind::Image { picture, .. } => {
                let slides = show
                    .files
                    .iter()
                    .map(|path| {
                        let source = Source::Theme {
                            dir: folder.dir.clone(),
                            path: path.clone(),
                        };
                        (name(Path::new(path)), source)
                    })
                    .collect();
                (picture, None, slides, 0)
            }
            _ => return None,
        };
        Some(Self {
            schedule: Schedule {
                count: slides.len(),
                first,
                shuffle: show.shuffle,
                seed: seed ^ index as u64,
            },
            slides,
            interval: show.interval,
            crossfade: show.crossfade,
            fit: picture.fit,
            blur,
            on: None,
            leaving: None,
            made: Vec::new(),
            asked: Vec::new(),
            failed: Vec::new(),
            drawn: Rc::new(Cell::new((0, 0))),
        })
    }

    fn step(&self, started: Instant, now: Instant) -> u64 {
        let shown = now.saturating_duration_since(started).as_nanos();
        (shown / self.interval.as_nanos().max(1)) as u64
    }

    /// When `step` begins, after the show began.
    fn start_of(&self, step: u64) -> Duration {
        Duration::from_nanos((self.interval.as_nanos() * u128::from(step)) as u64)
    }

    fn made(&self, slide: usize) -> Option<&Made> {
        self.made
            .iter()
            .find(|(made, ..)| *made == slide)
            .map(|(_, made, _)| made)
    }

    /// How far the slide on screen has faded in at `now`, from 0 to 1.
    fn fade(&self, now: Instant) -> f32 {
        match self.on {
            Some(on) if !self.crossfade.is_zero() => {
                let shown = now.saturating_duration_since(on.since);
                (shown.as_secs_f32() / self.crossfade.as_secs_f32()).min(1.0)
            }
            _ => 1.0,
        }
    }

    /// Lets go of the slides that are neither on screen nor due.
    fn keep(&mut self, step: u64) {
        let wanted = [
            self.on.map(|on| on.slide),
            self.leaving.map(|leaving| leaving.slide),
            self.schedule.at(step).map(|(slide, _)| slide),
            self.schedule.at(step + 1).map(|(slide, _)| slide),
        ];
        self.made
            .retain(|(slide, ..)| wanted.contains(&Some(*slide)));
    }
}

/// What a slideshow layer shows now.
pub struct Showing {
    /// The slide on screen, and its opacity as it fades in.
    pub on: Option<(Made, f32)>,
    /// The slide it replaces, under it while it fades in.
    pub leaving: Option<Made>,
}

/// The slideshows of the theme folder shown, for every window.
pub struct Slides {
    shows: RefCell<HashMap<usize, Show>>,
    started: Instant,
    /// Tells the slides of one theme from those of the one before.
    generation: u64,
    requests: async_channel::Sender<Request>,
    /// The largest monitor, which slides are made for until their layer is
    /// first drawn.
    largest: (u32, u32),
}

impl Slides {
    /// The slideshows of `folder`, whose slides `requests` takes to be made.
    /// A wallpaper slideshow shows the pictures in `wallpapers`, from the
    /// one `pick` chooses. Each show's first two slides are asked for at
    /// once, so they are ready when it first shows.
    pub fn new(
        folder: Option<&Folder>,
        wallpapers: (&Path, &Pick),
        largest: (u32, u32),
        generation: u64,
        requests: async_channel::Sender<Request>,
    ) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos() as u64)
            ^ u64::from(std::process::id());
        let unit = (largest.0 as f32 / FRAME.0).min(largest.1 as f32 / FRAME.1);
        let mut shows = HashMap::new();
        if let Some(folder) = folder {
            for &index in &folder.slideshows {
                if let Some(show) = Show::of(folder, index, wallpapers, unit, seed) {
                    shows.insert(index, show);
                }
            }
        }
        let slides = Self {
            shows: RefCell::new(shows),
            started: Instant::now(),
            generation,
            requests,
            largest,
        };
        for (&layer, show) in slides.shows.borrow_mut().iter_mut() {
            for step in [0, 1] {
                if let Some((slide, _)) = show.schedule.at(step) {
                    slides.ask(layer, show, slide);
                }
            }
        }
        slides
    }

    /// What layer `layer`'s slideshow shows at `now`. A slide comes on once
    /// it is made, the one before staying till then; the slides due next are
    /// asked for, and `pace` hears when to draw again.
    pub fn showing(&self, layer: usize, now: Instant, pace: &Pace) -> Option<Showing> {
        let mut shows = self.shows.borrow_mut();
        let show = shows.get_mut(&layer)?;
        let step = show.step(self.started, now);
        let (due, number) = show.schedule.at(step)?;
        if show.on.is_none_or(|on| on.slide != due) && show.made(due).is_some() {
            // A layer not drawn before shows its first slide at once.
            let since = match show.on {
                None if show.drawn.get() == (0, 0) => {
                    now.checked_sub(show.crossfade).unwrap_or(now)
                }
                _ => now,
            };
            show.leaving = show.on;
            show.on = Some(On {
                slide: due,
                number,
                since,
            });
        }
        let fade = show.fade(now);
        if fade < 1.0 {
            pace.smooth.set(true);
            pace.settling.set(true);
        } else {
            show.leaving = None;
        }
        if show.made(due).is_none() && !show.failed.contains(&due) {
            pace.settling.set(true);
        }
        pace.wake_at(self.started + show.start_of(step + 1));
        show.keep(step);
        let next = show.schedule.at(step + 1).map(|(slide, _)| slide);
        let on = show.on.map(|on| on.slide);
        for slide in [Some(due), next, on].into_iter().flatten() {
            self.ask(layer, show, slide);
        }
        let eased = fade * fade * (3.0 - 2.0 * fade);
        Some(Showing {
            on: show
                .on
                .and_then(|on| Some((show.made(on.slide)?.clone(), eased))),
            leaving: show
                .leaving
                .and_then(|leaving| show.made(leaving.slide).cloned()),
        })
    }

    /// Asks for `slide` to be made, unless it is made for a box near as large
    /// as the layer's now, on its way, or cannot be made.
    fn ask(&self, layer: usize, show: &mut Show, slide: usize) {
        let made_for = match show.drawn.get() {
            (0, _) | (_, 0) => self.largest,
            drawn => drawn,
        };
        let large_enough = |size: (u32, u32)| {
            u64::from(size.0) * 10 >= u64::from(made_for.0) * 9
                && u64::from(size.1) * 10 >= u64::from(made_for.1) * 9
        };
        let made = show
            .made
            .iter()
            .any(|(made, _, size)| *made == slide && large_enough(*size));
        if made || show.asked.contains(&slide) || show.failed.contains(&slide) {
            return;
        }
        let Some((_, source)) = show.slides.get(slide) else {
            return;
        };
        let request = Request {
            generation: self.generation,
            layer,
            slide,
            source: source.clone(),
            made_for,
            fit: show.fit,
            blur: show.blur,
        };
        if self.requests.try_send(request).is_ok() {
            show.asked.push(slide);
        }
    }

    /// A slide made in the background at `now`, or why it could not be.
    pub fn arrived(&self, request: &Request, made: Result<Made>, now: Instant) {
        if request.generation != self.generation {
            return;
        }
        let mut shows = self.shows.borrow_mut();
        let Some(show) = shows.get_mut(&request.layer) else {
            return;
        };
        show.asked.retain(|slide| *slide != request.slide);
        match made {
            Ok(made) => {
                show.made.retain(|(slide, ..)| *slide != request.slide);
                show.made.push((request.slide, made, request.made_for));
                show.keep(show.step(self.started, now));
            }
            Err(error) => {
                let name = show.slides.get(request.slide).map_or("", |(name, _)| name);
                log::error!("cannot show the slide {name}: {error:#}");
                show.failed.push(request.slide);
            }
        }
    }

    /// The name, the number and the count of the slide layer `layer`'s
    /// slideshow shows, the one leaving until the next is half in, or of the
    /// one due while none shows yet.
    pub fn caption(&self, layer: usize, now: Instant) -> Option<(String, usize, usize)> {
        let shows = self.shows.borrow();
        let show = shows.get(&layer)?;
        let shown = match (show.on, show.leaving) {
            (Some(_), Some(leaving)) if show.fade(now) < 0.5 => Some(leaving),
            (on, _) => on,
        };
        let (slide, number) = match shown {
            Some(shown) => (shown.slide, shown.number),
            None => show.schedule.at(show.step(self.started, now))?,
        };
        Some((show.slides.get(slide)?.0.clone(), number, show.slides.len()))
    }

    /// Where layer `layer` notes the largest box it is drawn in.
    pub fn drawn(&self, layer: usize) -> Option<Rc<Cell<(u32, u32)>>> {
        self.shows
            .borrow()
            .get(&layer)
            .map(|show| show.drawn.clone())
    }

    /// Whether `image` is a slide still kept, which windows go on drawing.
    pub fn holds(&self, image: &RenderImage) -> bool {
        self.shows.borrow().values().any(|show| {
            show.made
                .iter()
                .any(|(_, made, _)| made.images().any(|own| **own == *image))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn goes_in_order_from_the_first() {
        let schedule = Schedule {
            count: 4,
            first: 2,
            shuffle: false,
            seed: 1,
        };
        let shown: Vec<(usize, usize)> = (0..6).filter_map(|step| schedule.at(step)).collect();
        assert_eq!(shown, [(2, 1), (3, 2), (0, 3), (1, 4), (2, 1), (3, 2)]);
        let empty = Schedule {
            count: 0,
            ..schedule
        };
        assert_eq!(empty.at(0), None);
    }

    #[test]
    fn a_shuffle_shows_each_slide_once_a_round_and_none_twice_running() {
        let mut orders = Vec::new();
        for seed in 0..300 {
            let schedule = Schedule {
                count: 5,
                first: 3,
                shuffle: true,
                seed,
            };
            let shown: Vec<(usize, usize)> = (0..60).filter_map(|step| schedule.at(step)).collect();
            let slides: Vec<usize> = shown.iter().map(|(slide, _)| *slide).collect();
            assert_eq!(slides[0], 3, "the first slide opens the show");
            for round in slides.chunks(5) {
                let mut sorted = round.to_vec();
                sorted.sort_unstable();
                assert_eq!(sorted, [0, 1, 2, 3, 4], "{slides:?}");
            }
            assert!(
                slides.windows(2).all(|pair| pair[0] != pair[1]),
                "{slides:?}"
            );
            let numbers: Vec<usize> = shown.iter().map(|(_, number)| *number).collect();
            assert_eq!(numbers[..7], [1, 2, 3, 4, 5, 1, 2]);
            orders.push(slides);
        }
        orders.sort();
        orders.dedup();
        assert!(orders.len() > 250, "shuffles differ from seed to seed");
        let two = Schedule {
            count: 2,
            first: 1,
            shuffle: true,
            seed: 9,
        };
        let shown: Vec<usize> = (0..4)
            .filter_map(|step| two.at(step))
            .map(|(s, _)| s)
            .collect();
        assert_eq!(shown, [1, 0, 1, 0]);
    }

    #[test]
    fn makes_a_slide_no_larger_than_its_picture_or_its_box() {
        let (size, shrunk) = fitted((4000, 2000), (1000, 1000), Fit::Cover);
        assert_eq!((size, shrunk), ((2000, 1000), 1.0));
        assert_eq!(
            fitted((4000, 2000), (1000, 1000), Fit::Contain).0,
            (1000, 500)
        );
        let (size, shrunk) = fitted((800, 600), (1600, 1200), Fit::Cover);
        assert_eq!((size, shrunk), ((800, 600), 0.5));
    }

    fn show(count: usize) -> (Slides, async_channel::Receiver<Request>) {
        let (requests, asked) = async_channel::unbounded();
        let show = Show {
            slides: (0..count)
                .map(|n| {
                    let path = format!("slides/{n}.jpg");
                    (
                        format!("{n}.jpg"),
                        Source::Theme {
                            dir: PathBuf::new(),
                            path,
                        },
                    )
                })
                .collect(),
            schedule: Schedule {
                count,
                first: 0,
                shuffle: false,
                seed: 0,
            },
            interval: Duration::from_secs(10),
            crossfade: Duration::from_millis(600),
            fit: Fit::Cover,
            blur: None,
            on: None,
            leaving: None,
            made: Vec::new(),
            asked: Vec::new(),
            failed: Vec::new(),
            drawn: Rc::new(Cell::new((0, 0))),
        };
        let slides = Slides {
            shows: RefCell::new(HashMap::from([(7, show)])),
            started: Instant::now(),
            generation: 3,
            requests,
            largest: (3840, 2160),
        };
        (slides, asked)
    }

    fn made() -> Result<Made> {
        Ok(Made::Picture(render_image(RgbaImage::new(2, 2))))
    }

    fn kept(slides: &Slides) -> Vec<usize> {
        let shows = slides.shows.borrow();
        let mut kept: Vec<usize> = shows[&7].made.iter().map(|(slide, ..)| *slide).collect();
        kept.sort_unstable();
        kept
    }

    #[test]
    fn a_slide_comes_on_once_made_and_fades_in_over_the_one_before() {
        let (slides, asked) = show(4);
        let at = |ms: u64| slides.started + Duration::from_millis(ms);
        let pace = Pace::default();
        let showing = slides.showing(7, at(0), &pace).unwrap();
        assert!(showing.on.is_none() && pace.settling.get());
        let (first, second) = (asked.try_recv().unwrap(), asked.try_recv().unwrap());
        assert_eq!((first.slide, second.slide), (0, 1));
        assert_eq!(
            first.made_for,
            (3840, 2160),
            "for the largest monitor at first"
        );
        assert!(asked.try_recv().is_err(), "each asked for once");

        // The layer was drawn empty meanwhile, so its first slide fades in.
        slides.drawn(7).unwrap().set((1200, 700));
        slides.arrived(&first, made(), at(50));
        slides.arrived(&second, made(), at(60));
        let pace = Pace::default();
        let showing = slides.showing(7, at(100), &pace).unwrap();
        assert_eq!(showing.on.map(|(_, opacity)| opacity), Some(0.0));
        assert!(pace.smooth.get());
        let pace = Pace::default();
        let showing = slides.showing(7, at(1000), &pace).unwrap();
        assert_eq!(showing.on.map(|(_, opacity)| opacity), Some(1.0));
        assert!(!pace.smooth.get() && !pace.settling.get());
        assert_eq!(
            pace.wake.get(),
            Some(at(10_000)),
            "drawn again when the next is due"
        );

        // At ten seconds the second slide fades in over the first, and the
        // third is asked for, for the box the layer is drawn in.
        let pace = Pace::default();
        let showing = slides.showing(7, at(10_000), &pace).unwrap();
        assert!(showing.leaving.is_some() && showing.on.is_some());
        let third = asked.try_recv().unwrap();
        assert_eq!((third.slide, third.made_for), (2, (1200, 700)));
        assert_eq!(
            slides.caption(7, at(10_000)),
            Some(("0.jpg".into(), 1, 4)),
            "named for the picture that shows the most"
        );
        assert_eq!(slides.caption(7, at(10_400)), Some(("1.jpg".into(), 2, 4)));
        slides.arrived(&third, made(), at(10_100));
        assert_eq!(kept(&slides), [0, 1, 2]);

        // Once it has faded in, the first is let go of.
        let leaving = showing.leaving.unwrap();
        let image = leaving.images().next().unwrap().clone();
        assert!(slides.holds(&image));
        let showing = slides.showing(7, at(10_700), &Pace::default()).unwrap();
        assert!(showing.leaving.is_none());
        assert_eq!(kept(&slides), [1, 2]);
        assert!(!slides.holds(&image));
    }

    #[test]
    fn a_late_slide_leaves_the_one_before_on_screen() {
        let (slides, asked) = show(3);
        let at = |ms: u64| slides.started + Duration::from_millis(ms);
        slides.showing(7, at(0), &Pace::default());
        let (first, second) = (asked.try_recv().unwrap(), asked.try_recv().unwrap());
        slides.arrived(&first, made(), at(50));
        let showing = slides.showing(7, at(100), &Pace::default()).unwrap();
        assert_eq!(
            showing.on.map(|(_, opacity)| opacity),
            Some(1.0),
            "never drawn, so at once"
        );

        // The second is still being made at its turn.
        let pace = Pace::default();
        let showing = slides.showing(7, at(10_000), &pace).unwrap();
        assert!(showing.on.is_some() && showing.leaving.is_none());
        assert!(pace.settling.get());
        assert_eq!(slides.caption(7, at(10_000)), Some(("0.jpg".into(), 1, 3)));
        let third = asked.try_recv().unwrap();
        assert_eq!(third.slide, 2);

        // One that cannot be made gives its turn to the one before, and is
        // never asked for again.
        slides.arrived(&second, Err(anyhow::anyhow!("broken")), at(12_000));
        let showing = slides.showing(7, at(15_000), &Pace::default()).unwrap();
        assert!(showing.on.is_some());
        assert!(asked.try_recv().is_err());

        // A slide made for a theme shown before this one is dropped.
        let stale = Request {
            generation: 2,
            ..third
        };
        slides.arrived(&stale, made(), at(16_000));
        assert_eq!(kept(&slides), [0]);
    }
}
