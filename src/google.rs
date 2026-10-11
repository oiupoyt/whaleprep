// Google Identity Services.
//
// Two paths end at the same place:
//   1. The One Tap / button popup hands the browser an ID token, posted to /student/google.
//   2. Where a popup is blocked (in-app browsers, hardened settings) the page falls back
//      to the authorization-code + PKCE redirect, which returns the same ID token.
//
// Both carry a token this module verifies before anything is trusted. The signature is
// checked against Google's published keys and the audience, issuer and expiry must all
// match this deployment. A token that fails any of that is rejected here, not in the
// browser.

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine as _;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

const JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const ISSUERS: &[&str] = &["https://accounts.google.com", "accounts.google.com"];
const CLOCK_SKEW: u64 = 60;
const JWKS_TTL: Duration = Duration::from_secs(3600);
const MAX_CREDENTIAL: usize = 8192;

#[derive(Deserialize, Debug, Clone)]
pub struct Claims {
    /// Google's stable account id. Decoded so the token round-trips fully; the
    /// account here is keyed on email instead.
    #[allow(dead_code)]
    pub sub: String,
    pub email: String,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
}

impl Claims {
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| self.email.split('@').next().map(str::to_string))
            .unwrap_or_else(|| "Student".into())
    }
}

pub struct Google {
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
    keys: Arc<RwLock<Cached>>,
}

struct Cached {
    fetched: Instant,
    keys: Vec<(String, DecodingKey)>,
}

impl Google {
    pub fn new(client_id: &str, client_secret: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent(concat!("pyq-clone/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            keys: Arc::new(RwLock::new(Cached {
                // Starts stale so the first sign-in fetches the keys.
                fetched: Instant::now() - JWKS_TTL,
                keys: Vec::new(),
            })),
        }
    }

    async fn jwks(&self) -> anyhow::Result<Vec<(String, DecodingKey)>> {
        let cached = self.keys.read().await;
        if !cached.keys.is_empty() && cached.fetched.elapsed() < JWKS_TTL {
            return Ok(cached.keys.clone());
        }
        drop(cached);

        let doc: Jwks = self
            .http
            .get(JWKS_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let mut keys = Vec::new();
        for k in doc.keys {
            if k.alg.as_deref() != Some("RS256") {
                continue;
            }
            // One unusable entry must not sink the set, so a bad key is skipped.
            if let Ok(dec) = DecodingKey::from_rsa_components(&k.n, &k.e) {
                keys.push((k.kid, dec));
            }
        }
        if keys.is_empty() {
            anyhow::bail!("Google published no usable RS256 signing key");
        }
        let mut guard = self.keys.write().await;
        guard.fetched = Instant::now();
        guard.keys = keys.clone();
        Ok(keys)
    }

    /// Verifies an ID token end to end: signature, audience, issuer, expiry, then the
    /// account fields this app actually depends on.
    pub async fn verify(&self, id_token: &str) -> Result<Claims, String> {
        let kid = header_kid(id_token).ok_or("that credential is not a Google sign-in token")?;
        let keys = self.jwks().await.map_err(|e| format!("cannot reach Google right now: {e}"))?;
        let key = keys
            .iter()
            .find(|(k, _)| k == &kid)
            .map(|(_, key)| key)
            .ok_or("Google rotated this signing key, so that token cannot be checked")?;
        let claims = decode_claims(id_token, key, &self.client_id)?;
        usable_account(&claims)?;
        Ok(claims)
    }

    /// Authorization-code redemption, used by the redirect fallback.
    pub async fn exchange_code(&self, code: &str, verifier: &str, redirect_uri: &str) -> Result<Claims, String> {
        let mut form = vec![
            ("code", code.to_string()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", verifier.to_string()),
            ("redirect_uri", redirect_uri.to_string()),
            ("grant_type", "authorization_code".to_string()),
        ];
        if !self.client_secret.is_empty() {
            form.push(("client_secret", self.client_secret.clone()));
        }
        let res: TokenResponse = self
            .http
            .post(TOKEN_URL)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("cannot reach Google right now: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Google refused the code exchange: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Google sent an unexpected reply: {e}"))?;
        if let Some(err) = res.error {
            return Err(format!("Google rejected that sign-in: {}", err.error_description.unwrap_or(err.error)));
        }
        self.verify(&res.id_token).await
    }
}

