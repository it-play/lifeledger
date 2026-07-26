use sha2::{Digest, Sha256};

pub trait MarketEntropy: Send + Sync {
    /// Returns one counter-derived word. Equal inputs must always return equal output.
    fn sample_u64(&self, world_seed: u64, game_day: u32, stream: u32) -> u64;
}

struct Sha256CounterEntropy;

impl MarketEntropy for Sha256CounterEntropy {
    fn sample_u64(&self, world_seed: u64, game_day: u32, stream: u32) -> u64 {
        let mut digest = Sha256::new();
        digest.update(b"lifeledger.market.entropy.v1\0");
        digest.update(world_seed.to_be_bytes());
        digest.update(game_day.to_be_bytes());
        digest.update(stream.to_be_bytes());
        let bytes = digest.finalize();
        let mut word = [0_u8; 8];
        word.copy_from_slice(&bytes[..8]);
        u64::from_be_bytes(word)
    }
}

pub fn create_sha256_market_entropy() -> impl MarketEntropy {
    Sha256CounterEntropy
}
