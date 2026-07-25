//! HTTP 경계의 오류 처리.
//!
//! 도메인·저장소는 `anyhow::Error` 로 실패를 올리고, 여기서 한 번만 상태 코드로 바꾼다.
//! 원인은 서버 로그에만 남긴다 — 클라이언트에 DB 오류 문자열을 흘리지 않는다.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Serialize)]
struct ErrorBody {
    message: &'static str,
}

pub struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(error = ?self.0, "요청 처리에 실패했습니다");

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorBody {
                message: "서버에서 요청을 처리하지 못했습니다",
            }),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(error: E) -> Self {
        Self(error.into())
    }
}
