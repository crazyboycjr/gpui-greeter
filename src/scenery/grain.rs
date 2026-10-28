//! Paper's grain: the mockup's fractal noise filter (`feTurbulence` at a
//! base frequency of .9 over three octaves, made grey and at most 9% opaque)
//! as a seamless tile, spread over the picture.

use anyhow::{Context as _, Result};
use resvg::tiny_skia::{
    ColorU8, FilterQuality, Paint, Pattern, Pixmap, Rect, SpreadMode, Transform,
};

const BASE_FREQUENCY: f64 = 0.9;
const OCTAVES: u32 = 3;
/// How opaque the grain is where the noise is strongest.
const STRENGTH: f64 = 0.09;
/// The tile's side, in the window's pixels.
const TILE: u32 = 256;

/// Lays the grain over `canvas`, drawn at `scale` window pixels per mockup
/// pixel.
pub fn spread(canvas: &mut Pixmap, scale: f64) -> Result<()> {
    let tile = tile(scale)?;
    let paint = Paint {
        shader: Pattern::new(
            tile.as_ref(),
            SpreadMode::Repeat,
            FilterQuality::Nearest,
            1.0,
            Transform::identity(),
        ),
        ..Paint::default()
    };
    let whole = Rect::from_xywh(0.0, 0.0, canvas.width() as f32, canvas.height() as f32)
        .context("the canvas is empty")?;
    canvas.fill_rect(whole, &paint, Transform::identity(), None);
    Ok(())
}

fn tile(scale: f64) -> Result<Pixmap> {
    let mut tile = Pixmap::new(TILE, TILE).context("cannot make the grain's tile")?;
    let noise = Turbulence::new(f64::from(TILE) / scale);
    for (index, pixel) in tile.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (index as u32 % TILE, index as u32 / TILE);
        let [red, green, blue, alpha] =
            noise.fractal_noise(f64::from(x) / scale, f64::from(y) / scale);
        // feColorMatrix's saturate 0 and feFuncA, in linear light, as filters
        // work by default.
        let grey = to_srgb(0.213 * red + 0.715 * green + 0.072 * blue);
        let alpha = (STRENGTH * alpha * 255.0).round() as u8;
        *pixel = ColorU8::from_rgba(grey, grey, grey, alpha).premultiply();
    }
    Ok(tile)
}

fn to_srgb(linear: f64) -> u8 {
    let encoded = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}

const LATTICE: usize = 0x100;
const PERLIN_N: i32 = 0x1000;

/// The noise of SVG's `feTurbulence`, as its specification gives it with
/// seed 0, made to repeat every `tile` mockup pixels across and down.
struct Turbulence {
    selector: [usize; LATTICE * 2 + 2],
    gradients: [[[f64; 2]; LATTICE * 2 + 2]; 4],
    frequency: f64,
    /// The lattice cells across the tile at the first octave.
    cells: i32,
}

impl Turbulence {
    fn new(tile: f64) -> Self {
        let mut selector = [0; LATTICE * 2 + 2];
        let mut gradients = [[[0.0; 2]; LATTICE * 2 + 2]; 4];
        let mut seed = 1;
        let mut random = || {
            seed = next_seed(seed);
            seed
        };
        for gradient in &mut gradients {
            for (i, vector) in gradient.iter_mut().take(LATTICE).enumerate() {
                selector[i] = i;
                for component in vector.iter_mut() {
                    let lattice = LATTICE as i64;
                    *component = ((random() % (2 * lattice)) - lattice) as f64 / lattice as f64;
                }
                let length = vector[0].hypot(vector[1]);
                vector[0] /= length;
                vector[1] /= length;
            }
        }
        for i in (1..LATTICE).rev() {
            let j = (random() % LATTICE as i64) as usize;
            selector.swap(i, j);
        }
        for i in 0..LATTICE + 2 {
            selector[LATTICE + i] = selector[i];
            for gradient in &mut gradients {
                gradient[LATTICE + i] = gradient[i];
            }
        }
        // The frequency nearest .9 that fits a whole number of cells in the
        // tile, so that its edges meet.
        let fitting = [
            (tile * BASE_FREQUENCY).floor() / tile,
            (tile * BASE_FREQUENCY).ceil() / tile,
        ];
        let frequency = if BASE_FREQUENCY / fitting[0] < fitting[1] / BASE_FREQUENCY {
            fitting[0]
        } else {
            fitting[1]
        };
        Self {
            selector,
            gradients,
            frequency,
            cells: (tile * frequency).round() as i32,
        }
    }

