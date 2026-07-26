mod pipeline;
mod types;

pub use pipeline::create_daily_pipeline;
pub use types::{
    CommittedGameState, DailyAdvanceResult, DailyCommandAdvanceResult, DailyPipeline,
    DailyStartGameResult,
};
