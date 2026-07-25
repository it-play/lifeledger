//! DataGSM OAuth (<https://docs.datagsm.kr/oauth>).
//!
//! Standard authorization code + PKCE, with two deviations:
//!   - the token exchange body is JSON, not a form
//!   - authorize/token and userinfo live on different hosts

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::types::{OAuthIdentity, OAuthProvider, ProviderKind};

const AUTHORIZE_URL: &str = "https://oauth.authorization.datagsm.kr/v1/oauth/authorize";
const TOKEN_URL: &str = "https://oauth.authorization.datagsm.kr/v1/oauth/token";
const USERINFO_URL: &str = "https://oauth.resource.datagsm.kr/userinfo";

pub struct DatagsmProvider {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
}

/// DataGSM scope strings are always `{applicationId}:{scopeName}`, so reading a user's own
/// record is `datagsm:self_read` — not the `self:read` the public docs give, which is
/// rejected with `invalid_scope`.
///
/// Passing `None` omits the parameter and grants every scope the client is registered for.
/// We name the one we need instead, so registering another scope later does not silently
/// widen this login.
pub const fn create_datagsm_provider(
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
) -> DatagsmProvider {
    DatagsmProvider {
        http,
        client_id,
        client_secret,
        scope,
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// The `userinfo` response, narrowed to what the game uses today.
#[derive(Deserialize)]
struct UserInfo {
    id: i64,
    email: Option<String>,
    student: Option<StudentInfo>,
}

#[derive(Deserialize)]
struct StudentInfo {
    name: Option<String>,
}

#[async_trait]
impl OAuthProvider for DatagsmProvider {
    fn authorize_url(&self, state: &str, code_challenge: &str, redirect_uri: &str) -> String {
        let mut pairs = vec![
            ("client_id", self.client_id.as_str()),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("state", state),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
        ];
        if let Some(scope) = &self.scope {
            pairs.push(("scope", scope));
        }

        format!("{AUTHORIZE_URL}?{}", super::query_string(&pairs))
    }

    async fn identify(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthIdentity> {
        // Send PKCE and client_secret together: this exchange happens server-side
        let token: TokenResponse = super::read_json(
            self.http
                .post(TOKEN_URL)
                .json(&serde_json::json!({
                    "grant_type": "authorization_code",
                    "code": code,
                    "client_id": self.client_id,
                    "client_secret": self.client_secret,
                    "redirect_uri": redirect_uri,
                    "code_verifier": code_verifier,
                }))
                .send()
                .await
                .context("DataGSM 토큰 교환 요청이 실패했습니다")?,
        )
        .await
        .context("DataGSM 토큰 교환 응답을 읽지 못했습니다")?;

        let info: UserInfo = super::read_json(
            self.http
                .get(USERINFO_URL)
                .bearer_auth(&token.access_token)
                .send()
                .await
                .context("DataGSM 사용자 조회 요청이 실패했습니다")?,
        )
        .await
        .context("DataGSM 사용자 조회 응답을 읽지 못했습니다")?;

        Ok(OAuthIdentity {
            provider: ProviderKind::Datagsm,
            subject: info.id.to_string(),
            email: info.email,
            // Only students carry a name
            display_name: info.student.and_then(|student| student.name),
        })
    }
}
