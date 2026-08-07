use super::{
    models::{
        AccessEntryItem, AccessEntryItems, ContractItem, ContractItems, ContractPartyItem,
        DocumentItem, DocumentItems, DocumentVersionItem,
    },
    presenter::LegalContent,
};
use async_trait::async_trait;
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_legal::ports::{
    AccessEntryView, AccessHistoryList, ContractList, ContractView, DocumentList, DocumentView,
};
use erpx_legal::{
    ContractStatus, LegalCommand, LegalMutationResult, LegalQuery, LegalQueryResponse,
};
use erpx_ui_foundation::{CommandGateway, QueryGateway, UiError};

pub fn present_mutation(value: &LegalMutationResult) -> LegalContent {
    match value {
        LegalMutationResult::Contract(v) => LegalContent::Contract(v.clone().into()),
        LegalMutationResult::Document(v) => LegalContent::Document(v.clone().into()),
        LegalMutationResult::AccessEntry(v) => LegalContent::AccessEntry(v.clone().into()),
    }
}
pub fn present_query(value: QueryResult<LegalQueryResponse>) -> QueryResult<Option<LegalContent>> {
    let metadata = value.metadata;
    let value = match value.value {
        LegalQueryResponse::Contract(v) => v.map(ContractItem::from).map(LegalContent::Contract),
        LegalQueryResponse::Contracts(v) => {
            let items = ContractItems::from(v);
            (!items.contracts.is_empty())
                .then_some(items)
                .map(LegalContent::Contracts)
        }
        LegalQueryResponse::Document(v) => v.map(DocumentItem::from).map(LegalContent::Document),
        LegalQueryResponse::Documents(v) => {
            let items = DocumentItems::from(v);
            (!items.documents.is_empty())
                .then_some(items)
                .map(LegalContent::Documents)
        }
        LegalQueryResponse::AccessHistory(v) => {
            let items = AccessEntryItems::from(v);
            (!items.entries.is_empty())
                .then_some(items)
                .map(LegalContent::AccessHistory)
        }
    };
    QueryResult { value, metadata }
}

