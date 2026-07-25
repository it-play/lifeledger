//! Google OAuth (OpenID Connect).
//!
//! Plain OIDC: the token exchange body is a form and the user identifier is `sub`.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;

use super::types::{OAuthIdentity, OAuthProvider, ProviderKind};

const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
/// Non-sensitive scopes, so no Google verification is required.
const SCOPE: &str = "openid email profile";

pub struct GoogleProvider {
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
}

pub fn create_google_provider(
    http: reqwest::Client,
    client_id: String,
    client_secret: String,
) -> GoogleProvider {
    GoogleProvider {
        http,
        client_id,
        client_secret,
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct UserInfo {
    /// OIDC subject. Stable even when the email address changes.
    sub: String,
    email: Option<String>,
    name: Option<String>,
}

#[async_trait]
impl OAuthProvider for GoogleProvider {
    fn authorize_url(&self, state: &str, code_challenge: &str, redirect_uri: &str) -> String {
        let query = super::query_string(&[
            ("client_id", &self.client_id),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("state", state),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
            ("scope", SCOPE),
        ]);

        format!("{AUTHORIZE_URL}?{query}")
    }

    async fn identify(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthIdentity> {
        let token: TokenResponse = super::read_json(
            self.http
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "authorization_code"),
                    ("code", code),
                    ("client_id", self.client_id.as_str()),
                    ("client_secret", self.client_secret.as_str()),
                    ("redirect_uri", redirect_uri),
                    ("code_verifier", code_verifier),
                ])
                .send()
                .await
                .context("Google 토큰 교환 요청이 실패했습니다")?,
        )
        .await
        .context("Google 토큰 교환 응답을 읽지 못했습니다")?;

        let info: UserInfo = super::read_json(
            self.http
                .get(USERINFO_URL)
                .bearer_auth(&token.access_token)
                .send()
                .await
                .context("Google 사용자 조회 요청이 실패했습니다")?,
        )
        .await
        .context("Google 사용자 조회 응답을 읽지 못했습니다")?;

        Ok(OAuthIdentity {
            provider: ProviderKind::Google,
            subject: info.sub,
            email: info.email,
            display_name: info.name,
        })
    }
}
