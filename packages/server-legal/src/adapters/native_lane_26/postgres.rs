//! sqlx execution for the native Legal executed-copy read.
//!
//! Every statement joins `legal_items` so the Company scope is enforced by the
//! owner query rather than trusted from the caller. The statements are fixed
//! constants: no request value selects a table, a column or a predicate.

use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use sqlx::Row;

use super::application::{
    CitationEvidenceSource, ExecutedCopySource, LaneFuture, TrustMaterialSource,
};
use super::domain::{
    COPY_LIMIT, CitationChunk, CopyAuthors, CopyHead, ReviewRow, TRUST_MATERIAL_LIMIT,
    TrustMaterial, TrustMaterialDetail,
};

/// Copy list. Original owner SQL selected `e.*,d.name,d.version` for one item;
/// the `legal_items` join and its `company_id` bind are the Rust-owned scope.
pub const SQL_LIST: &str = "SELECT e.id,e.item_id,e.document_id,e.approved_cycle::bigint approved_cycle,e.approved_document_id,e.approved_hash,e.executed_hash,e.metadata,e.state,e.imported_by,e.imported_at,e.reviewed_by,e.reviewed_at,e.version::bigint version,d.name document_name,d.version::bigint document_version \
 FROM legal_executed_copies e JOIN legal_items scope ON scope.id=e.item_id \
 JOIN legal_documents d ON d.id=e.document_id \
 WHERE e.item_id=$1 AND scope.company_id=$2 ORDER BY e.imported_at DESC,e.id LIMIT $3";

/// Per-copy author witness. Original owner SQL is retained verbatim apart from
/// the Company-scoped join and the explicit `e.item_id` key.
pub const SQL_AUTHORS: &str = "SELECT e.id,d.created_by document_author,p.created_by approved_author \
 FROM legal_executed_copies e JOIN legal_items scope ON scope.id=e.item_id \
 JOIN legal_documents d ON d.id=e.document_id \
 JOIN legal_documents p ON p.id=e.approved_document_id \
 WHERE e.id=$1 AND e.item_id=$2 AND scope.company_id=$3";

/// Ordered review history, Company-scoped through the immutable copy parent.
pub const SQL_REVIEWS: &str = "SELECT r.id,r.executed_copy_id,r.item_version::bigint item_version,r.decision,r.checks,r.reason,r.reviewer_id,r.created_at \
 FROM legal_executed_reviews r JOIN legal_executed_copies e ON e.id=r.executed_copy_id \
 JOIN legal_items scope ON scope.id=e.item_id \
 WHERE r.executed_copy_id=$1 AND scope.company_id=$2 ORDER BY r.created_at,r.id";

/// Explicit item membership, Company-scoped through the item parent.
pub const SQL_MEMBER: &str = "SELECT 1 FROM legal_members m \
 JOIN legal_items scope ON scope.id=m.item_id \
 WHERE m.item_id=$1 AND m.user_id=$2 AND scope.company_id=$3";

/// Latest approved cycle, Company-scoped through the item parent.
pub const SQL_APPROVAL: &str = "SELECT a.cycle::bigint cycle FROM legal_approvals a \
 JOIN legal_items scope ON scope.id=a.item_id \
 WHERE a.item_id=$1 AND a.decision='approved' AND scope.company_id=$2 \
 ORDER BY a.cycle DESC LIMIT 1";

/// Approved signing material for one Company. The projection and its order are
/// byte-identical to the retired owner statement; the limit is a bound value.
pub const SQL_TRUST_APPROVED: &str = "SELECT id,kind,content,checksum FROM legal_trust_material WHERE company_id=$1 AND state='approved' ORDER BY id LIMIT $2";

/// Bounded trust-material witness. The original `SELECT *` is narrowed in Rust
/// to the four fields the trust command actually consumes.
pub const SQL_TRUST_MATERIAL: &str = "SELECT id,state,version::bigint version,created_by FROM legal_trust_material WHERE id=$1 AND company_id=$2";

/// Pg-backed owner reader. Composition owns the pool and mounts the HTTP router.
pub struct PgExecutedCopySource {
    pool: DatabasePool,
}

impl PgExecutedCopySource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "NATIVE_LEGAL_COPY_READ_UNAVAILABLE",
        "The native Legal executed-copy service is unavailable.",
    )
}