/// Pure half of `verify`, kept separate so it can be tested without a network call.
fn decode_claims(token: &str, key: &DecodingKey, client_id: &str) -> Result<Claims, String> {
    if token.len() > MAX_CREDENTIAL {
        return Err("that credential is too long to be a token".into());
    }
    let mut v = Validation::new(Algorithm::RS256);
    v.set_audience(&[client_id]);
    v.set_issuer(ISSUERS);
    v.leeway = CLOCK_SKEW;
    v.validate_exp = true;

    jsonwebtoken::decode::<Claims>(token, key, &v)
        .map(|d| d.claims)
        .map_err(|e| format!("that sign-in could not be verified: {e}"))
}

fn usable_account(c: &Claims) -> Result<(), String> {
    if c.email_verified == Some(false) {
        return Err("that Google account has no verified email address".into());
    }
    if c.email.trim().is_empty() || !c.email.contains('@') {
        return Err("that Google account has no email address to key the account on".into());
    }
    Ok(())
}

/// Reads `kid` from the JWT header so the matching key is chosen before decoding.
fn header_kid(token: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Header {
        kid: Option<String>,
    }
    let raw = B64.decode(token.split('.').next()?).ok()?;
    serde_json::from_slice::<Header>(&raw).ok()?.kid
}

pub fn new_verifier() -> String {
    random_b64()
}

