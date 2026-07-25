use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;
use tokio::sync::broadcast;
use utoipa::ToSchema;

use crate::auth::{Providers, token_hash_of};
use crate::character::{Character, net_worth_krw};
use crate::store::{AccountUser, SaveState, SaveStore, UserStore};

/// Game start date. Moves into world seed configuration later.
const START_DATE: &str = "2026-01-01";

/// The game state sent to a client.
///
/// Carries the start date plus elapsed days rather than a formatted date: the
/// calculation is deterministic, so letting the client do it costs no authority.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GameSnapshot {
    pub game_day: u32,
    pub start_date: &'static str,
    pub cash_krw: i64,
    pub debt_krw: i64,
    pub net_worth_krw: i64,
    /// `None` until a character exists; the client routes to creation.
    pub character_name: Option<String>,
}

/// Carries which save the tick belongs to so subscribers can filter to their own (§4.5).
#[derive(Debug, Clone)]
pub struct Tick {
    pub save_id: u64,
    pub snapshot: GameSnapshot,
}

/// The server owns day advancement (§4.2); a client only asks how far.
///
/// State itself lives in the database (§4.4). This layer composes store calls and
/// broadcasts only what has been committed.
pub struct AppState {
    saves: Arc<dyn SaveStore>,
    users: Arc<dyn UserStore>,
    pub providers: Providers,
    ticks: broadcast::Sender<Tick>,
}

impl AppState {
    pub fn new(
        saves: Arc<dyn SaveStore>,
        users: Arc<dyn UserStore>,
        providers: Providers,
    ) -> Arc<Self> {
        let (ticks, _) = broadcast::channel(256);
        Arc::new(Self {
            saves,
            users,
            providers,
            ticks,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Tick> {
        self.ticks.subscribe()
    }

    /// Resolves a session cookie token to a user. `None` when absent or expired.
    pub async fn authenticate(&self, token: &str) -> Result<Option<AccountUser>> {
        self.users.find_by_session(&token_hash_of(token)).await
    }

    pub fn users(&self) -> &Arc<dyn UserStore> {
        &self.users
    }

    /// Opens a session and returns the raw token to put in the cookie.
    pub async fn open_session(&self, user_id: u64, ttl: Duration) -> Result<String> {
        let token = crate::auth::random_token()?;
        self.users
            .open_session(user_id, &token_hash_of(&token), ttl)
            .await?;

        Ok(token)
    }

    pub async fn close_session(&self, token: &str) -> Result<()> {
        self.users.close_session(&token_hash_of(token)).await
    }

    pub async fn snapshot(&self, user_id: u64) -> Result<GameSnapshot> {
        Ok(to_snapshot(&self.saves.load(user_id).await?))
    }

    /// Current state for a subscriber that just connected, with its save id.
    pub async fn current(&self, user_id: u64) -> Result<(u64, GameSnapshot)> {
        let state = self.saves.load(user_id).await?;

        Ok((state.save_id, to_snapshot(&state)))
    }

    /// Advances the game day and broadcasts the result.
    ///
    /// Emits one final state for now. Once daily settlement lands (M1) each day will
    /// actually change values, and this splits into per-day ticks.
    pub async fn advance(&self, user_id: u64, days: u32) -> Result<GameSnapshot> {
        let state = self.saves.advance(user_id, days).await?;

        Ok(self.broadcast(&state))
    }

    /// Commits a character and resets the game to day 0.
    pub async fn start_game(&self, user_id: u64, character: Character) -> Result<GameSnapshot> {
        let state = self.saves.start_game(user_id, &character).await?;

        Ok(self.broadcast(&state))
    }

    fn broadcast(&self, state: &SaveState) -> GameSnapshot {
        let snapshot = to_snapshot(state);
        // Sending with no subscribers errors, which is a normal state here
        let _ = self.ticks.send(Tick {
            save_id: state.save_id,
            snapshot: snapshot.clone(),
        });

        snapshot
    }
}

fn to_snapshot(state: &SaveState) -> GameSnapshot {
    GameSnapshot {
        game_day: state.game_day,
        start_date: START_DATE,
        cash_krw: state.cash_krw,
        debt_krw: state.debt_krw,
        net_worth_krw: net_worth_krw(state.cash_krw, state.debt_krw),
        character_name: state.character.as_ref().map(|c| c.name.clone()),
    }
}
