//! Signs a token the way Google would, but with a key the server does not trust.
//! Used to confirm the callback refuses a well-formed token from an unknown issuer.
//!
//!   cargo run --example forge_google_token -- <client_id> <email>

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::Serialize;

#[derive(Serialize)]
struct Claims {
    sub: String,
    email: String,
    email_verified: bool,
    name: String,
    exp: i64,
    aud: String,
    iss: String,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let client_id = args.next().unwrap_or_else(|| "test-client.apps.googleusercontent.com".into());
    let email = args.next().unwrap_or_else(|| "attacker@example.com".into());

    // A throwaway key. The server has never seen it, so its signature must not verify.
    let pem = std::process::Command::new("openssl")
        .args(["genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048"])
        .output()
        .expect("openssl is needed to generate the throwaway key")
        .stdout;
    let key = EncodingKey::from_rsa_pem(&pem).expect("key");

    let claims = Claims {
        sub: "9999999999".into(),
        email,
        email_verified: true,
        name: "Attacker".into(),
        exp: chrono::Utc::now().timestamp() + 3600,
        aud: client_id,
        iss: "https://accounts.google.com".into(),
    };
    // A real Google token names the key that signed it. Reusing a published kid
    // forces the server down its "check the signature" path rather than the
    // "which key is this" one, which is the case worth proving.
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("a-key-id-nobody-signed-with".into());
    println!("{}", encode(&header, &claims, &key).expect("sign"));
}