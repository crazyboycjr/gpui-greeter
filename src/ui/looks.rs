//! The looks the theme key switches between at the login screen, each with
//! what it draws: a built-in look's backgrounds, a theme folder and its
//! pictures, for every monitor. Every look is made ready in the background,
//! one at a time and nearest first, before it is needed, and kept while
//! the pictures of the looks not shown fit in `BUDGET`. A look is ready to
//! show as soon as it can be drawn at all: what each monitor draws for it
//! follows when done, and fades in.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gpui_kit::{Context, RenderImage};

use super::model::Greeter;
use super::theme::folder::{MonitorPictures, Pictures, asset_key, pictures};
use crate::cache;
use crate::config::{Config, Theme};
use crate::scenery::{self, Look, Scenery};
use crate::themes::{self, AssetKind, FRAME_PIXELS_PER_POINT, Folder, valid_name};
use crate::x11::Monitor;

/// The SVGs GPUI's asset source serves, which grows as theme folders load.
pub type SvgAssets = Arc<RwLock<HashMap<String, Arc<[u8]>>>>;

/// How many bytes of pictures the looks not shown may keep: a 4K picture
/// is 33 MB, and a look one to four of them.
const BUDGET: usize = 512 << 20;

/// What a look draws on one monitor.
#[derive(Clone, Default)]
pub struct ScreenAssets {
    pub scenery: Option<Arc<Scenery>>,
    /// A theme folder's SVGs drawn for this monitor; `None` while drawing.
    pub drawn: Option<Arc<MonitorPictures>>,
}

/// A look made ready.
#[derive(Clone, Default)]
pub struct Prepared {
    pub folder: Option<Arc<Folder>>,
    pub pictures: Option<Arc<Pictures>>,
    /// By monitor, in the order of the windows.
    pub screens: Vec<ScreenAssets>,
}

impl Prepared {
    /// The pictures it keeps in memory, for GPUI to draw.
    pub fn images(&self) -> impl Iterator<Item = &Arc<RenderImage>> {
        let pictures = self
            .pictures
            .iter()
            .flat_map(|pictures| pictures.images.values());
        let screens = self.screens.iter().flat_map(|screen| {
            let scenery = screen.scenery.iter().map(|scenery| &scenery.picture);
            scenery.chain(screen.drawn.iter().flat_map(|drawn| drawn.images()))
        });
        pictures.chain(screens)
    }

    /// The bytes of its pictures: those GPUI draws, and a theme folder's
    /// own, decoded.
    fn bytes(&self) -> usize {
        let images: usize = self
            .images()
            .filter_map(|image| image.as_bytes(0))
            .map(<[u8]>::len)
            .sum();
        let own: usize = self
            .folder
            .iter()
            .flat_map(|folder| folder.assets.iter())
            .map(|asset| match &asset.kind {
                AssetKind::Raster(image) => image.as_raw().len(),
                _ => 0,
            })
            .sum();
        images + own
    }
}

pub struct Looks {
    /// What the theme key cycles through.
    pub cycle: Vec<Theme>,
    /// A look the greeter waits for, to switch to once ready.
    pub wanted: Option<Theme>,
    themes_dir: PathBuf,
    /// The settings, for each look's backgrounds.
    config: Config,
    monitors: Vec<Monitor>,
    ready: HashMap<Theme, Prepared>,
    loading: HashSet<Theme>,
    failed: HashSet<Theme>,
    /// The looks still to make ready, nearest to the one shown first.
    queue: Vec<Theme>,
    /// Counts the looks let go, so that windows know when to let go of
    /// their pictures on the GPU.
    pub released: u64,
    svgs: SvgAssets,
}

impl Looks {
    pub fn new(
        config: &Config,
        monitors: &[Monitor],
        first: Theme,
        prepared: Prepared,
        svgs: SvgAssets,
        switching: bool,
    ) -> Self {
        let cycle = if switching {
            cycle(config, &first)
        } else {
            vec![first.clone()]
        };
        Self {
            cycle,
            wanted: None,
            themes_dir: config.themes.dir.clone(),
            config: config.clone(),
            monitors: monitors.to_vec(),
            ready: HashMap::from([(first, prepared)]),
            loading: HashSet::new(),
            failed: HashSet::new(),
            queue: Vec::new(),
            released: 0,
            svgs,
        }
    }

