//! Domain facts for the native execution of the shared IAM, active assignment
//! and active Company routes. Every operation is allow-listed here; an unknown
//! path can never reach a backend and can never fall back to TypeScript SQL.

use serde_json::Value;

/// One exact, registered public target. A request is served only when its Host
/// and Holding slug match the same key and its Origin matches `origin`.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteMethod {
    Get,
    Post,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteFamily {
    Iam,
    AssignmentsActive,
    CompaniesActive,
}

/// Allow-listed IAM catch-all operations. Each maps to an owner port already
/// executed in Rust; the composition root wires the backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IamOperation {
    Overview,
    MyAccess,
    Audit,
    Inspector,
    MatrixExport,
    PeopleRelationships,
    PeopleRelationshipsPreview,
    PeopleRelationshipsApply,
    PeopleAccess,
    PeopleAccessPreview,
    PeopleAccessApply,
    Roles,
    ReviewsDecide(String),
    ReviewsEvidence(String),
    AccessReviews,
    AccessReviewsCancel(String),
    AccessReviewItemsDecide(String),
    Connections,
    ConnectionsAction(String, String),
    Assignments,
    AssignmentsRevoke(String),
    Mappings,
    MappingsRevoke(String),
    Migrations,
    MigrationsEvidence(String),
    Emergency,
    EmergencyActive,
    EmergencyActivate,
    EmergencyRevoke,
    MfaEnroll,
    MfaVerify,
    SessionsRevoke,
    UserManagementPreview,
    UserManagementSave,
    UserManagementDecideReview(String),
    UsersSessions(String),
    UsersRestore(String),
    UsersSuspend(String),
}

