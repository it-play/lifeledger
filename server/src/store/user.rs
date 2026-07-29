//! MySQL implementation of `UserStore`.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use sqlx::MySqlPool;
use time::OffsetDateTime;

use super::types::{AccountUser, UserStore};
use crate::auth::{OAuthIdentity, ProviderKind};

pub struct MySqlUserStore {
    pool: MySqlPool,
}

pub const fn create_mysql_user_store(pool: MySqlPool) -> MySqlUserStore {
    MySqlUserStore { pool }
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: u64,
    provider: String,
    email: Option<String>,
    display_name: Option<String>,
}

impl UserRow {
    fn into_account(self) -> Result<AccountUser> {
        let provider = ProviderKind::parse(&self.provider)
            .with_context(|| format!("unknown provider stored: {}", self.provider))?;

        Ok(AccountUser {
            id: self.id,
            provider,
            email: self.email,
            display_name: self.display_name,
        })
    }
}

#[async_trait]
impl UserStore for MySqlUserStore {
    async fn upsert(&self, identity: &OAuthIdentity) -> Result<AccountUser> {
        // Email and name can change upstream, so refresh them on every login. Identity is
        // (provider, provider_user_id), so refreshing them never splits the account
        sqlx::query(
            "INSERT INTO user (provider, provider_user_id, email, display_name)
             VALUES (?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE
                 email = VALUES(email),
                 display_name = VALUES(display_name)",
        )
        .bind(identity.provider.as_str())
        .bind(&identity.subject)
        .bind(&identity.email)
        .bind(&identity.display_name)
        .execute(&self.pool)
        .await?;

        // last_insert_id is unset when the statement took the UPDATE branch, so re-read
        let row: Option<UserRow> = sqlx::query_as(
            "SELECT id, provider, email, display_name
             FROM user WHERE provider = ? AND provider_user_id = ?",
        )
        .bind(identity.provider.as_str())
        .bind(&identity.subject)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            bail!("the account just written could not be found");
        };

        row.into_account()
    }

    async fn open_session(&self, user_id: u64, token_hash: &str, ttl: Duration) -> Result<()> {
        let expires_at = OffsetDateTime::now_utc() + ttl;

        sqlx::query("INSERT INTO session (user_id, token_hash, expires_at) VALUES (?, ?, ?)")
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn find_by_session(&self, token_hash: &str) -> Result<Option<AccountUser>> {
        let row: Option<UserRow> = sqlx::query_as(
            "SELECT u.id, u.provider, u.email, u.display_name
             FROM session s
             JOIN user u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > UTC_TIMESTAMP(3)",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        row.map(UserRow::into_account).transpose()
    }

    async fn close_session(&self, token_hash: &str) -> Result<()> {
        sqlx::query("DELETE FROM session WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn delete_account(&self, user_id: u64) -> Result<bool> {
        let result = sqlx::query("DELETE FROM user WHERE id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() == 1)
    }
}