impl ExecutedCopySource for PgExecutedCopySource {
    fn list<'a>(&'a self, company: &'a str, item: &'a str) -> LaneFuture<'a, Vec<CopyHead>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let item = item.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_LIST)
                .bind(item.as_str())
                .bind(company.as_str())
                .bind(COPY_LIMIT)
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|r| {
                    Ok(CopyHead {
                        id: r.try_get("id").map_err(|_| unavailable())?,
                        item_id: r.try_get("item_id").map_err(|_| unavailable())?,
                        document_id: r.try_get("document_id").map_err(|_| unavailable())?,
                        approved_cycle: r.try_get("approved_cycle").map_err(|_| unavailable())?,
                        approved_document_id: r
                            .try_get("approved_document_id")
                            .map_err(|_| unavailable())?,
                        approved_hash: r.try_get("approved_hash").map_err(|_| unavailable())?,
                        executed_hash: r.try_get("executed_hash").map_err(|_| unavailable())?,
                        metadata: r.try_get("metadata").map_err(|_| unavailable())?,
                        state: r.try_get("state").map_err(|_| unavailable())?,
                        imported_by: r.try_get("imported_by").map_err(|_| unavailable())?,
                        imported_at: r.try_get("imported_at").map_err(|_| unavailable())?,
                        reviewed_by: r.try_get("reviewed_by").map_err(|_| unavailable())?,
                        reviewed_at: r.try_get("reviewed_at").map_err(|_| unavailable())?,
                        version: r.try_get("version").map_err(|_| unavailable())?,
                        document_name: r.try_get("document_name").map_err(|_| unavailable())?,
                        document_version: r
                            .try_get("document_version")
                            .map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn authors<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
        copy: &'a str,
    ) -> LaneFuture<'a, Option<CopyAuthors>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let item = item.to_owned();
        let copy = copy.to_owned();
        Box::pin(async move {
            let row = sqlx::query(SQL_AUTHORS)
                .bind(copy.as_str())
                .bind(item.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|r| {
                Ok(CopyAuthors {
                    id: r.try_get("id").map_err(|_| unavailable())?,
                    document_author: r.try_get("document_author").map_err(|_| unavailable())?,
                    approved_author: r.try_get("approved_author").map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }

    fn reviews<'a>(
        &'a self,
        company: &'a str,
        copy: &'a str,
    ) -> LaneFuture<'a, Vec<(String, ReviewRow)>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let copy = copy.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_REVIEWS)
                .bind(copy.as_str())
                .bind(company.as_str())
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|r| {
                    let id: String = r.try_get("id").map_err(|_| unavailable())?;
                    let row = ReviewRow {
                        id: id.clone(),
                        executed_copy_id: r.try_get("executed_copy_id").map_err(|_| unavailable())?,
                        item_version: r.try_get("item_version").map_err(|_| unavailable())?,
                        decision: r.try_get("decision").map_err(|_| unavailable())?,
                        checks: r.try_get("checks").map_err(|_| unavailable())?,
                        reason: r.try_get("reason").map_err(|_| unavailable())?,
                        reviewer_id: r.try_get("reviewer_id").map_err(|_| unavailable())?,
                        created_at: r.try_get("created_at").map_err(|_| unavailable())?,
                    };
                    Ok((id, row))
                })
                .collect()
        })
    }

    fn member<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
        user: &'a str,
    ) -> LaneFuture<'a, bool> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let item = item.to_owned();
        let user = user.to_owned();
        Box::pin(async move {
            sqlx::query(SQL_MEMBER)
                .bind(item.as_str())
                .bind(user.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map(|row| row.is_some())
                .map_err(|_| unavailable())
        })
    }

    fn approval<'a>(
        &'a self,
        company: &'a str,
        item: &'a str,
    ) -> LaneFuture<'a, Option<i64>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let item = item.to_owned();
        Box::pin(async move {
            sqlx::query_scalar::<_, i64>(SQL_APPROVAL)
                .bind(item.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())
        })
    }
}

/// Approved OCR chunk, byte-identical to the retired owner statement apart
/// from the `$n` binds and the explicit bigint cast for the page column.
pub const SQL_CITATION_CHUNK: &str = "SELECT c.id,c.document_id,CASE WHEN c.page_mapping THEN c.page::bigint ELSE NULL END page,c.page_mapping,c.section,c.text,c.document_checksum FROM legal_source_chunks c JOIN legal_text_reviews r ON r.id=c.text_review_id AND r.state='approved' AND r.company_id=c.company_id AND r.item_id=c.item_id AND r.document_id=c.document_id AND r.document_checksum=c.document_checksum WHERE c.id=$1 AND c.company_id=$2";

/// Approved-text witness. The original point read gains a Company bind so the
/// owner never answers for another Company's review.
pub const SQL_TEXT_APPROVED: &str = "SELECT 1 FROM legal_text_reviews WHERE id=$1 AND document_id=$2 AND company_id=$3 AND state='approved'";

/// Pg-backed trust-material reader. Every query is Company-scoped by bind.
pub struct PgTrustMaterialSource {
    pool: DatabasePool,
}

impl PgTrustMaterialSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl TrustMaterialSource for PgTrustMaterialSource {
    fn approved<'a>(&'a self, company: &'a str) -> LaneFuture<'a, Vec<TrustMaterial>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        Box::pin(async move {
            let rows = sqlx::query(SQL_TRUST_APPROVED)
                .bind(company.as_str())
                .bind(TRUST_MATERIAL_LIMIT)
                .fetch_all(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            rows.into_iter()
                .map(|r| {
                    Ok(TrustMaterial {
                        id: r.try_get("id").map_err(|_| unavailable())?,
                        kind: r.try_get("kind").map_err(|_| unavailable())?,
                        content: r.try_get("content").map_err(|_| unavailable())?,
                        checksum: r.try_get("checksum").map_err(|_| unavailable())?,
                    })
                })
                .collect()
        })
    }

