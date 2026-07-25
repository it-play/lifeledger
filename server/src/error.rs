//! Error handling at the HTTP boundary.
//!
//! Domain and store layers raise `anyhow::Error`; this is the single place it becomes a
//! status code. Causes stay in the server log so database errors never reach a client.

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
