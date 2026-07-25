use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::{Config, SwaggerUi};

mod auth;

use crate::auth::AuthUser;
use crate::character;
use crate::error::AppError;
use crate::state::{AppState, GameSnapshot};

/// Reconnect delay the server suggests; the client uses it as its backoff baseline.
const RETRY_HINT: Duration = Duration::from_secs(1);
/// Keep-alive comment interval, so proxies do not drop an idle connection.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// API docs, mounted under `/api` because that is the prefix nginx forwards.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "LifeLedger API",
        description = "모의 자산관리 인생 시뮬레이션 서버"
    ),
    paths(
        health,
        presets,
        create_character,
        snapshot,
        advance,
        auth::providers,
        auth::me,
        auth::logout,
    ),
    components(schemas(
        GameSnapshot,
        Health,
        AdvanceRequest,
        ValidationFailure,
        character::CharacterDraft,
        character::Preset,
        character::ValidationError,
        character::Gender,
        character::MilitaryStatus,
        character::Education,
        character::Region,
        character::FamilyBackground,
        character::Health,
        auth::ProviderSummary,
        auth::MeResponse,
        crate::auth::ProviderKind,
    ))
)]
pub struct ApiDoc;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/presets", get(presets))
        .route("/api/characters", post(create_character))
        .route("/api/state", get(snapshot))
        .route("/api/advance", post(advance))
        .route("/api/stream", get(stream))
        .merge(auth::router())
        .with_state(state)
        .merge(
            SwaggerUi::new("/api/docs")
                .url("/api/docs/openapi.json", ApiDoc::openapi())
                // Relative, so the spec URL follows the prefix nginx adds; an absolute path
                // would skip `/lifeledger` and resolve against the domain root
                .config(Config::from("./openapi.json")),
        )
}

#[utoipa::path(
    get,
    path = "/api/presets",
    responses((status = 200, description = "선택 가능한 시작 프리셋", body = [character::Preset]))
)]
async fn presets() -> Json<&'static [character::Preset]> {
    Json(character::presets())
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct ValidationFailure {
    errors: Vec<character::ValidationError>,
}

/// Character creation. The domain validates (§3.5); this only picks a status code.
#[utoipa::path(
    post,
    path = "/api/characters",
    request_body = character::CharacterDraft,
    responses(
        (status = 200, description = "생성된 캐릭터의 시작 스냅샷", body = GameSnapshot),
        (status = 422, description = "시작 조건이 서로 모순됨", body = ValidationFailure),
        (status = 500, description = "저장 실패"),
    )
)]
async fn create_character(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Json(draft): Json<character::CharacterDraft>,
) -> Result<Json<GameSnapshot>, CreateCharacterError> {
    let character = character::create_character(draft).map_err(CreateCharacterError::Invalid)?;

    Ok(Json(state.start_game(user.id, character).await?))
}

/// 422 and 500 have different causes, so they have different response shapes.
enum CreateCharacterError {
    Invalid(Vec<character::ValidationError>),
    Internal(AppError),
}

impl From<anyhow::Error> for CreateCharacterError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(AppError::from(error))
    }
}

impl axum::response::IntoResponse for CreateCharacterError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Invalid(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ValidationFailure { errors }),
            )
                .into_response(),
            Self::Internal(error) => error.into_response(),
        }
    }
}

#[derive(Serialize, ToSchema)]
struct Health {
    status: &'static str,
    version: &'static str,
}

#[utoipa::path(
    get,
    path = "/api/health",
    responses((status = 200, description = "서버가 살아 있음", body = Health))
)]
async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[utoipa::path(
    get,
    path = "/api/state",
    responses(
        (status = 200, description = "현재 게임 상태", body = GameSnapshot),
        (status = 500, description = "조회 실패"),
    )
)]
async fn snapshot(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<GameSnapshot>, AppError> {
    Ok(Json(state.snapshot(user.id).await?))
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
struct AdvanceRequest {
    days: u32,
}

#[utoipa::path(
    post,
    path = "/api/advance",
    request_body = AdvanceRequest,
    responses(
        (status = 200, description = "전진 후 스냅샷", body = GameSnapshot),
        (status = 500, description = "전진 실패"),
    )
)]
async fn advance(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<GameSnapshot>, AppError> {
    // Capped so one request cannot occupy the server for long
    let days = request.days.clamp(1, 3650);

    Ok(Json(state.advance(user.id, days).await?))
}

/// Stream of game-day advances.
///
/// Events are named `tick` and carry the game day as `id`. A reconnecting client sends
/// its last day back as `Last-Event-ID`, leaving room to replay from there later.
async fn stream(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let (save_id, current) = state.current(user.id).await?;

    // Only this save's ticks; without the filter another player's advance would arrive (§4.5)
    let updates = BroadcastStream::new(state.subscribe())
        .filter_map(|result| result.ok())
        .filter(move |tick| tick.save_id == save_id)
        .map(|tick| Ok(to_event(&tick.snapshot)));

    // Send current state once on connect so the client can draw without a separate fetch
    let initial = tokio_stream::once(Ok(to_event(&current).retry(RETRY_HINT)));

    Ok(Sse::new(initial.chain(updates)).keep_alive(KeepAlive::new().interval(KEEP_ALIVE)))
}

fn to_event(snapshot: &GameSnapshot) -> Event {
    Event::default()
        .event("tick")
        .id(snapshot.game_day.to_string())
        .json_data(snapshot)
        .unwrap_or_else(|_| Event::default().event("error").data("스냅샷 직렬화 실패"))
}
