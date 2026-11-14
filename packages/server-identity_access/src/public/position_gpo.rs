//! Native Position GPO mapping and delivery owner port.
//!
//! The caller owns the serializable Holding transaction, the current Work
//! authority and the `pg_advisory_lock(821039320)` catalog lock. Directory
//! provider reads/effects stay behind [`PositionGpoDirectory`]; this crate
//! never bypasses the installed connector profile.
pub use crate::adapters::position_gpo_postgres::PgPositionGpo;
#[cfg(feature = "position-gpo-connector")]
pub use crate::adapters::position_gpo_directory::{
    ConnectorRequest, ConnectorReply, ConnectorTransport, PgPositionGpoDirectory,
    PositionGpoConnector, ReqwestConnector,
};
pub use crate::adapters::position_gpo_http::{
    position_gpo_http_router, PositionGpoHttpBackend, PositionGpoHttpCommand,
    PositionGpoHttpFuture, PositionGpoHttpRequest, PositionGpoHttpResponse, PositionGpoHttpTarget,
};
pub use crate::application::position_gpo::{
    configure_gpos, drain_position_gpo_locked, position_gpo_overview, preview_position_gpo,
    preview_unmap_position_gpo, publish_position_gpo, reconcile_position_gpo, retry_position_gpo,
    retry_position_gpo_config, unmap_position_gpo, EffectiveRule, GpoFuture, PositionGpoDirectory,
    PositionGpoRepository, PublishPlan, UnmapPlan,
};
pub use crate::domain::iam_roles::IamRoleActor;
pub use crate::domain::position_gpo::{
    delivery_id, inventory_filters, inventory_links, inventory_object, membership_key,
    preview_token, rule_source_id, sha256_hex, waiting_key, ConfigRetryInput, ConnectionRow,
    DesiredRow, FreshMfa, GpoAction, GpoAuthority, HolderRow, Inventory, InventoryObject,
    JobRetryInput, JobRow, MappingRow, PositionGpoInput, PublishInput, UnmapInput,
    UnmapPublishInput, CONFIG_RETRY_OPERATION, JOB_RETRY_OPERATION, PUBLISH_OPERATION,
    UNMAP_OPERATION,
};