    /// Whether a look that is ready keeps `image`, which a window may then
    /// keep on the GPU.
    pub fn holds(&self, image: &Arc<RenderImage>) -> bool {
        self.ready
            .values()
            .any(|prepared| prepared.images().any(|kept| Arc::ptr_eq(kept, image)))
    }

    pub fn get(&self, theme: &Theme) -> Option<&Prepared> {
        self.ready.get(theme)
    }

    /// What `theme` draws on monitor `output`.
    pub fn screen(&self, theme: &Theme, output: usize) -> ScreenAssets {
        self.ready
            .get(theme)
            .and_then(|prepared| prepared.screens.get(output).cloned())
            .unwrap_or_default()
    }

    /// The look `step` places from `current` in the cycle.
    pub fn neighbour(&self, current: &Theme, step: isize) -> Option<Theme> {
        let count = self.cycle.len() as isize;
        if count == 0 {
            return None;
        }
        let next = match self.cycle.iter().position(|theme| theme == current) {
            Some(at) => (at as isize + step).rem_euclid(count),
            // Outside the cycle, the first step goes to its start or end.
            None if step > 0 => 0,
            None => count - 1,
        };
        Some(self.cycle[next as usize].clone()).filter(|theme| theme != current)
    }

    pub fn is_ready(&self, theme: &Theme) -> bool {
        self.ready.contains_key(theme)
    }

    pub fn has_failed(&self, theme: &Theme) -> bool {
        self.failed.contains(theme)
    }

    /// What a look draws on a monitor, done after its window opened.
    pub fn set_screen(&mut self, theme: &Theme, output: usize, assets: ScreenAssets) {
        if let Some(screen) = self
            .ready
            .get_mut(theme)
            .and_then(|prepared| prepared.screens.get_mut(output))
        {
            *screen = assets;
        }
    }

    /// Makes `theme` ready in the background, unless it is or is on its way.
    /// The greeter hears as soon as it can be drawn: at once for a built-in
    /// look, once loaded for a theme folder; what each monitor draws for it
    /// follows when done.
    pub fn prepare(&mut self, theme: &Theme, cx: &mut Context<Greeter>) {
        if self.ready.contains_key(theme)
            || self.loading.contains(theme)
            || self.failed.contains(theme)
        {
            return;
        }
        self.loading.insert(theme.clone());
        let mut config = self.config.clone();
        config.theme = theme.clone();
        let (theme, monitors, dir) = (
            theme.clone(),
            self.monitors.clone(),
            self.themes_dir.clone(),
        );
        cx.spawn(async move |this, cx| {
            let started = Instant::now();
            let made = cx
                .background_executor()
                .spawn(async move { begin(&config, &dir, &monitors) })
                .await;
            let _ = this.update(cx, |greeter, cx| {
                let looks = &mut greeter.looks;
                looks.loading.remove(&theme);
                match made {
                    Some((prepared, later)) => {
                        log::info!("look {} ready in {:?}", theme.name(), started.elapsed());
                        if let Some(folder) = &prepared.folder {
                            looks.register(folder, cx);
                        }
                        looks.ready.insert(theme.clone(), prepared);
                        for (output, thread) in later {
                            deliver(theme.clone(), output, thread, cx);
                        }
                    }
                    None => {
                        looks.failed.insert(theme.clone());
                        if looks.wanted.as_ref() == Some(&theme) {
                            looks.wanted = None;
                        }
                    }
                }
                greeter.look_ready(&theme, cx);
                let shown = greeter.theme().clone();
                let looks = &mut greeter.looks;
                looks.fit(&shown);
                looks.prepare_next(cx);
            });
        })
        .detach();
    }

    /// Makes every look in the cycle ready, one at a time, nearest to
    /// `shown` first, for as long as the budget keeps them.
    pub fn prepare_all(&mut self, shown: &Theme, cx: &mut Context<Greeter>) {
        let count = self.cycle.len();
        let at = self
            .cycle
            .iter()
            .position(|theme| theme == shown)
            .unwrap_or(0);
        let mut queue: Vec<Theme> = Vec::new();
        for distance in 1..count {
            for index in [at + distance, at + count - distance] {
                let theme = &self.cycle[index % count];
                if theme != shown && !queue.contains(theme) {
                    queue.push(theme.clone());
                }
            }
        }
        self.queue = queue;
        self.fit(shown);
        self.prepare_next(cx);
    }

