//! PostgreSQL read adapter for the native Customer 360 timeline activity port.
//!
//! The SQL reproduces the retained TypeScript `customerTimelineEvents`
//! federation over the source owners' existing relations read-only: the same
//! scoped orders, leads, service, invoices and legal references, the same
//! per-source event shapes and the same customer party join. It adds a bounded
//! one-year window, an optional selected customer and a hard row ceiling. The
//! caller owns the retained serializable Holding transaction; every source is
//! read inside that one snapshot.
use crate::adapters::timeline::FinalRootEventSources;
use crate::application::timeline::CustomerTimelineFuture;
use crate::application::timeline_activity::CustomerTimelineActivityRepository;
use crate::domain::timeline::{CustomerTimelineActor, CustomerTimelineView};
use crate::domain::timeline_activity::{
    CustomerTimelineActivityQuery, CustomerTimelineActivityRow,
};
use erpx_platform_runtime_server::public::postgres::{PersistenceError, PgTransactionError};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

use erpx_platform_shared_server::public::BusinessError;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CUSTOMER_TIMELINE_ACTIVITY_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(
            PersistenceError::new("CUSTOMER_TIMELINE_ACTIVITY_SERIALIZABLE_REQUIRED").into(),
        );
    }
    Ok(())
}

/// Bounded federated activity reader over a caller-owned serializable unit.
pub struct PgCustomerTimelineActivityRead<'a> {
    connection: &'a mut PgConnection,
    sources: FinalRootEventSources,
}

impl<'a> PgCustomerTimelineActivityRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self::with_sources(connection, FinalRootEventSources::DEFAULT)
    }

    /// Use the fixed Field event statements from a current final-root witness.
    pub fn with_sources(connection: &'a mut PgConnection, sources: FinalRootEventSources) -> Self {
        Self { connection, sources }
    }

    pub async fn activity(
        &mut self,
        actor: &CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &CustomerTimelineActivityQuery,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineActivityRow>, Error> {
        serializable(&mut *self.connection).await?;
        let sources = self.sources;
        let sql = format!(
            "WITH params AS (
               SELECT $1::text company_id,$2::text branch_id,$3::text department_id,
               $4::bigint from_at,$5::bigint to_at,$6::text customer_id,$7::text query,
               $8::bigint limit_n,$9::boolean support,$10::boolean field_service,
               $11::boolean finance,$12::boolean legal,$13::text user_id,
               $14::text role,$15::text assignment_id
             ), scoped_orders AS (
               SELECT o.id,o.company_id,o.party_id,o.kind,o.number,o.state,o.version,o.updated_at
               FROM commerce_orders o, params p
               WHERE o.company_id=p.company_id AND (p.branch_id IS NULL OR o.branch_id=p.branch_id)
               AND (p.department_id IS NULL OR o.department_id=p.department_id)
               AND o.kind IN ('sales','quote')
             ), scoped_leads AS (
               SELECT l.id,l.title,l.status,l.version,l.created_at,l.updated_at,v.customer_id
               FROM crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id, params p
               WHERE l.company_id=p.company_id AND (p.branch_id IS NULL OR l.branch_id=p.branch_id)
               AND (p.department_id IS NULL OR l.department_id=p.department_id)
             ), scoped_service AS (
               SELECT g.id,g.customer_id,g.title FROM field_service_agreements g, params p
               WHERE g.company_id=p.company_id AND (p.branch_id IS NULL OR g.branch_id=p.branch_id)
               AND (p.department_id IS NULL OR g.department_id=p.department_id)
             ), scoped_invoices AS (
               SELECT i.id,i.party_id,i.invoice_number,i.status,i.version,i.updated_at
               FROM finance_invoices i, params p WHERE i.company_id=p.company_id
             ), scoped_legal AS (
               SELECT i.id,i.title,n.target_id order_id FROM legal_items i JOIN legal_native_references n ON n.item_id=i.id, params p
               WHERE i.company_id=p.company_id AND (p.branch_id IS NULL OR i.branch_id=p.branch_id)
               AND (p.department_id IS NULL OR i.department_id=p.department_id)
               AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=p.user_id))
               AND (p.role <> 'legal_requester' OR (i.kind='request' AND i.created_by=p.user_id))
               AND n.target_module='commerce_order'
             ), customers AS (
               SELECT f.id,f.name FROM finance_parties f, params p
               WHERE f.company_id=p.company_id AND f.kind='customer'
             ), events AS (
               SELECT 'order:'||o.id id,o.id source_id,'order' source_type,
               o.kind||' · '||o.number title,o.state status,o.version,(o.updated_at)::bigint at,
               '/commerce/orders/'||o.id href,o.party_id customer_id,c.name customer_name
               FROM scoped_orders o JOIN customers c ON c.id=o.party_id
               UNION ALL
               SELECT 'fulfillment:'||f.id,f.id,'fulfillment','Fulfillment · '||f.number,'recorded',
               f.version,(f.created_at)::bigint,'/commerce/orders/'||o.id,o.party_id,c.name
               FROM commerce_fulfillments f JOIN scoped_orders o ON o.id=f.order_id JOIN customers c ON c.id=o.party_id
               UNION ALL
               SELECT 'return:'||r.id,r.id,'return','Customer return · '||r.number,r.status,r.version,
               (r.updated_at)::bigint,'/commerce/orders/'||o.id,o.party_id,c.name
               FROM commerce_returns r JOIN scoped_orders o ON o.id=r.order_id AND o.company_id=r.company_id
               JOIN customers c ON c.id=o.party_id WHERE r.kind='customer_return'
               UNION ALL
               SELECT 'contact:'||h.id,n.id,'contact','Contact · '||n.channel,h.action,h.version,
               (h.created_at)::bigint,'/commerce/orders/'||o.id||'/notes',o.party_id,c.name
               FROM customer_contact_note_history h JOIN customer_contact_notes n ON n.id=h.note_id
               JOIN scoped_orders o ON o.id=n.order_id AND o.company_id=n.company_id
               JOIN customers c ON c.id=o.party_id
               UNION ALL
               SELECT 'followup:'||e.id,c2.id,'followup','Customer follow-up',e.action,e.version,
               (e.created_at)::bigint,'/customer-followups/'||c2.id,o.party_id,c.name
               FROM customer_followup_events e JOIN customer_followups c2 ON c2.id=e.followup_id
               JOIN scoped_orders o ON o.id=c2.order_id AND o.company_id=c2.company_id
               JOIN customers c ON c.id=o.party_id
               UNION ALL
               SELECT 'support:'||e.id,t.id,'support',t.title,e.action,e.version,(e.created_at)::bigint,
               '/support/'||t.id,o.party_id,c.name
               FROM support_ticket_events e JOIN support_tickets t ON t.id=e.ticket_id
               JOIN scoped_orders o ON o.id=t.order_id AND o.company_id=t.company_id
               JOIN customers c ON c.id=o.party_id, params p
               WHERE p.support AND EXISTS(SELECT 1 FROM assignments g
                 WHERE g.id=p.assignment_id AND g.user_id=p.user_id AND g.role=p.role
                 AND g.company_id=t.company_id
                 AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
                 AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
                 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id
                   OR g.scope_kind='department' AND g.scope_node_id=t.department_id)
                 AND (iam_role_allows(g.role,g.company_id,'support.read')
                   OR t.source_kind<>'sales_order' AND t.created_by=g.user_id
                   AND iam_role_allows(g.role,g.company_id,'records.read')))
               UNION ALL
               SELECT 'service-agreement:'||e.id,g.id,'field_service',
               'Field service agreement · '||g.title,e.action,e.version,(e.created_at)::bigint,
               '/field-service',g.customer_id,c.name
               FROM ({}) e JOIN field_service_agreements g ON g.id=e.agreement_id
               JOIN scoped_service sg ON sg.id=g.id JOIN customers c ON c.id=g.customer_id, params p
               WHERE p.field_service
               UNION ALL
               SELECT 'service-order:'||e.id,o.id,'field_service','Field service · '||t.title,
               e.action,e.version,(e.created_at)::bigint,'/field-service/'||o.id,so.party_id,c.name
               FROM ({}) e JOIN field_service_orders o ON o.id=e.order_id
               JOIN support_tickets t ON t.id=o.ticket_id
               JOIN scoped_orders so ON so.id=t.order_id AND so.company_id=t.company_id
               JOIN customers c ON c.id=so.party_id, params p
               WHERE p.field_service
               UNION ALL
               SELECT 'legal:'||e.id,i.id,'legal','Legal · '||i.title,e.action,e.version,
               (e.created_at)::bigint,'/legal',o.party_id,c.name
               FROM legal_events e JOIN scoped_legal i ON i.id=e.item_id
               JOIN scoped_orders o ON o.id=i.order_id JOIN customers c ON c.id=o.party_id, params p
               WHERE p.legal
               UNION ALL
               SELECT 'invoice:'||i.id,i.id,'invoice','Invoice · '||i.invoice_number,i.status,i.version,
               (i.updated_at)::bigint,'/finance/invoices/'||i.id,i.party_id,c.name
               FROM finance_invoices i JOIN scoped_invoices si ON si.id=i.id
               JOIN customers c ON c.id=i.party_id, params p
               WHERE p.finance
               UNION ALL
               SELECT 'crm:'||e.id,l.id,'crm',l.title,e.action,e.version,(e.created_at)::bigint,
               '/crm/'||l.id,l.customer_id,c.name
               FROM crm_case_events e JOIN scoped_leads l ON l.id=e.lead_id
               JOIN customers c ON c.id=l.customer_id WHERE e.opportunity_id IS NULL
               UNION ALL
               SELECT 'crm-activity:'||x.id,x.id,'crm_activity','CRM activity · '||x.activity_type,
               x.status,x.version,(x.updated_at)::bigint,'/crm/'||l.id||'#crm-activities',
               l.customer_id,c.name
               FROM crm_lead_activities x JOIN scoped_leads l ON l.id=x.lead_id
               JOIN customers c ON c.id=l.customer_id
             )
             SELECT e.id,e.source_id,e.source_type,e.title,e.status,e.version,e.at,e.href,
             e.customer_id,e.customer_name FROM events e, params p
             WHERE e.at>=p.from_at AND e.at<p.to_at
             AND (p.customer_id IS NULL OR e.customer_id=p.customer_id)
             AND (p.query IS NULL OR p.query='' OR position(lower(p.query) in lower(concat_ws(' ',e.id,e.source_id,e.source_type,e.title,e.status,e.customer_name)))>0)
             ORDER BY e.at DESC,e.id DESC LIMIT $8",
            sources.agreement_events,
            sources.order_events,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(query.from)
            .bind(query.to)
            .bind(query.customer_id.as_deref())
            .bind(query.query.as_deref())
            .bind(limit as i64)
            .bind(view.support)
            .bind(view.field_service)
            .bind(view.finance)
            .bind(view.legal)
            .bind(actor.user_id.as_str())
            .bind(actor.role.as_str())
            .bind(actor.assignment_id.as_str())
            .fetch_all(&mut *self.connection)
            .await?;
        rows.into_iter()
            .map(|row| {
                let item = CustomerTimelineActivityRow {
                    id: row.try_get("id")?,
                    source_id: row.try_get("source_id")?,
                    source_type: row.try_get("source_type")?,
                    title: row.try_get("title")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    at: row.try_get("at")?,
                    href: row.try_get("href")?,
                    customer_id: row.try_get("customer_id")?,
                    customer_name: row.try_get("customer_name")?,
                };
                item.validate().map_err(PgTransactionError::Business)?;
                Ok(item)
            })
            .collect()
    }
}

impl CustomerTimelineActivityRepository for PgCustomerTimelineActivityRead<'_> {
    type Error = Error;

    fn activity<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &'a CustomerTimelineActivityQuery,
        limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<CustomerTimelineActivityRow>, Self::Error> {
        Box::pin(async move {
            PgCustomerTimelineActivityRead::activity(self, actor, view, query, limit).await
        })
    }
}
