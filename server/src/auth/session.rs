//! Session cookies and the auth extractor (§4.5).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;

use crate::state::AppState;
use crate::store::AccountUser;

/// Holds the login session.
pub const SESSION_COOKIE: &str = "lifeledger_session";
/// Holds `state` and `code_verifier` for the duration of an OAuth round trip.
pub const TRANSACTION_COOKIE: &str = "lifeledger_oauth";

/// Matches the 30-day lifetime of a DataGSM refresh token.
pub const SESSION_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 30);
/// Long enough to sit on a provider's login page; matches DataGSM's 10-minute auth token.
pub const TRANSACTION_TTL: Duration = Duration::from_secs(60 * 10);

/// An authenticated user. A route that asks for this cannot be reached anonymously.
#[derive(Debug, Clone)]
pub struct AuthUser(pub AccountUser);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        const UNAUTHORIZED: (StatusCode, &str) = (StatusCode::UNAUTHORIZED, "로그인이 필요합니다");

        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).ok_or(UNAUTHORIZED)?.value();

        let user = state
            .authenticate(token)
            .await
            .map_err(|error| {
                tracing::error!(error = ?error, "세션 조회에 실패했습니다");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "세션을 확인하지 못했습니다",
                )
            })?
            .ok_or(UNAUTHORIZED)?;

        Ok(Self(user))
    }
}
