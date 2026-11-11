//! Garbage between two players (`func_800239A0` and helpers; docs "Garbage").
//!
//! A turn's attack points become blobs for the opponent (`func_8003A82C`): they land in the
//! opponent's unassigned pool `incoming[0]`, are spread over the columns into `incoming[1..7]`
//! (`func_80024034`), move to `pending` once the sender's turn is over (sender in player state
//! 11 or 0), and drop in at the end of the receiver's next turn (receiver in state 4).

use crate::field::{st, Field, COLS};
use crate::game::ps;
use crate::rng::BiosRand;
use crate::tables::{GARBAGE_DIVISOR, GARBAGE_SPREAD};

/// One player's garbage counters, indexed by column (1–6; 0 is the unassigned pool).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Garbage {
    /// `D_801003B8[p]`: sent to this player but not committed. `[0]` is the pool.
    pub incoming: [i16; COLS - 1],
    /// `D_801006A8[p]`: committed, drops at the end of this player's next turn.
    pub pending: [i16; COLS - 1],
    /// `D_80100640[p]`: blobs dropped by the last drop.
    pub dropped: u16,
    /// Grey/special blobs dropped into the field, ever (each drop also adds to the field's
    /// grey count `D_801005FA[p]`).
    pub dropped_total: u32,
    /// `D_801005EE` / `D_801005F4`: blobs this player sent during the current turn (the
    /// all-clear bonus resends them); cleared when the player is in state 4.
    pub sent_this_turn: u16,
}

/// `func_8003AE70`: attack points per blob after `frames` of play (`D_80066AD0`).
pub fn divisor(frames: u32, mode14: bool) -> i16 {
    let i = (frames / 900) as usize + if mode14 { 2 } else { 0 };
    GARBAGE_DIVISOR[i.min(32)]
}

/// `func_8003A82C`: the blobs a turn's `points` send (points / divisor + 1).
pub fn blobs_for(points: u16, frames: u32, mode14: bool) -> u16 {
    ((points as i16) / divisor(frames, mode14)).wrapping_add(1) as u16
}

impl Garbage {
    /// Total committed plus uncommitted blobs (for the warning display).
    pub fn total(&self) -> i32 {
        self.incoming.iter().chain(&self.pending[1..]).map(|&n| i32::from(n)).sum()
    }

    /// `func_80024034`: spread the pool over the columns. Each column below the fair share
    /// (everything / 6) gets `share − pending` (overwriting its uncommitted count); the
    /// remainder is placed by a random mask (`func_800241D4`).
    pub fn spread(&mut self, rng: &mut BiosRand) {
        if self.incoming[0] == 0 {
            return;
        }
        let mut total = self.incoming[0];
        for c in 1..COLS - 1 {
            total = total.wrapping_add(self.pending[c]).wrapping_add(self.incoming[c]);
        }
        let share = total / 6;
        for c in 1..COLS - 1 {
            if self.pending[c] < share {
                self.incoming[c] = share - self.pending[c];
            }
        }
        self.spread_remainder((total % 6) as u8, rng);
        self.incoming[0] = 0;
    }

    /// `func_800241D4`.
    fn spread_remainder(&mut self, n: u8, rng: &mut BiosRand) {
        if n == 0 {
            return;
        }
        let rec = &GARBAGE_SPREAD[n as usize];
        let pick = rng.next() % u32::from(rec.count);
        let mask = rec.masks[pick as usize];
        for b in 0..6 {
            if mask >> b & 1 != 0 {
                self.incoming[b + 1] += 1;
                self.incoming[0] -= 1;
            }
        }
    }

    /// The merge in `func_800239A0`: uncommitted column counts become pending.
    pub fn commit(&mut self) {
        for c in 1..COLS - 1 {
            self.pending[c] = self.pending[c].wrapping_add(self.incoming[c]);
            self.incoming[c] = 0;
        }
    }

