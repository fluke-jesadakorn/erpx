//! Native Legal specialized-attachment backfill contract. The external read
//! reuses the r44 join head; the document read and hold witness are owner-owned.
pub use crate::adapters::attachment_migration_facts::{
    ATTACHMENT_MIGRATION_FACTS_SOURCES_JSON, AttachmentMigrationFactsSources,
    attachment_hold_exists_sql, attachment_migration_facts_sources,
    render_attachment_migration_documents,
};
