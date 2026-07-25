//! Login routes (§4.5).
//!
//! Per-round-trip state (`state`, `code_verifier`) rides in a short-lived cookie rather
//! than server memory, so a restart does not break a login in flight.

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::{
    AuthUser, ProviderKind, SESSION_COOKIE, SESSION_TTL, TRANSACTION_COOKIE, TRANSACTION_TTL,
    code_challenge_of, random_token,
};
use crate::error::AppError;
use crate::state::AppState;

/// Where the client lands once login finishes.
const AFTER_LOGIN_PATH: &str = "/";

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/auth/providers", get(providers))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/{provider}/start", get(start))
        .route("/api/auth/{provider}/callback", get(callback))
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    /// Path segment of `/api/auth/{id}/start`.
    id: &'static str,
    label: &'static str,
}

/// The enabled login providers, which is what the client draws buttons from.
#[utoipa::path(
    get,
    path = "/api/auth/providers",
    responses((status = 200, description = "사용할 수 있는 로그인 제공자", body = [ProviderSummary]))
)]
async fn providers(State(state): State<Arc<AppState>>) -> Json<Vec<ProviderSummary>> {
    Json(
        state
            .providers
            .enabled()
            .into_iter()
            .map(|kind| ProviderSummary {
                id: kind.as_str(),
                label: kind.label(),
            })
            .collect(),
    )
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MeResponse {
    provider: ProviderKind,
    email: Option<String>,
    display_name: Option<String>,
}

/// The signed-in account. Anonymous callers get 401 so the client can route to login.
#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "로그인한 계정", body = MeResponse),
        (status = 401, description = "로그인하지 않음"),
    )
)]
async fn me(AuthUser(user): AuthUser) -> Json<MeResponse> {
    Json(MeResponse {
        provider: user.provider,
        email: user.email,
        display_name: user.display_name,
    })
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses((status = 204, description = "로그아웃됨"))
)]
async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> Result<Response, AppError> {
    if let Some(cookie) = jar.get(SESSION_COOKIE) {
        state.close_session(cookie.value()).await?;
    }

    let jar = jar.remove(Cookie::from(SESSION_COOKIE));

    Ok((jar, axum::http::StatusCode::NO_CONTENT).into_response())
}

/// What the round-trip cookie carries.
#[derive(Serialize, Deserialize)]
struct Transaction {
    provider: String,
    state: String,
    verifier: String,
}

/// Redirects to the provider's login page.
async fn start(
    State(app): State<Arc<AppState>>,
    Path(provider): Path<String>,
    jar: CookieJar,
) -> Result<Response, AppError> {
    let (kind, provider) = resolve(&app, &provider)?;

    let state = random_token()?;
    let verifier = random_token()?;
    let redirect_uri = app.providers.redirect_uri(kind);

    let authorize_url =
        provider.authorize_url(&state, &code_challenge_of(&verifier), &redirect_uri);

    let transaction = serde_json::to_string(&Transaction {
        provider: kind.as_str().to_owned(),
        state,
        verifier,
    })?;

    Ok((
        jar.add(transient_cookie(
            TRANSACTION_COOKIE,
            transaction,
            TRANSACTION_TTL,
        )),
        Redirect::to(&authorize_url),
    )
        .into_response())
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Where the provider returns with an authorization code.
async fn callback(
    State(app): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(query): Query<CallbackQuery>,
    jar: CookieJar,
) -> Result<Response, AppError> {
    let (kind, provider) = resolve(&app, &provider)?;

    if let Some(error) = query.error {
        // Usually just a declined consent screen, so not an error
        tracing::info!(provider = kind.as_str(), error, "login cancelled");
        return Ok(finish(jar, "cancelled"));
    }

    let (Some(code), Some(returned_state)) = (query.code, query.state) else {
        return Ok(finish(jar, "invalid_response"));
    };

    let Some(transaction) = jar
        .get(TRANSACTION_COOKIE)
        .and_then(|cookie| serde_json::from_str::<Transaction>(cookie.value()).ok())
    else {
        // Cookie expired, or the login began in a different browser
        return Ok(finish(jar, "expired"));
    };

    // CSRF defence: confirm this browser started the login (RFC 6749 §10.12)
    if transaction.provider != kind.as_str()
        || !constant_time_eq(&transaction.state, &returned_state)
    {
        tracing::warn!(provider = kind.as_str(), "state mismatch");
        return Ok(finish(jar, "state_mismatch"));
    }

    let identity = provider
        .identify(
            &code,
            &transaction.verifier,
            &app.providers.redirect_uri(kind),
        )
        .await?;

    let user = app.users().upsert(&identity).await?;
    let token = app.open_session(user.id, SESSION_TTL).await?;

    tracing::info!(provider = kind.as_str(), user_id = user.id, "signed in");

    let jar = jar
        .remove(Cookie::from(TRANSACTION_COOKIE))
        .add(transient_cookie(SESSION_COOKIE, token, SESSION_TTL));

    Ok((jar, Redirect::to(AFTER_LOGIN_PATH)).into_response())
}

/// Returns to the client even on failure, passing the reason as a query parameter.
fn finish(jar: CookieJar, reason: &str) -> Response {
    let jar = jar.remove(Cookie::from(TRANSACTION_COOKIE));

    (
        jar,
        Redirect::to(&format!("{AFTER_LOGIN_PATH}?login_error={reason}")),
    )
        .into_response()
}

fn resolve<'a>(
    app: &'a AppState,
    raw: &str,
) -> Result<(ProviderKind, &'a Arc<dyn crate::auth::OAuthProvider>), AppError> {
    let kind =
        ProviderKind::parse(raw).ok_or_else(|| anyhow::anyhow!("unknown login provider: {raw}"))?;
    let provider = app
        .providers
        .get(kind)
        .ok_or_else(|| anyhow::anyhow!("{raw} login is not configured"))?;

    Ok((kind, provider))
}

/// An httpOnly, Secure, SameSite=Lax cookie.
///
/// Lax is required: under Strict the cookie would not ride the top-level navigation
/// back from the provider.
fn transient_cookie(
    name: &'static str,
    value: String,
    ttl: std::time::Duration,
) -> Cookie<'static> {
    Cookie::build((name, value))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(ttl.as_secs().cast_signed()))
        .build()
}

/// Compares the whole value so neither length nor content leaks through timing.
fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.bytes()
        .zip(right.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_state_is_compared_on_callback {
        use super::*;

        #[test]
        fn given_identical_states_when_compared_then_they_match() {
            assert!(constant_time_eq("abc123", "abc123"));
        }

        #[test]
        fn given_states_differing_in_one_byte_when_compared_then_they_do_not_match() {
            assert!(!constant_time_eq("abc123", "abc124"));
        }

        #[test]
        fn given_states_of_different_length_when_compared_then_they_do_not_match() {
            assert!(!constant_time_eq("abc", "abc123"));
        }
    }
}
