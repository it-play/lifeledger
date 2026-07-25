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

use crate::character;
use crate::state::{AppState, GameSnapshot};

/// 서버가 재연결 지연으로 권하는 값. 클라이언트는 이 값을 백오프의 기준으로 쓴다.
const RETRY_HINT: Duration = Duration::from_secs(1);
/// keep-alive 주석 간격. 프록시가 유휴 연결을 끊는 것을 막는다.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/presets", get(presets))
        .route("/api/characters", post(create_character))
        .route("/api/state", get(snapshot))
        .route("/api/advance", post(advance))
        .route("/api/stream", get(stream))
        .with_state(state)
}

async fn presets() -> Json<&'static [character::Preset]> {
    Json(character::presets())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationFailure {
    errors: Vec<character::ValidationError>,
}

/// 캐릭터 생성. 검증은 도메인(§3.5)이 하고 여기서는 상태 코드만 정한다.
async fn create_character(
    State(state): State<Arc<AppState>>,
    Json(draft): Json<character::CharacterDraft>,
) -> Result<Json<GameSnapshot>, (StatusCode, Json<ValidationFailure>)> {
    match character::create_character(draft) {
        Ok(character) => Ok(Json(state.start_game(character))),
        Err(errors) => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ValidationFailure { errors }),
        )),
    }
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn snapshot(State(state): State<Arc<AppState>>) -> Json<GameSnapshot> {
    Json(state.snapshot())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdvanceRequest {
    days: u32,
}

async fn advance(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AdvanceRequest>,
) -> Json<GameSnapshot> {
    // 상한을 두어 한 요청이 서버를 오래 점유하지 못하게 한다
    let days = request.days.clamp(1, 3650);
    Json(state.advance(days))
}

/// 게임일 전진 스트림.
///
/// 이벤트 이름은 `tick`, `id` 는 게임일이다. 클라이언트가 재연결할 때
/// `Last-Event-ID` 로 마지막 게임일을 보내오므로, 나중에 그 지점부터 재생할 수 있다.
async fn stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let current = state.snapshot();
    let updates = BroadcastStream::new(state.subscribe())
        .filter_map(|result| result.ok())
        .map(|snapshot| Ok(to_event(&snapshot)));

    // 연결 직후 현재 상태를 한 번 보낸다 — 클라이언트가 별도 조회 없이 그릴 수 있다
    let initial = tokio_stream::once(Ok(to_event(&current).retry(RETRY_HINT)));

    Sse::new(initial.chain(updates)).keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
}

fn to_event(snapshot: &GameSnapshot) -> Event {
    Event::default()
        .event("tick")
        .id(snapshot.game_day.to_string())
        .json_data(snapshot)
        .unwrap_or_else(|_| Event::default().event("error").data("스냅샷 직렬화 실패"))
}
