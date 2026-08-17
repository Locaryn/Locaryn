//! Drivers par dialecte : `responses` (OpenResponses — OpenClaw) et `runs`
//! (Runs API — Hermes). `chat_completions` est servi directement par la
//! boucle existante du runtime agent (voir `crate::CoreAgent::run`).

pub mod responses;
pub mod runs;
