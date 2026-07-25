//! PKCE (RFC 7636) and random token generation.
//!
//! Everything is Base64URL without padding: the form RFC 7636 §4.1 requires, and safe
//! to drop into a cookie or URL without escaping.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// RFC 7636 requires a 43-128 character verifier; 32 bytes in Base64URL is exactly 43.
const VERIFIER_BYTES: usize = 32;

/// Generates a value that must be unpredictable (code_verifier, state, session token).
///
/// Fails loudly rather than falling back to a weaker source of randomness.
pub fn random_token() -> anyhow::Result<String> {
    let mut bytes = [0u8; VERIFIER_BYTES];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("난수를 얻지 못했습니다: {error}"))?;

    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// code_verifier -> code_challenge (S256).
pub fn code_challenge_of(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());

    URL_SAFE_NO_PAD.encode(digest)
}

/// Only this hash reaches the database; the session token itself is never stored (§4.5).
pub fn token_hash_of(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());

    // Lowercase hex, sized for the CHAR(64) column
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod context_pkce_challenge_is_derived {
        use super::*;

        /// Test vector from RFC 7636 Appendix B.
        #[test]
        fn given_the_rfc_example_verifier_when_hashed_then_it_matches_the_rfc_challenge() {
            let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

            let challenge = code_challenge_of(verifier);

            assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        }

        #[test]
        fn given_two_different_verifiers_when_hashed_then_challenges_differ() {
            let first = code_challenge_of("verifier-one");
            let second = code_challenge_of("verifier-two");

            assert_ne!(first, second);
        }
    }

    mod context_a_random_token_is_generated {
        use super::*;

        #[test]
        fn given_two_generations_when_compared_then_they_are_not_the_same() {
            let first = random_token().expect("난수를 얻을 수 있어야 한다");
            let second = random_token().expect("난수를 얻을 수 있어야 한다");

            assert_ne!(first, second);
        }

        #[test]
        fn given_a_generated_token_when_measured_then_it_meets_the_rfc_length_floor() {
            let token = random_token().expect("난수를 얻을 수 있어야 한다");

            assert!(token.len() >= 43, "실제 길이: {}", token.len());
        }
    }

    mod context_a_session_token_is_stored {
        use super::*;

        #[test]
        fn given_a_token_when_hashed_then_it_fits_the_char64_column() {
            let hash = token_hash_of("some-session-token");

            assert_eq!(hash.len(), 64);
        }

        #[test]
        fn given_the_same_token_when_hashed_twice_then_the_hash_is_stable() {
            let first = token_hash_of("same-token");
            let second = token_hash_of("same-token");

            assert_eq!(first, second);
        }
    }
}