impl From<ContractView> for ContractItem {
    fn from(v: ContractView) -> Self {
        Self {
            contract_id: v.contract_id.as_uuid().to_string(),
            version: v.version,
            scope: format!("{:?}", v.scope),
            reference: v.reference,
            title: v.title,
            object_ref: v.object_ref,
            content_sha256: v.content_sha256,
            retention_until: v.retention_until.to_rfc3339(),
            status: status(v.status).into(),
            parties: v
                .parties
                .into_iter()
                .map(|p| ContractPartyItem {
                    party_id: p.party_id.to_string(),
                    role: p.role,
                })
                .collect(),
            created_by: v.created_by,
            approved_by: v.approved_by,
            created_at: v.created_at.to_rfc3339(),
            updated_at: v.updated_at.to_rfc3339(),
        }
    }
}
impl From<ContractList> for ContractItems {
    fn from(v: ContractList) -> Self {
        Self {
            contracts: v.contracts.into_iter().map(ContractItem::from).collect(),
            next_cursor: v.next_cursor,
        }
    }
}
impl From<DocumentView> for DocumentItem {
    fn from(v: DocumentView) -> Self {
        Self {
            document_id: v.document_id.as_uuid().to_string(),
            version: v.version,
            scope: format!("{:?}", v.scope),
            title: v.title,
            versions: v
                .versions
                .into_iter()
                .map(|x| DocumentVersionItem {
                    version: x.version,
                    object_ref: x.object_ref,
                    content_sha256: x.content_sha256,
                    created_at: x.created_at.to_rfc3339(),
                    created_by: x.created_by,
                })
                .collect(),
            current_version: v.current_version,
            archived: v.archived,
            created_at: v.created_at.to_rfc3339(),
            updated_at: v.updated_at.to_rfc3339(),
        }
    }
}
impl From<DocumentList> for DocumentItems {
    fn from(v: DocumentList) -> Self {
        Self {
            documents: v.documents.into_iter().map(DocumentItem::from).collect(),
            next_cursor: v.next_cursor,
        }
    }
}
impl From<AccessEntryView> for AccessEntryItem {
    fn from(v: AccessEntryView) -> Self {
        Self {
            access_entry_id: v.access_entry_id.as_uuid().to_string(),
            version: v.version,
            scope: format!("{:?}", v.scope),
            document_id: v.document_id.as_uuid().to_string(),
            document_version: v.document_version,
            actor_ref: v.actor_ref,
            action: v.action,
            purpose: v.purpose,
            observed_at: v.observed_at.to_rfc3339(),
        }
    }
}
impl From<AccessHistoryList> for AccessEntryItems {
    fn from(v: AccessHistoryList) -> Self {
        Self {
            entries: v.entries.into_iter().map(AccessEntryItem::from).collect(),
            next_cursor: v.next_cursor,
        }
    }
}
const fn status(v: ContractStatus) -> &'static str {
    match v {
        ContractStatus::Draft => "draft",
        ContractStatus::Review => "review",
        ContractStatus::Approved => "approved",
        ContractStatus::Active => "active",
        ContractStatus::Expired => "expired",
        ContractStatus::Terminated => "terminated",
        ContractStatus::Archived => "archived",
    }
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait LegalProtoTransport: Send + Sync {
    async fn command(
        &self,
        envelope: CommandEnvelope<LegalCommand>,
    ) -> Result<CommandReceipt<LegalMutationResult>, UiError>;
    async fn query(
        &self,
        envelope: QueryEnvelope<LegalQuery>,
    ) -> Result<QueryResult<LegalQueryResponse>, UiError>;
}
#[derive(Debug, Clone)]
pub struct LegalGateway<T> {
    transport: T,
}
impl<T> LegalGateway<T> {
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
    pub const fn transport(&self) -> &T {
        &self.transport
    }
    pub fn into_transport(self) -> T {
        self.transport
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: LegalProtoTransport> CommandGateway<LegalCommand> for LegalGateway<T> {
    type Response = LegalMutationResult;
    async fn execute(
        &self,
        envelope: CommandEnvelope<LegalCommand>,
    ) -> Result<CommandReceipt<Self::Response>, UiError> {
        self.transport.command(envelope).await
    }
}
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<T: LegalProtoTransport> QueryGateway<LegalQuery> for LegalGateway<T> {
    type Response = LegalQueryResponse;
    async fn execute(
        &self,
        envelope: QueryEnvelope<LegalQuery>,
    ) -> Result<QueryResult<Self::Response>, UiError> {
        self.transport.query(envelope).await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake;
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    impl LegalProtoTransport for Fake {
        async fn command(
            &self,
            _: CommandEnvelope<LegalCommand>,
        ) -> Result<CommandReceipt<LegalMutationResult>, UiError> {
            unreachable!()
        }
        async fn query(
            &self,
            _: QueryEnvelope<LegalQuery>,
        ) -> Result<QueryResult<LegalQueryResponse>, UiError> {
            unreachable!()
        }
    }
    #[test]
    fn gateway_is_typed_in_both_directions() {
        fn c<G: CommandGateway<LegalCommand, Response = LegalMutationResult>>() {}
        fn q<G: QueryGateway<LegalQuery, Response = LegalQueryResponse>>() {}
        c::<LegalGateway<Fake>>();
        q::<LegalGateway<Fake>>();
    }
    #[test]
    fn only_gateway_imports_core() {
        let source = include_str!("gateway.rs");
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("tonic", "::")));
        for source in [
            include_str!("models.rs"),
            include_str!("presenter.rs"),
            include_str!("view.rs"),
        ] {
            assert!(!source.contains(concat!("erpx_", "legal")));
        }
    }
}