    /// The drop in `func_800239A0`: per column, up to 5 blobs fall in from rows 5, 4, … 1.
    /// Before placing the k-th blob the cell at row 6 + k is checked; if it is settled the rest
    /// of that column goes back to the pool `incoming[0]` (`0x80023B48`), to be spread again.
    /// Blobs are colour 6 in `special_column` (1–6) or everywhere when `all_special` (mode 11),
    /// else grey. Returns the number dropped.
    pub fn drop_into(&mut self, field: &mut Field, special_column: Option<usize>, all_special: bool) -> u16 {
        self.dropped = 0;
        for c in 1..COLS - 1 {
            let color = if all_special || special_column == Some(c) { 6 } else { 0 };
            let want = self.pending[c];
            let mut placed: i16 = 0;
            while placed < want && placed < 5 {
                let k = placed as usize;
                if field.cell(c, 6 + k).state == st::SETTLED {
                    self.incoming[0] = self.incoming[0].wrapping_add(want - placed);
                    self.pending[c] = placed;
                    break;
                }
                field.drop_blob(c, 5 - k, color);
                placed += 1;
            }
            self.pending[c] -= placed;
            self.dropped += placed as u16;
            self.dropped_total += placed as u32;
            field.grey = field.grey.wrapping_add(placed);
        }
        self.dropped
    }

    /// One player's part of `func_800239A0`, at the start of its cell update: spread, commit if
    /// the opponent's turn is over (`opponent_state` 11 or 0), drop if this player is in state 4.
    pub fn step(
        &mut self,
        field: &mut Field,
        rng: &mut BiosRand,
        own_state: u8,
        opponent_state: u8,
        special_column: Option<usize>,
        all_special: bool,
    ) -> u16 {
        self.spread(rng);
        if opponent_state == ps::SPAWN || opponent_state == ps::CONTROL {
            self.commit();
        }
        if own_state == ps::NO_MATCH {
            self.drop_into(field, special_column, all_special)
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divisor_falls_every_15_seconds_and_caps() {
        assert_eq!(divisor(0, false), 70);
        assert_eq!(divisor(6 * 900, false), 60);
        assert_eq!(divisor(6 * 900, true), 47);
        assert_eq!(divisor(10_000_000, false), 1);
        assert_eq!(blobs_for(140, 0, false), 3);
        assert_eq!(blobs_for(69, 0, false), 1);
    }

    #[test]
    fn spread_is_even_with_a_random_remainder() {
        let mut g = Garbage::default();
        g.incoming[0] = 14;
        let mut rng = BiosRand::new(1);
        g.spread(&mut rng);
        assert_eq!(g.incoming[0], 0);
        let cols = &g.incoming[1..];
        assert_eq!(cols.iter().sum::<i16>(), 14);
        assert!(cols.iter().all(|&n| n == 2 || n == 3));
    }

    #[test]
    fn spread_tops_up_to_the_share() {
        let mut g = Garbage::default();
        g.pending = [0, 4, 0, 0, 0, 0, 2];
        g.incoming[0] = 6;
        g.spread(&mut BiosRand::new(1));
        // total 12 → share 2: column 1 (4 pending) gets nothing, column 6 (2) nothing.
        assert_eq!(g.incoming, [0, 0, 2, 2, 2, 2, 0]);
    }

    #[test]
    fn drop_fills_from_row_5_and_stops_on_a_full_column() {
        let mut f = Field::new();
        for r in 7..18 {
            f.put(2, r, 1);
        }
        let mut g = Garbage { pending: [0, 7, 3, 0, 0, 0, 0], ..Default::default() };
        let n = g.drop_into(&mut f, Some(1), false);
        // Column 1: 5 now (rows 5…1, special), 2 stay pending. Column 2: row 6 is free, row 7
        // is settled, so 1 drops and 2 go back to the pool.
        assert_eq!(n, 6);
        assert_eq!(g.pending, [0, 2, 0, 0, 0, 0, 0]);
        assert_eq!(g.incoming, [2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(f.cell(1, 5).color, 6);
        assert_eq!(f.cell(1, 1).state, st::FALLING);
        assert_eq!(f.cell(1, 0).state, st::EMPTY);
        assert_eq!(f.cell(2, 5).color, 0);
        assert_eq!(f.cell(2, 5).state, st::FALLING);
        assert_eq!(f.cell(2, 4).state, st::EMPTY);
    }
}
