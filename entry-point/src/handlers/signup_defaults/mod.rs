//! Signup-defaults engine.
//!
//! Catalog rows flagged `is_default` are granted to every user as soon
//! as they sign up. The same engine powers the admin backfill button
//! that retroactively grants the current default set to every existing
//! user (idempotent via ON CONFLICT DO NOTHING).
//!
//! Resources covered:
//!   skins       — INSERT INTO user_skins
//!   avatars     — INSERT INTO user_avatars
//!   frames      — INSERT INTO user_frames
//!   characters  — INSERT INTO user_characters with unlocked = true
//!                 (this is on top of the legacy `default_unlocked`
//!                 column which makes the character render unlocked in
//!                 the list without a concrete row — keep both for now)
//!   store_items — apply payload Grants via `apply_grant`. IAP-priced
//!                 items still get their payload applied (no Etomin
//!                 charge — this is an admin gift channel).

pub mod backfill;
pub mod router;
pub mod service;
