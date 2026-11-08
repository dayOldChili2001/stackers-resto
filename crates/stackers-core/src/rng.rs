/// The PS1 BIOS `rand()` (A-function 0x2F), which the game uses for piece colours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BiosRand {
    pub seed: u32,
}

impl BiosRand {
    pub fn new(seed: u32) -> Self {
        Self { seed }
    }

    pub fn next(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        (self.seed >> 16) & 0x7FFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_c_rand_sequence() {
        let mut r = BiosRand::new(1);
        let got: Vec<u32> = (0..5).map(|_| r.next()).collect();
        assert_eq!(got, [16838, 5758, 10113, 17515, 31051]);
    }
}
