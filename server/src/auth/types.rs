//! Auth contracts (§4.5). Providers differ only in endpoint URLs and the shape of
//! `userinfo`; the flow itself (PKCE, state, token exchange) is shared.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A supported login provider. The value is used verbatim in URL paths and DB columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Datagsm,
    Google,
}

impl ProviderKind {
    pub const ALL: [Self; 2] = [Self::Datagsm, Self::Google];

    /// URL path segment and `user.provider` column value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Datagsm => "datagsm",
            Self::Google => "google",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    /// Name shown on the login button.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Datagsm => "DataGSM",
            Self::Google => "Google",
        }
    }
}

/// A user as reported by a provider. Account identity is the `(provider, subject)` pair (§4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthIdentity {
    pub provider: ProviderKind,
    /// Provider-side user id. Kept as a string because the format varies by provider.
    pub subject: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
}

/// One provider's OAuth round trip.
#[async_trait]
pub trait OAuthProvider: Send + Sync + 'static {
    /// Where to send the user to sign in.
    fn authorize_url(&self, state: &str, code_challenge: &str, redirect_uri: &str) -> String;

    /// Exchanges an authorization code for tokens and reads the user back.
    async fn identify(
        &self,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthIdentity>;
}