/// Resolves one IAM sub-path. Unknown, ambiguous or over-long paths are
/// rejected so the public handler has no execute-by-default fallback.
pub fn iam_operation(method: RouteMethod, path: &[String]) -> Option<IamOperation> {
    let get = method == RouteMethod::Get;
    let post = method == RouteMethod::Post;
    let id = |value: &str| {
        (!value.is_empty()
            && value.len() <= 200
            && !value.contains('/')
            && !value.contains('\\')
            && value.bytes().all(|byte| !byte.is_ascii_control()))
        .then(|| value.to_owned())
    };
    let part = |index: usize| path.get(index).map(String::as_str);
    if path.is_empty() {
        return get.then_some(IamOperation::Overview);
    }
    let joined = path.join("/");
    if get && matches!(joined.as_str(), "emergency/active" | "matrix/export") {
        return Some(if joined == "matrix/export" {
            IamOperation::MatrixExport
        } else {
            IamOperation::EmergencyActive
        });
    }
    if post
        && matches!(
            joined.as_str(),
            "emergency/activate"
                | "emergency/revoke"
                | "mfa/enroll"
                | "mfa/verify"
                | "sessions/revoke"
        )
    {
        return Some(match joined.as_str() {
            "emergency/activate" => IamOperation::EmergencyActivate,
            "emergency/revoke" => IamOperation::EmergencyRevoke,
            "mfa/enroll" => IamOperation::MfaEnroll,
            "mfa/verify" => IamOperation::MfaVerify,
            _ => IamOperation::SessionsRevoke,
        });
    }
    let first = path.first().map(String::as_str)?;
    match first {
        "roles" if post && path.len() == 1 => Some(IamOperation::Roles),
        "access-reviews" if post && path.len() == 1 => Some(IamOperation::AccessReviews),
        "access-reviews" if post && path.len() == 3 && part(2) == Some("cancel") => {
            id(part(1)?).map(IamOperation::AccessReviewsCancel)
        }
        "access-review-items" if post && path.len() == 3 && part(2) == Some("decide") => {
            id(part(1)?).map(IamOperation::AccessReviewItemsDecide)
        }
        "reviews" if post && path.len() == 3 && part(2) == Some("decide") => {
            id(part(1)?).map(IamOperation::ReviewsDecide)
        }
        "reviews" if post && path.len() == 3 && part(2) == Some("evidence") => {
            id(part(1)?).map(IamOperation::ReviewsEvidence)
        }
        "connections" if post && path.len() == 1 => Some(IamOperation::Connections),
        "connections"
            if post
                && path.len() == 3
                && part(2)
                    .is_some_and(|value| ["test", "activate", "disable"].contains(&value)) =>
        {
            Some(IamOperation::ConnectionsAction(
                id(part(1)?)?,
                part(2)?.to_owned(),
            ))
        }
        "assignments" if get && path.len() == 1 => Some(IamOperation::Assignments),
        "assignments" if post && path.len() == 3 && part(2) == Some("revoke") => {
            id(part(1)?).map(IamOperation::AssignmentsRevoke)
        }
        "mappings" if post && path.len() == 1 => Some(IamOperation::Mappings),
        "mappings" if post && path.len() == 3 && part(2) == Some("revoke") => {
            id(part(1)?).map(IamOperation::MappingsRevoke)
        }
        "migrations" if post && path.len() == 1 => Some(IamOperation::Migrations),
        "migrations" if post && path.len() == 3 && part(2) == Some("evidence") => {
            id(part(1)?).map(IamOperation::MigrationsEvidence)
        }
        "emergency" if post && path.len() == 1 => Some(IamOperation::Emergency),
        "user-management" if post && path.len() == 2 && part(1) == Some("preview") => {
            Some(IamOperation::UserManagementPreview)
        }
        "user-management" if post && path.len() == 2 && part(1) == Some("save") => {
            Some(IamOperation::UserManagementSave)
        }
        "user-management" if post && path.len() == 3 && part(2) == Some("decide-review") => {
            id(part(1)?).map(IamOperation::UserManagementDecideReview)
        }
        "users" if post && path.len() == 3 && part(2) == Some("sessions") => {
            id(part(1)?).map(IamOperation::UsersSessions)
        }
        "users" if post && path.len() == 3 && part(2) == Some("restore") => {
            id(part(1)?).map(IamOperation::UsersRestore)
        }
        "users" if post && path.len() == 3 && part(2) == Some("suspend") => {
            id(part(1)?).map(IamOperation::UsersSuspend)
        }
        "people-relationships" if get && path.len() == 1 => Some(IamOperation::PeopleRelationships),
        "people-relationships" if post && path.len() == 2 && part(1) == Some("preview") => {
            Some(IamOperation::PeopleRelationshipsPreview)
        }
        "people-relationships" if post && path.len() == 2 && part(1) == Some("apply") => {
            Some(IamOperation::PeopleRelationshipsApply)
        }
        "people-access" if get && path.len() == 1 => Some(IamOperation::PeopleAccess),
        "people-access" if post && path.len() == 2 && part(1) == Some("preview") => {
            Some(IamOperation::PeopleAccessPreview)
        }
        "people-access" if post && path.len() == 2 && part(1) == Some("apply") => {
            Some(IamOperation::PeopleAccessApply)
        }
        "overview" if get && path.len() == 1 => Some(IamOperation::Overview),
        "my-access" if get && path.len() == 1 => Some(IamOperation::MyAccess),
        "audit" if get && path.len() == 1 => Some(IamOperation::Audit),
        "inspector" if get && path.len() == 1 => Some(IamOperation::Inspector),
        // `reviews/{id}/decide` is also reached through the generic review
        // command; the backend receives the same id and full request body.
        _ => None,
    }
}

/// One browser-visible cookie action. The composition backend derives the
/// Holding-scoped name from its verified context; the owner HTTP layer only
/// serializes the bounded value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteCookie {
    pub name: String,
    pub value: String,
    pub max_age: i64,
    pub delete: bool,
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
}

pub fn valid_cookie(cookie: &RouteCookie) -> bool {
    !cookie.name.is_empty()
        && cookie.name.len() <= 200
        && cookie.value.len() <= 4096
        && cookie
            .name
            .bytes()
            .chain(cookie.value.bytes())
            .all(|byte| byte > b' ' && byte != 0x7f && byte != b';' && byte != b',')
}

/// One command body, always owner-validated. No SQL or owner-plan fragments are
/// accepted: the backend receives only the parsed JSON input.
#[derive(Clone, Debug)]
pub struct RouteInput {
    pub method: RouteMethod,
    pub body: Value,
    pub key: Option<String>,
    pub query: String,
}