    fn material<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> LaneFuture<'a, Option<TrustMaterialDetail>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            sqlx::query_as::<_, (String, String, i64, String)>(SQL_TRUST_MATERIAL)
                .bind(id.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map(|row| {
                    row.map(|(id, state, version, created_by)| TrustMaterialDetail {
                        id,
                        state,
                        version,
                        created_by,
                    })
                })
                .map_err(|_| unavailable())
        })
    }
}

/// Pg-backed citation-evidence reader.
pub struct PgCitationEvidenceSource {
    pool: DatabasePool,
}

impl PgCitationEvidenceSource {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl CitationEvidenceSource for PgCitationEvidenceSource {
    fn chunk<'a>(
        &'a self,
        company: &'a str,
        id: &'a str,
    ) -> LaneFuture<'a, Option<CitationChunk>> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let id = id.to_owned();
        Box::pin(async move {
            let row = sqlx::query(SQL_CITATION_CHUNK)
                .bind(id.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map_err(|_| unavailable())?;
            row.map(|r| {
                Ok(CitationChunk {
                    id: r.try_get("id").map_err(|_| unavailable())?,
                    document_id: r.try_get("document_id").map_err(|_| unavailable())?,
                    page: r.try_get("page").map_err(|_| unavailable())?,
                    page_mapping: r.try_get("page_mapping").map_err(|_| unavailable())?,
                    section: r.try_get("section").map_err(|_| unavailable())?,
                    text: r.try_get("text").map_err(|_| unavailable())?,
                    document_checksum: r
                        .try_get("document_checksum")
                        .map_err(|_| unavailable())?,
                })
            })
            .transpose()
        })
    }

    fn text_approved<'a>(
        &'a self,
        company: &'a str,
        review: &'a str,
        document: &'a str,
    ) -> LaneFuture<'a, bool> {
        let pool = self.pool.clone();
        let company = company.to_owned();
        let review = review.to_owned();
        let document = document.to_owned();
        Box::pin(async move {
            sqlx::query(SQL_TEXT_APPROVED)
                .bind(review.as_str())
                .bind(document.as_str())
                .bind(company.as_str())
                .fetch_optional(pool.raw())
                .await
                .map(|row| row.is_some())
                .map_err(|_| unavailable())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hash(sql: &str) -> String {
        let mut d = Sha256::new();
        d.update(sql.as_bytes());
        hex_lower(d.finalize().as_slice())
    }

    fn hex_lower(bytes: &[u8]) -> String {
        const H: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push(H[(b >> 4) as usize] as char);
            s.push(H[(b & 15) as usize] as char);
        }
        s
    }

    #[test]
    fn statements_are_fixed_and_company_scoped() {
        for sql in [
            SQL_LIST,
            SQL_AUTHORS,
            SQL_REVIEWS,
            SQL_MEMBER,
            SQL_APPROVAL,
            SQL_TRUST_APPROVED,
            SQL_TRUST_MATERIAL,
            SQL_CITATION_CHUNK,
            SQL_TEXT_APPROVED,
        ] {
            assert!(sql.contains("company_id=$"), "missing Company scope: {sql}");
            assert!(!sql.contains('?'), "placeholder must be $n: {sql}");
        }
        // Rendered SQL oracles pin the exact statements executed by sqlx.
        for (sql, expected) in [
            (
                SQL_LIST,
                "687f0531b15421f2dc28d84d23ac1e7176e472217bfd954137d4181f1ad0d27c",
            ),
            (
                SQL_AUTHORS,
                "9c8463896c86a5084b8945ae332e307a86c18da0ac23937fea9115845ac7455e",
            ),
            (
                SQL_REVIEWS,
                "549689279491291555e7a0c01a3f387d205a3778b40b728cbc218e2bb974082c",
            ),
            (
                SQL_MEMBER,
                "79061b5f8de059ff9540239762df2aa59c87c38416adaf66b714e206a6592d18",
            ),
            (
                SQL_APPROVAL,
                "0270872d021e7f07258f022e544367d328219cab99d8cdeb64e16e31d1f96c4a",
            ),
            (
                SQL_TRUST_APPROVED,
                "3de96a4475f98d002869464a37ba051c75a2ec9ceba000a86305a21b0893db48",
            ),
            (
                SQL_TRUST_MATERIAL,
                "2f1d12f82c6c275cb3f78b008ec12fbfaca0a282a52d990f558f9ef42f5eb37e",
            ),
            (
                SQL_CITATION_CHUNK,
                "3ab2742c3d1567bd1246addd1d266956a6df118e7797fd62817420da6438200c",
            ),
            (
                SQL_TEXT_APPROVED,
                "94496468f3e030d95426815dc682e892cc54557694f2574177da3f41e7743ef5",
            ),
        ] {
            assert_eq!(hash(sql), expected);
        }
    }
}
