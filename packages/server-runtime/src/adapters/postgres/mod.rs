pub(crate) mod config;
pub(crate) mod migrations;
pub(crate) mod pool;
pub(crate) mod request_lease;
pub(crate) mod unit_of_work;

/// Sanitized persistence failures never contain SQL, parameters, passwords or DSNs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersistenceError {
    pub code: &'static str,
    sql_state: Option<String>,
    context: Option<String>,
}
impl PersistenceError {
    pub fn new(code: &'static str) -> Self {
        Self {
            code,
            sql_state: None,
            context: None,
        }
    }
    pub(crate) fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
    pub fn sql_state(&self) -> Option<&str> {
        self.sql_state.as_deref()
    }
    pub fn retryable(&self) -> bool {
        matches!(self.sql_state(), Some("40001" | "40P01"))
    }
}
impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}:{context}", self.code),
            None => f.write_str(self.code),
        }
    }
}
impl std::error::Error for PersistenceError {}
impl From<sqlx::Error> for PersistenceError {
    fn from(error: sqlx::Error) -> Self {
        Self {
            code: "POSTGRES_OPERATION_FAILED",
            context: None,
            sql_state: error
                .as_database_error()
                .and_then(|e| e.code())
                .map(|s| s.into_owned()),
        }
    }
}