    /// Makes the next look in the queue ready, unless one already is being.
    fn prepare_next(&mut self, cx: &mut Context<Greeter>) {
        if !self.loading.is_empty() {
            return;
        }
        while !self.queue.is_empty() {
            let theme = self.queue.remove(0);
            if !self.ready.contains_key(&theme) && !self.failed.contains(&theme) {
                self.prepare(&theme, cx);
                return;
            }
        }
    }

    /// The bytes of pictures the ready looks other than `shown` keep.
    fn kept(&self, shown: &Theme) -> usize {
        self.ready
            .iter()
            .filter(|&(theme, _)| theme != shown)
            .map(|(_, prepared)| prepared.bytes())
            .sum()
    }

    /// Lets go of the ready looks farthest from `shown` while those not
    /// shown keep more than the budget, and makes no more ready then.
    fn fit(&mut self, shown: &Theme) {
        self.fit_within(shown, BUDGET);
    }

    fn fit_within(&mut self, shown: &Theme, budget: usize) {
        let count = self.cycle.len().max(1);
        let at = self.cycle.iter().position(|theme| theme == shown);
        let distance =
            |theme: &Theme| match (at, self.cycle.iter().position(|other| other == theme)) {
                (Some(at), Some(other)) => {
                    let ahead = other.abs_diff(at);
                    ahead.min(count - ahead)
                }
                _ => count,
            };
        loop {
            let kept = self.kept(shown);
            if kept <= budget {
                log::debug!("the looks not shown keep {} MB of pictures", kept >> 20);
                return;
            }
            let farthest = self
                .ready
                .keys()
                .filter(|&theme| theme != shown && Some(theme) != self.wanted.as_ref())
                .max_by_key(|&theme| distance(theme))
                .cloned();
            let Some(farthest) = farthest else {
                return;
            };
            log::info!(
                "look {} let go: the looks not shown keep {} MB",
                farthest.name(),
                kept >> 20
            );
            self.ready.remove(&farthest);
            self.queue.clear();
            self.released += 1;
        }
    }

    /// Hands GPUI a theme folder's fonts and the SVGs it tints.
    pub fn register(&self, folder: &Folder, cx: &mut Context<Greeter>) {
        let fonts: Vec<std::borrow::Cow<'static, [u8]>> = folder
            .fonts
            .iter()
            .map(|font| std::borrow::Cow::Owned(font.bytes.to_vec()))
            .collect();
        if !fonts.is_empty()
            && let Err(error) = cx.text_system().add_fonts(fonts)
        {
            log::error!("theme {}: cannot load its fonts: {error:#}", folder.name);
        }
        add_svgs(&self.svgs, folder);
    }
}

/// Adds a theme folder's SVGs to the asset source.
pub fn add_svgs(svgs: &SvgAssets, folder: &Folder) {
    let mut svgs = svgs.write().unwrap_or_else(PoisonError::into_inner);
    for asset in &folder.assets {
        if let AssetKind::Svg { bytes, .. } = &asset.kind {
            svgs.insert(asset_key(&folder.name, &asset.path), bytes.clone());
        }
    }
}

/// The looks to cycle through: the settings' list, or every built-in look
/// and every installed theme folder; the current one is always in it.
pub fn cycle(config: &Config, current: &Theme) -> Vec<Theme> {
    let installed = installed(&config.themes.dir);
    let available = |theme: &Theme| match theme {
        Theme::Folder(name) => installed.contains(name),
        _ => true,
    };
    let mut cycle: Vec<Theme> = if config.themes.cycle.is_empty() {
        Theme::BUILT_IN
            .into_iter()
            .chain(installed.iter().cloned().map(Theme::Folder))
            .collect()
    } else {
        let mut listed: Vec<Theme> = Vec::new();
        for theme in config.themes.cycle.iter().filter(|theme| available(theme)) {
            if !listed.contains(theme) {
                listed.push(theme.clone());
            }
        }
        listed
    };
    if !cycle.contains(current) {
        cycle.insert(0, current.clone());
    }
    cycle
}

