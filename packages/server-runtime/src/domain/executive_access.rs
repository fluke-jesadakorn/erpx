//! Access-audit input from an owning source operation; never an inbound grant.
#[derive(Clone, Copy)]
pub enum EvidenceAccessAction {
    Search,
    Open,
}
impl EvidenceAccessAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Open => "open",
        }
    }
}
pub struct EvidenceAccessRow {
    pub id: String,
    pub version: i64,
}
pub struct ExecutiveEvidenceAccess<'a> {
    pub company: &'a str,
    pub user: &'a str,
    pub assignment: &'a str,
    pub provider: &'static str,
    pub metric: &'static str,
    pub action: EvidenceAccessAction,
    pub fields: &'a [&'static str],
    pub query_hash: &'a str,
    pub at: i64,
    pub rows: &'a [EvidenceAccessRow],
}
