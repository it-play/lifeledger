//! Store contracts. The MySQL implementation does not know this file, and callers do
//! not know the implementation.

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

use crate::auth::{OAuthIdentity, ProviderKind};
use crate::character::Character;

/// The durable state of one save, mirroring what the database holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveState {
    /// Used to filter SSE ticks down to the subscriber's own save (§4.5).
    pub save_id: u64,
    pub game_day: u32,
    pub cash_krw: i64,
    pub debt_krw: i64,
    /// `None` until a character has been created.
    pub character: Option<Character>,
}

/// A signed-in account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountUser {
    pub id: u64,
    pub provider: ProviderKind,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

/// Accounts and sessions.
#[async_trait]
pub trait UserStore: Send + Sync + 'static {
    /// Records an OAuth-verified user: creates the account, or refreshes it if known.
    async fn upsert(&self, identity: &OAuthIdentity) -> Result<AccountUser>;

    /// Opens a session, storing only the token hash.
    async fn open_session(&self, user_id: u64, token_hash: &str, ttl: Duration) -> Result<()>;

    /// Finds a user by token hash. An expired session counts as absent.
    async fn find_by_session(&self, token_hash: &str) -> Result<Option<AccountUser>>;

    /// Closes one session (logout).
    async fn close_session(&self, token_hash: &str) -> Result<()>;
}

/// Save reads and writes. Every access is scoped to an account (§4.5).
#[async_trait]
pub trait SaveStore: Send + Sync + 'static {
    /// Reads the account's save, creating it in its initial state if absent.
    async fn load(&self, user_id: u64) -> Result<SaveState>;

    /// Commits a character and resets the save to those starting conditions.
    async fn start_game(&self, user_id: u64, character: &Character) -> Result<SaveState>;

    /// Advances the game day by `days`. There is no rewind (§2).
    async fn advance(&self, user_id: u64, days: u32) -> Result<SaveState>;
}
