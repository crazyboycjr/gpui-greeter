/// The mockups' seeded random numbers, mulberry32, so that the scenery they
/// grow from a seed comes out the same here.
pub struct Mulberry32(u32);

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    /// The next number in [0, 1).
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let s = self.0;
        let mut t = (s ^ (s >> 15)).wrapping_mul(1 | s);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// A number between `low` and `high`.
    pub fn between(&mut self, low: f64, high: f64) -> f64 {
        low + self.next() * (high - low)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_mockups_numbers() {
        let mut seven = Mulberry32::new(7);
        assert_eq!(
            [seven.next(), seven.next(), seven.next()],
            [0.011704753153026104, 0.06195825757458806, 0.97690763277933]
        );
        let mut eleven = Mulberry32::new(11);
        assert_eq!(
            [eleven.next(), eleven.next(), eleven.next()],
            [0.5115870486479253, 0.5299464082345366, 0.6081185641232878]
        );
    }
}