    /// The four channels at `(x, y)`, each between 0 and 1.
    fn fractal_noise(&self, x: f64, y: f64) -> [f64; 4] {
        std::array::from_fn(|channel| {
            let (mut x, mut y) = (x * self.frequency, y * self.frequency);
            let (mut cells, mut sum, mut ratio) = (self.cells, 0.0, 1.0);
            for _ in 0..OCTAVES {
                sum += self.noise(channel, x, y, cells) / ratio;
                (x, y, cells, ratio) = (x * 2.0, y * 2.0, cells * 2, ratio * 2.0);
            }
            ((sum + 1.0) / 2.0).clamp(0.0, 1.0)
        })
    }

    fn noise(&self, channel: usize, x: f64, y: f64, cells: i32) -> f64 {
        let corner = |at: f64| {
            let shifted = at + f64::from(PERLIN_N);
            let low = shifted.floor() as i32;
            // Lattice points past the tile's far edge wrap to its near one.
            let wrap = |cell: i32| {
                let cell = if cell >= PERLIN_N + cells {
                    cell - cells
                } else {
                    cell
                };
                (cell & (LATTICE as i32 - 1)) as usize
            };
            (wrap(low), wrap(low + 1), shifted - shifted.floor())
        };
        let (x0, x1, rx) = corner(x);
        let (y0, y1, ry) = corner(y);
        let (i, j) = (self.selector[x0], self.selector[x1]);
        let gradient = &self.gradients[channel];
        let dot = |cell: usize, dx: f64, dy: f64| dx * gradient[cell][0] + dy * gradient[cell][1];
        let (sx, sy) = (s_curve(rx), s_curve(ry));
        let top = lerp(
            sx,
            dot(self.selector[i + y0], rx, ry),
            dot(self.selector[j + y0], rx - 1.0, ry),
        );
        let bottom = lerp(
            sx,
            dot(self.selector[i + y1], rx, ry - 1.0),
            dot(self.selector[j + y1], rx - 1.0, ry - 1.0),
        );
        lerp(sy, top, bottom)
    }
}

/// The specification's random numbers: Park and Miller's minimal standard.
fn next_seed(seed: i64) -> i64 {
    const M: i64 = 2_147_483_647;
    let next = 16_807 * (seed % 127_773) - 2_836 * (seed / 127_773);
    if next <= 0 { next + M } else { next }
}

fn s_curve(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tile_repeats_seamlessly() {
        let noise = Turbulence::new(100.0);
        for y in [0.0, 13.7, 42.25] {
            let (left, right) = (noise.fractal_noise(0.0, y), noise.fractal_noise(100.0, y));
            for (a, b) in left.iter().zip(right) {
                assert!((a - b).abs() < 1e-9, "{a} against {b}");
            }
        }
    }

    #[test]
    fn the_grain_is_faint() {
        let tile = tile(2.4).unwrap();
        let alphas: Vec<f64> = tile.pixels().iter().map(|p| f64::from(p.alpha())).collect();
        let mean = alphas.iter().sum::<f64>() / alphas.len() as f64;
        assert!(alphas.iter().all(|&a| a <= 23.0));
        assert!((8.0..15.0).contains(&mean), "{mean}");
    }
}
