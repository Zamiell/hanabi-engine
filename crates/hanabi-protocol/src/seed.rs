//! Hanabi Live's canonical No Variant seed shuffle, independent of host RNGs.
//! Sources: hanabi-live/server/src/{misc,game_deck}.go and Go 1.24 math/rand.
//! This is compatibility code: replacing it with a different shuffle changes deals.

mod cooked;

use crate::hanabi_live::HanabiLiveCard;

pub(crate) fn deck_from_seed(seed: &str, players: usize) -> Result<Vec<HanabiLiveCard>, String> {
    let (count, tail) = seed
        .strip_prefix('p')
        .and_then(|s| s.split_once('v'))
        .ok_or("expected a canonical Hanabi Live seed such as p4v0s1")?;
    let (variant, suffix) = tail
        .split_once('s')
        .ok_or("expected a canonical Hanabi Live seed such as p4v0s1")?;
    if !matches!(count, "2" | "3" | "4" | "5") || count.parse::<usize>().ok() != Some(players) {
        return Err("seed player count must match the replay's 2–5 players".to_owned());
    }
    if variant != "0" {
        return Err("seed generation only supports variant 0 (No Variant)".to_owned());
    }
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return Err("canonical seed suffix must be a nonempty decimal number".to_owned());
    }
    let mut deck = hanabi_core::standard_deck()
        .into_iter()
        .map(|card| HanabiLiveCard {
            suit_index: u8::try_from(card.suit.index()).expect("five suits"),
            rank: card.rank.number(),
        })
        .collect::<Vec<_>>();
    let mut rng = GoRandom::new(i64::from_ne_bytes(
        crc64_ecma(seed.as_bytes()).to_ne_bytes(),
    ));
    // Hanabi Live uses ascending swaps, including Intn(1), which consumes RNG.
    for i in 0..deck.len() {
        let j = usize::try_from(rng.int31n(u32::try_from(i + 1).expect("50 cards")))
            .expect("bounded index");
        deck.swap(i, j);
    }
    Ok(deck)
}

fn crc64_ecma(bytes: &[u8]) -> u64 {
    let mut crc = !0_u64;
    for byte in bytes {
        crc ^= u64::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1)
                ^ if crc & 1 == 0 {
                    0
                } else {
                    0xc96c_5795_d787_0f42
                };
        }
    }
    !crc
}

// Port of Go's legacy Mitchell/Reeds generator and Int31n; Go's BSD license
// is retained in licenses/GO-LICENSE alongside the copied seed constants.
struct GoRandom {
    state: [u64; 607],
    tap: usize,
    feed: usize,
}

impl GoRandom {
    fn new(seed: i64) -> Self {
        let mut x = seed.rem_euclid(2_147_483_647);
        if x == 0 {
            x = 89_482_311;
        }
        let mut next_seed = || {
            x = x * 48_271 % 2_147_483_647;
            u64::try_from(x).expect("positive seed step")
        };
        for _ in 0..20 {
            next_seed();
        }
        let state = std::array::from_fn(|i| {
            (next_seed() << 40)
                ^ (next_seed() << 20)
                ^ next_seed()
                ^ u64::from_ne_bytes(cooked::COOKED[i].to_ne_bytes())
        });
        Self {
            state,
            tap: 0,
            feed: 607 - 273,
        }
    }

    fn int31(&mut self) -> u32 {
        self.tap = (self.tap + 606) % 607;
        self.feed = (self.feed + 606) % 607;
        let value = self.state[self.feed].wrapping_add(self.state[self.tap]);
        self.state[self.feed] = value;
        u32::try_from((value & 0x7fff_ffff_ffff_ffff) >> 32).expect("31 bits")
    }

    fn int31n(&mut self, n: u32) -> u32 {
        if n.is_power_of_two() {
            return self.int31() & (n - 1);
        }
        let max = (1_u32 << 31) - 1 - (1_u32 << 31) % n;
        let mut value = self.int31();
        while value > max {
            value = self.int31();
        }
        value % n
    }
}