pub fn challenge(verifier: &str) -> String {
    B64.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn new_state() -> String {
    random_b64()
}

fn random_b64() -> String {
    // Two UUIDs give 32 bytes, base64url-encoded to 43 characters: inside the
    // 43..128 range RFC 7636 asks of a PKCE verifier.
    let bytes: Vec<u8> = uuid::Uuid::new_v4()
        .as_bytes()
        .iter()
        .chain(uuid::Uuid::new_v4().as_bytes().iter())
        .copied()
        .collect();
    B64.encode(bytes)
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: String,
    alg: Option<String>,
    n: String,
    e: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    id_token: String,
    #[serde(default)]
    error: Option<TokenError>,
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::pkcs8::SecretDocument;
    use rsa::traits::PublicKeyParts;
    use rsa::RsaPrivateKey;
    use serde::Serialize;

    const APP: &str = "client-abc.apps.googleusercontent.com";

    #[derive(Serialize)]
    struct Claims {
        sub: String,
        email: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        email_verified: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        exp: usize,
        aud: String,
        iss: String,
    }

    struct Fixture {
        signing: EncodingKey,
        verifying: DecodingKey,
        foreign_signing: EncodingKey,
    }

    fn fixture() -> Fixture {
        let mine = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("rsa key");
        let theirs = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("rsa key");
        // jsonwebtoken wants the raw PKCS#1 private key, not the PKCS#8 wrapper.
        let der = |k: &RsaPrivateKey| {
            let doc: SecretDocument = k.to_pkcs1_der().expect("der");
            EncodingKey::from_rsa_der(doc.as_bytes())
        };
        Fixture {
            signing: der(&mine),
            verifying: DecodingKey::from_rsa_components(&B64.encode(mine.n().to_bytes_be()), &B64.encode(mine.e().to_bytes_be()))
                .expect("verifying key"),
            foreign_signing: der(&theirs),
        }
    }

    fn claims(mutate: impl FnOnce(&mut Claims)) -> Claims {
        let mut c = Claims {
            sub: "1234567890".into(),
            email: "student@example.com".into(),
            email_verified: Some(true),
            name: Some("Test Student".into()),
            exp: chrono::Utc::now().timestamp() as usize + 3600,
            aud: APP.into(),
            iss: ISSUERS[0].into(),
        };
        mutate(&mut c);
        c
    }

    fn token(f: &Fixture, mutate: impl FnOnce(&mut Claims)) -> String {
        encode(&Header::new(Algorithm::RS256), &claims(mutate), &f.signing).expect("sign")
    }

    #[test]
    fn accepts_a_well_formed_token() {
        let f = fixture();
        let c = decode_claims(&token(&f, |_| {}), &f.verifying, APP).expect("should verify");
        assert_eq!(c.email, "student@example.com");
        assert_eq!(c.display_name(), "Test Student");
    }

    #[test]
    fn accepts_the_bare_issuer_without_a_scheme() {
        let f = fixture();
        let t = token(&f, |c| c.iss = "accounts.google.com".into());
        assert!(decode_claims(&t, &f.verifying, APP).is_ok());
    }

    #[test]
    fn rejects_a_token_signed_by_another_key() {
        let f = fixture();
        let forged = encode(&Header::new(Algorithm::RS256), &claims(|_| {}), &f.foreign_signing).expect("sign");
        assert!(decode_claims(&forged, &f.verifying, APP).is_err(), "a foreign signing key must not verify");
    }

    #[test]
    fn rejects_a_token_audienced_to_another_app() {
        let f = fixture();
        let t = token(&f, |c| c.aud = "somebody-elses-app.apps.googleusercontent.com".into());
        let err = decode_claims(&t, &f.verifying, APP).unwrap_err();
        assert!(err.contains("Audience"), "an audience mismatch must be reported as such, got: {err}");
    }

    #[test]
    fn rejects_a_token_from_another_issuer() {
        let f = fixture();
        let t = token(&f, |c| c.iss = "https://evil.example".into());
        assert!(decode_claims(&t, &f.verifying, APP).is_err());
    }

    #[test]
    fn rejects_an_expired_token() {
        let f = fixture();
        let past = chrono::Utc::now().timestamp() as usize - 3600;
        let t = token(&f, |c| c.exp = past);
        assert!(decode_claims(&t, &f.verifying, APP).is_err());
    }

    #[test]
    fn rejects_a_tampered_payload() {
        let f = fixture();
        let good = token(&f, |_| {});
        let mut parts: Vec<String> = good.split('.').map(str::to_string).collect();
        let forged = format!(
            r#"{{"sub":"1","email":"attacker@example.com","email_verified":true,"exp":{},"aud":"{APP}","iss":"https://accounts.google.com"}}"#,
            chrono::Utc::now().timestamp() as usize + 3600
        );
        parts[1] = B64.encode(forged);
        assert!(decode_claims(&parts.join("."), &f.verifying, APP).is_err(), "a rewritten payload must not verify");
    }

    #[test]
    fn rejects_an_unverified_email() {
        let f = fixture();
        let t = token(&f, |c| c.email_verified = Some(false));
        let c = decode_claims(&t, &f.verifying, APP).expect("the signature is still valid");
        assert!(usable_account(&c).is_err());
    }

    #[test]
    fn falls_back_to_the_email_when_a_name_is_absent() {
        let f = fixture();
        let t = token(&f, |c| c.name = None);
        let c = decode_claims(&t, &f.verifying, APP).expect("verifies");
        assert_eq!(c.display_name(), "student");
    }

    #[test]
    fn pkce_challenge_is_the_sha256_of_the_verifier() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifiers_and_states_are_unique_and_sized_for_rfc_7636() {
        let a = new_verifier();
        let b = new_verifier();
        assert_ne!(a, b);
        assert!((43..=128).contains(&a.len()), "verifier length {} is out of range", a.len());
        assert_ne!(new_state(), new_state());
    }

    #[test]
    fn a_header_without_a_kid_is_not_treated_as_a_token() {
        let head = B64.encode(br#"{"alg":"RS256"}"#);
        let t = format!("{head}.e30.c2ln");
        assert_eq!(header_kid(&t), None);
        assert_eq!(header_kid("not-a-jwt"), None);
    }

    #[test]
    fn a_credential_longer_than_a_token_is_rejected() {
        let f = fixture();
        let padded = "x".repeat(MAX_CREDENTIAL + 1);
        assert!(decode_claims(&padded, &f.verifying, APP).is_err());
    }
}