/// The theme folders installed in `dir`, by name.
fn installed(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().join(themes::MANIFEST).is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_name(name) && !themes::RESERVED.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

/// What each monitor draws for a look, still drawing, by monitor.
pub type Later = Vec<(usize, JoinHandle<ScreenAssets>)>;

/// Starts drawing what `config.theme` needs for these monitors, and hands
/// the look back as soon as it can be drawn: a theme folder loaded and its
/// pictures decoded, with what each monitor draws still drawing. `None`
/// for a theme folder that cannot be loaded.
fn begin(config: &Config, themes_dir: &Path, monitors: &[Monitor]) -> Option<(Prepared, Later)> {
    let folder = match &config.theme {
        Theme::Folder(name) => Some(Arc::new(themes::load_configured(themes_dir, name)?)),
        _ => None,
    };
    Some(start(config, folder.as_ref(), monitors).finish(Some(Duration::ZERO)))
}

/// Hands the greeter what `theme` draws on monitor `output`, once drawn.
pub fn deliver(
    theme: Theme,
    output: usize,
    thread: JoinHandle<ScreenAssets>,
    cx: &mut Context<Greeter>,
) {
    cx.spawn(async move |this, cx| {
        let assets = cx
            .background_executor()
            .spawn(async move { thread.join().unwrap_or_default() })
            .await;
        let _ = this.update(cx, |greeter, cx| {
            greeter.looks.set_screen(&theme, output, assets);
            let shown = greeter.theme().clone();
            greeter.looks.fit(&shown);
            cx.notify();
        });
    })
    .detach();
}

/// A look being drawn on threads of its own, one for each monitor.
pub struct Preparing {
    folder: Option<Arc<Folder>>,
    pictures: Option<JoinHandle<Pictures>>,
    screens: Vec<JoinHandle<ScreenAssets>>,
}

/// Starts drawing a look: a built-in look's backgrounds, or a theme
/// folder's pictures.
pub fn start(config: &Config, folder: Option<&Arc<Folder>>, monitors: &[Monitor]) -> Preparing {
    let screens: Vec<((u32, u32), f32)> = monitors
        .iter()
        .map(|monitor| {
            (
                (monitor.rect.width, monitor.rect.height),
                monitor.pixels_per_point(),
            )
        })
        .collect();
    start_sized(config, folder, &screens)
}

/// Starts drawing a look for screens of these sizes in pixels, each with
/// so many pixels to a point, before the settings' scale.
pub fn start_sized(
    config: &Config,
    folder: Option<&Arc<Folder>>,
    screens: &[((u32, u32), f32)],
) -> Preparing {
    let look = Look::of(config);
    let largest = screens.iter().fold((1, 1), |(w, h), ((width, height), _)| {
        (w.max(*width), h.max(*height))
    });
    let pictures = folder
        .cloned()
        .map(|folder| std::thread::spawn(move || pictures::decode(&folder, largest)));
    let scale = config.scale;
    let screens = screens
        .iter()
        .map(|&(pixels, per_point)| {
            let folder = folder.cloned();
            let per_point = per_point * scale;
            std::thread::spawn(move || {
                let started = Instant::now();
                let scenery = look.and_then(|look| {
                    let cache = cache::dir("scenery");
                    match scenery::draw(look, pixels, per_point.into(), cache.as_deref()) {
                        Ok(scenery) => Some(Arc::new(scenery)),
                        Err(error) => {
                            log::error!("cannot draw the scenery for {pixels:?}: {error:#}");
                            None
                        }
                    }
                });
                let drawn = folder.map(|folder| {
                    let cache = cache::dir("themes").map(|dir| dir.join(&folder.name));
                    let unit = per_point * folder.scale / FRAME_PIXELS_PER_POINT;
                    Arc::new(pictures::draw(&folder, pixels, unit, cache.as_deref()))
                });
                if scenery.is_some() || drawn.is_some() {
                    log::info!("look for {pixels:?} drawn in {:?}", started.elapsed());
                }
                ScreenAssets { scenery, drawn }
            })
        })
        .collect();
    Preparing {
        folder: folder.cloned(),
        pictures,
        screens,
    }
}

impl Preparing {
    /// The look once drawn; with `wait`, the drawings not done by then are
    /// left drawing, to be handed over later by monitor.
    pub fn finish(
        self,
        wait: Option<Duration>,
    ) -> (Prepared, Vec<(usize, JoinHandle<ScreenAssets>)>) {
        let pictures = self
            .pictures
            .map(|thread| Arc::new(thread.join().unwrap_or_default()));
        if let Some(wait) = wait {
            let deadline = Instant::now() + wait;
            while Instant::now() < deadline && !self.screens.iter().all(JoinHandle::is_finished) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let mut screens = Vec::new();
        let mut later = Vec::new();
        for (output, thread) in self.screens.into_iter().enumerate() {
            if wait.is_none() || thread.is_finished() {
                screens.push(thread.join().unwrap_or_default());
            } else {
                screens.push(ScreenAssets::default());
                later.push((output, thread));
            }
        }
        let prepared = Prepared {
            folder: self.folder,
            pictures,
            screens,
        };
        (prepared, later)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> Theme {
        Theme::Folder(name.into())
    }

    #[test]
    fn cycles_through_the_built_in_looks_then_the_installed_folders() {
        let dir = std::env::temp_dir().join(format!("gpui-greeter-looks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for name in ["zebra", "console", "Not-A-Name", "empty"] {
            std::fs::create_dir_all(dir.join(name)).unwrap();
            if name != "empty" {
                std::fs::write(dir.join(name).join(themes::MANIFEST), "format = 1").unwrap();
            }
        }
        let mut config = Config::default();
        config.themes.dir = dir.clone();
        let all = cycle(&config, &Theme::Paper);
        assert_eq!(
            all,
            [
                Theme::Paper,
                Theme::PaperNight,
                Theme::Frost,
                Theme::Neon,
                folder("console"),
                folder("zebra")
            ]
        );
        config.themes.cycle = vec![folder("zebra"), Theme::Frost, folder("gone"), Theme::Frost];
        assert_eq!(
            cycle(&config, &Theme::Frost),
            [folder("zebra"), Theme::Frost]
        );
        assert_eq!(
            cycle(&config, &Theme::Neon),
            [Theme::Neon, folder("zebra"), Theme::Frost]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lets_go_of_the_farthest_looks_beyond_the_budget() {
        // Each look keeps one picture of 10 by 10 pixels: 400 bytes.
        let prepared = || Prepared {
            pictures: Some(Arc::new(Pictures {
                images: HashMap::from([(
                    0,
                    crate::wallpaper::render_image(image::RgbaImage::new(10, 10)),
                )]),
            })),
            ..Prepared::default()
        };
        let mut looks = Looks::new(
            &Config::default(),
            &[],
            Theme::Paper,
            prepared(),
            SvgAssets::default(),
            false,
        );
        looks.cycle = vec![
            Theme::Paper,
            Theme::PaperNight,
            Theme::Frost,
            Theme::Neon,
            folder("console"),
        ];
        for theme in looks.cycle.clone() {
            looks.ready.insert(theme, prepared());
        }
        let image = looks
            .get(&Theme::Neon)
            .unwrap()
            .images()
            .next()
            .unwrap()
            .clone();
        assert!(looks.holds(&image));
        // Room for two looks besides the one shown: the two either side.
        looks.fit_within(&Theme::Paper, 1000);
        assert!(looks.is_ready(&Theme::Paper));
        assert!(looks.is_ready(&Theme::PaperNight));
        assert!(looks.is_ready(&folder("console")));
        assert!(!looks.is_ready(&Theme::Frost));
        assert!(!looks.is_ready(&Theme::Neon));
        assert_eq!(looks.released, 2);
        assert!(!looks.holds(&image));
    }

    #[test]
    fn steps_round_the_cycle() {
        let looks = Looks::new(
            &Config::default(),
            &[],
            Theme::Paper,
            Prepared::default(),
            SvgAssets::default(),
            false,
        );
        assert_eq!(looks.neighbour(&Theme::Paper, 1), None);
        let looks = Looks {
            cycle: vec![Theme::Paper, Theme::Frost, folder("console")],
            ..looks
        };
        assert_eq!(looks.neighbour(&Theme::Paper, 1), Some(Theme::Frost));
        assert_eq!(looks.neighbour(&Theme::Paper, -1), Some(folder("console")));
        assert_eq!(looks.neighbour(&folder("console"), 1), Some(Theme::Paper));
        assert_eq!(looks.neighbour(&Theme::Neon, 1), Some(Theme::Paper));
    }
}
