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

use crate::character;
use crate::error::AppError;
use crate::state::{AppState, GameSnapshot};

/// 서버가 재연결 지연으로 권하는 값. 클라이언트는 이 값을 백오프의 기준으로 쓴다.
const RETRY_HINT: Duration = Duration::from_secs(1);
/// keep-alive 주석 간격. 프록시가 유휴 연결을 끊는 것을 막는다.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// API 문서. 경로는 nginx 가 `/api` 를 붙여 넘기므로 그 아래에 둔다.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "LifeLedger API",
        description = "모의 자산관리 인생 시뮬레이션 서버"
    ),
    paths(health, presets, create_character, snapshot, advance),
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
        .with_state(state)
        .merge(
            SwaggerUi::new("/api/docs")
                .url("/api/docs/openapi.json", ApiDoc::openapi())
                // UI 가 스펙을 부를 때는 상대 경로를 쓴다. 절대 경로면 nginx 가 앞에 붙이는
                // prefix(`/lifeledger`)를 건너뛰어 도메인 루트를 찾아가 버린다
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

/// 캐릭터 생성. 검증은 도메인(§3.5)이 하고 여기서는 상태 코드만 정한다.
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
    Json(draft): Json<character::CharacterDraft>,
) -> Result<Json<GameSnapshot>, CreateCharacterError> {
    let character = character::create_character(draft).map_err(CreateCharacterError::Invalid)?;

    Ok(Json(state.start_game(character).await?))
}

/// 422(검증 실패)와 500(저장 실패)은 원인이 달라 응답 모양도 다르다.
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
async fn snapshot(State(state): State<Arc<AppState>>) -> Result<Json<GameSnapshot>, AppError> {
    Ok(Json(state.snapshot().await?))
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
    Json(request): Json<AdvanceRequest>,
) -> Result<Json<GameSnapshot>, AppError> {
    // 상한을 두어 한 요청이 서버를 오래 점유하지 못하게 한다
    let days = request.days.clamp(1, 3650);

    Ok(Json(state.advance(days).await?))
}

/// 게임일 전진 스트림.
///
/// 이벤트 이름은 `tick`, `id` 는 게임일이다. 클라이언트가 재연결할 때
/// `Last-Event-ID` 로 마지막 게임일을 보내오므로, 나중에 그 지점부터 재생할 수 있다.
async fn stream(
    State(state): State<Arc<AppState>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let current = state.snapshot().await?;
    let updates = BroadcastStream::new(state.subscribe())
        .filter_map(|result| result.ok())
        .map(|snapshot| Ok(to_event(&snapshot)));

    // 연결 직후 현재 상태를 한 번 보낸다 — 클라이언트가 별도 조회 없이 그릴 수 있다
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
