//! Authentication (§4.5): two OAuth providers, kept alive by a session cookie.

mod datagsm;
mod google;
mod pkce;
mod session;
mod types;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

pub use pkce::{code_challenge_of, random_token, token_hash_of};
pub use session::{AuthUser, SESSION_COOKIE, SESSION_TTL, TRANSACTION_COOKIE, TRANSACTION_TTL};
pub use types::{OAuthIdentity, OAuthProvider, ProviderKind};

/// Keeps a request from hanging forever when a provider stops responding.
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

/// The configured providers. One without credentials never enters the map: a missing
/// button beats a button that fails when pressed.
pub struct Providers {
    by_kind: HashMap<&'static str, Arc<dyn OAuthProvider>>,
    /// Base of the callback address; must match the registered redirect URI exactly.
    public_origin: String,
}

impl Providers {
    /// Assembles providers from environment credentials.
    ///
    /// A provider is enabled only when both `{PROVIDER}_CLIENT_ID` and
    /// `{PROVIDER}_CLIENT_SECRET` are present.
    pub fn from_env(public_origin: String) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .context("HTTP 클라이언트를 만들지 못했습니다")?;

        let mut by_kind: HashMap<&'static str, Arc<dyn OAuthProvider>> = HashMap::new();

        for kind in ProviderKind::ALL {
            let Some((client_id, client_secret)) = read_credentials(kind)? else {
                tracing::warn!(provider = kind.as_str(), "no credentials, disabled");
                continue;
            };

            let provider: Arc<dyn OAuthProvider> = match kind {
                ProviderKind::Datagsm => Arc::new(datagsm::create_datagsm_provider(
                    http.clone(),
                    client_id,
                    client_secret,
                    read_optional("DATAGSM_SCOPE"),
                )),
                ProviderKind::Google => Arc::new(google::create_google_provider(
                    http.clone(),
                    client_id,
                    client_secret,
                )),
            };

            tracing::info!(provider = kind.as_str(), "login provider enabled");
            by_kind.insert(kind.as_str(), provider);
        }

        Ok(Self {
            by_kind,
            public_origin,
        })
    }

    pub fn get(&self, kind: ProviderKind) -> Option<&Arc<dyn OAuthProvider>> {
        self.by_kind.get(kind.as_str())
    }

    /// The enabled providers, which is what the client draws login buttons from.
    pub fn enabled(&self) -> Vec<ProviderKind> {
        ProviderKind::ALL
            .into_iter()
            .filter(|kind| self.by_kind.contains_key(kind.as_str()))
            .collect()
    }

    /// Must be byte-identical to the URI registered in the provider's console.
    pub fn redirect_uri(&self, kind: ProviderKind) -> String {
        format!("{}/api/auth/{}/callback", self.public_origin, kind.as_str())
    }
}

/// Reads an optional setting, treating an empty value as absent.
fn read_optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn read_credentials(kind: ProviderKind) -> Result<Option<(String, String)>> {
    let prefix = kind.as_str().to_uppercase();
    let id = std::env::var(format!("{prefix}_CLIENT_ID")).ok();
    let secret = std::env::var(format!("{prefix}_CLIENT_SECRET")).ok();

    match (id, secret) {
        (Some(id), Some(secret)) if !id.is_empty() && !secret.is_empty() => Ok(Some((id, secret))),
        (None, None) => Ok(None),
        // Half-filled credentials are a config mistake; failing silently hides the cause
        _ => bail!("{prefix}_CLIENT_ID 와 {prefix}_CLIENT_SECRET 은 함께 있어야 합니다"),
    }
}

/// Builds a query string. Values may contain `:` or spaces (DataGSM's `self:read`,
/// Google's `openid email profile`), so they are encoded here.
fn query_string(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{key}={}", percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encodes everything outside the RFC 3986 unreserved set.
fn percent_encode(raw: &str) -> String {
    let mut encoded = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }

    encoded
}

/// Reads a provider response as JSON.
///
/// Checks the status first: deserializing an error response reports a misleading
/// "missing field" instead of what actually went wrong.
async fn read_json<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context("응답 본문을 읽지 못했습니다")?;

    decode_provider_response(status, &body)
}

fn decode_provider_response<T: serde::de::DeserializeOwned>(
    status: reqwest::StatusCode,
    body: &str,
) -> Result<T> {
    if !status.is_success() {
        bail!("제공자가 {status} 를 응답했습니다");
    }

    serde_json::from_str(body).context("응답을 해석하지 못했습니다")
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_a_query_string_is_built {
        use super::*;

        #[test]
        fn given_a_scope_with_a_colon_when_encoded_then_the_colon_is_escaped() {
            let query = query_string(&[("scope", "self:read")]);

            assert_eq!(query, "scope=self%3Aread");
        }

        #[test]
        fn given_a_scope_with_spaces_when_encoded_then_spaces_are_escaped() {
            let query = query_string(&[("scope", "openid email profile")]);

            assert_eq!(query, "scope=openid%20email%20profile");
        }

        #[test]
        fn given_a_redirect_uri_when_encoded_then_it_survives_a_round_trip_unambiguously() {
            let query = query_string(&[(
                "redirect_uri",
                "https://example.com/api/auth/google/callback",
            )]);

            assert_eq!(
                query,
                "redirect_uri=https%3A%2F%2Fexample.com%2Fapi%2Fauth%2Fgoogle%2Fcallback"
            );
        }

        #[test]
        fn given_several_pairs_when_built_then_they_are_joined_with_ampersands() {
            let query = query_string(&[("a", "1"), ("b", "2")]);

            assert_eq!(query, "a=1&b=2");
        }
    }

    mod context_a_provider_path_segment_is_parsed {
        use super::*;

        #[test]
        fn given_a_known_segment_when_parsed_then_it_maps_to_its_provider() {
            assert_eq!(ProviderKind::parse("datagsm"), Some(ProviderKind::Datagsm));
        }

        #[test]
        fn given_an_unknown_segment_when_parsed_then_nothing_is_returned() {
            assert_eq!(ProviderKind::parse("kakao"), None);
        }
    }

    mod context_a_provider_response_contains_sensitive_data {
        use super::*;

        const SENSITIVE_BODY: &str = "access_token=provider-secret";

        fn assert_error_does_not_expose_sensitive_body(error: &anyhow::Error) {
            assert!(!format!("{error}").contains(SENSITIVE_BODY));
            assert!(!format!("{error:?}").contains(SENSITIVE_BODY));
        }

        #[test]
        fn given_an_error_status_when_decoded_then_the_body_is_not_exposed() {
            let error = decode_provider_response::<serde_json::Value>(
                reqwest::StatusCode::UNAUTHORIZED,
                SENSITIVE_BODY,
            )
            .expect_err("an error response must fail");

            assert_error_does_not_expose_sensitive_body(&error);
        }

        #[test]
        fn given_malformed_json_when_decoded_then_the_body_is_not_exposed() {
            let error = decode_provider_response::<serde_json::Value>(
                reqwest::StatusCode::OK,
                SENSITIVE_BODY,
            )
            .expect_err("malformed JSON must fail");

            assert_error_does_not_expose_sensitive_body(&error);
        }
    }
}
