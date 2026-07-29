use anyhow::Result;
use async_trait::async_trait;

use crate::market::{MarketDay, MarketWorld};
use crate::store::{
    AdvanceCommandReceipt, GameCommandRejection, ManualAdvanceCommand, SaveState, StartGameCommand,
    StartGameReceipt,
};

/// The player state and immutable market close committed for the same game day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedGameState {
    pub save: SaveState,
    pub world: MarketWorld,
    pub market: MarketDay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DailyAdvanceResult {
    Advanced(Box<CommittedGameState>),
    CharacterRequired,
    TargetReached(Box<CommittedGameState>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DailyStartGameResult {
    Applied {
        state: Box<CommittedGameState>,
        receipt: StartGameReceipt,
    },
    Replayed {
        state: Box<CommittedGameState>,
        receipt: StartGameReceipt,
    },
    Rejected(GameCommandRejection),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DailyCommandAdvanceResult {
    Advanced {
        state: Box<CommittedGameState>,
        receipt: Option<AdvanceCommandReceipt>,
    },
    Replayed {
        state: Box<CommittedGameState>,
        receipt: AdvanceCommandReceipt,
    },
    Rejected(GameCommandRejection),
}

/// Composes the shared market layer with account-owned player transactions (§4.2).
#[async_trait]
pub trait DailyPipeline: Send + Sync + 'static {
    async fn load(&self, user_id: u64) -> Result<CommittedGameState>;

    async fn start_game(
        &self,
        user_id: u64,
        command: &StartGameCommand,
    ) -> Result<DailyStartGameResult>;

    /// Runtime-owned automatic tick. It deliberately has no durable command identity.
    async fn advance_one_day(&self, user_id: u64) -> Result<DailyAdvanceResult>;

    /// Commits or replays the next missing step of a durable manual command.
    async fn advance_command_step(
        &self,
        user_id: u64,
        command: &ManualAdvanceCommand,
    ) -> Result<DailyCommandAdvanceResult>;
}
