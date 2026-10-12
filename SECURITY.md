# Security notes

Audited 2026-10-13 with `cargo audit` (1307 advisories, 333 crates).

## RUSTSEC-2023-0071: rsa crate, Marvin timing sidechannel (medium, no fix)

The `rsa` crate (via `jsonwebtoken`'s `rust_crypto` feature, used to verify
Google ID-token signatures) has no fixed upgrade. Assessment for this
service: the Marvin attack recovers **private** keys through a decryption
timing oracle. This server only performs **public-key verification** of
Google-issued tokens; it holds no RSA private key and never decrypts or
signs with RSA. The vulnerable operation does not occur here. Accepted risk,
revisit if an upstream fix lands or if private-key RSA use is added.

## Session handling

- Sessions are random 128-bit tokens in HttpOnly, SameSite=Lax cookies,
  30-day expiry. No passwords exist; email sign-in is deliberately
  low-assurance (any name + email starts a local-only account).
- Google sign-in verifies ID-token signature, audience, issuer, expiry,
  and email_verified server-side; `state` is single-use, 10-minute expiry.
- Autosave is per-user rate-limited (30/min) to protect SQLite's writer.

## Data handling

- All data stays on the host's SQLite file. No analytics, no third-party
  trackers. CDN dependencies (KaTeX, Google Fonts, Google Identity
  Services) load from their CDNs only on pages that need them.
- Daily backups: `deploy/termux-backup.sh` (VACUUM INTO, 7-day rotation).
