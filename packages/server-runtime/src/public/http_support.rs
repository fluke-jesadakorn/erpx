//! HTTP primitives re-exported for owner-local routers.
//!
//! Owner crates that expose their own `Router` must not add a second `axum`
//! dependency edge (that would require a lockfile change). This module is the
//! single shared re-export; the router itself remains owner-owned.
pub use axum::{
    Json, Router,
    body::{self, Body, to_bytes},
    extract::{Path, Query, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{self, get, post},
};
