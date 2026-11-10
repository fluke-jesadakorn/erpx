//! PostgreSQL read adapter for the native Customer 360 federated timeline.
//!
//! The SQL is the retained Customer 360 federation already exercised by the
//! TypeScript owner integration test: it joins the source owners' existing
//! relations read-only and adds no relation, column, write or cross-owner
//! mutation. The caller owns the retained serializable Holding transaction;
//! every source query runs inside that one snapshot so the visible customer and
//! event page cannot observe mixed owner states.
use crate::application::timeline::{
    CustomerTimelineFuture, CustomerTimelineRepository, ExecutiveCustomerActivityRepository,
};
use crate::domain::timeline::{
    CustomerAccount, CustomerTimelineActor, CustomerTimelineCursor, CustomerTimelineItem,
    CustomerTimelineView, ExecutiveCustomerActivityQuery, ExecutiveCustomerActivityRow,
    source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CUSTOMER_TIMELINE_READ_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CUSTOMER_TIMELINE_READ_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCustomerTimelineRead<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCustomerTimelineRead<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn customer(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        view: CustomerTimelineView,
    ) -> Result<Option<CustomerAccount>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "WITH scoped_orders AS (\
               SELECT o.id,o.party_id FROM commerce_orders o \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) AND o.kind IN ('sales','quote')\
             ), scoped_leads AS (\
               SELECT l.id,v.customer_id FROM crm_leads l \
               JOIN crm_opportunities v ON v.lead_id=l.id \
               WHERE l.company_id=$1 AND ($2::text IS NULL OR l.branch_id=$2) \
               AND ($3::text IS NULL OR l.department_id=$3)\
             ), scoped_service AS (\
               SELECT g.id,g.customer_id FROM field_service_agreements g \
               WHERE $5::boolean AND g.company_id=$1 AND ($2::text IS NULL OR g.branch_id=$2) \
               AND ($3::text IS NULL OR g.department_id=$3)\
             ), scoped_invoices AS (\
               SELECT i.id,i.party_id FROM finance_invoices i \
               WHERE $6::boolean AND i.company_id=$1\
             ), scoped_legal AS (\
               SELECT i.id,n.target_id order_id FROM legal_items i \
               JOIN legal_native_references n ON n.item_id=i.id \
               WHERE $7::boolean AND i.company_id=$1 AND ($2::text IS NULL OR i.branch_id=$2) \
               AND ($3::text IS NULL OR i.department_id=$3) \
               AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$8)) \
               AND ($9::text <> 'legal_requester' OR (i.kind='request' AND i.created_by=$8)) \
               AND n.target_module='commerce_order'\
             ) \
             SELECT p.id,p.name,p.code FROM finance_parties p \
             WHERE p.id=$4 AND p.company_id=$1 AND p.kind='customer' \
             AND (EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_service g WHERE g.customer_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_invoices i WHERE i.party_id=p.id) \
               OR EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (SELECT id FROM scoped_orders so WHERE so.party_id=p.id))) \
             LIMIT 2",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
        .bind(actor.department_id.as_ref().map(|value| value.as_str()))
        .bind(customer_id)
        .bind(view.field_service)
        .bind(view.finance)
        .bind(view.legal)
        .bind(actor.user_id.as_str())
        .bind(actor.role.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            Ok(CustomerAccount {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                code: row.try_get("code")?,
            })
        })
        .transpose()
    }

    pub async fn events(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        view: CustomerTimelineView,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        serializable(&mut *self.connection).await?;
        let mut items = Vec::new();
        items.extend(self.orders(actor, customer_id, cursor, limit).await?);
        items.extend(self.fulfillments(actor, customer_id, cursor, limit).await?);
        items.extend(self.returns(actor, customer_id, cursor, limit).await?);
        items.extend(self.contacts(actor, customer_id, cursor, limit).await?);
        items.extend(self.followups(actor, customer_id, cursor, limit).await?);
        if view.support {
            items.extend(self.support(actor, customer_id, cursor, limit).await?);
        }
        if view.field_service {
            items.extend(
                self.service_agreements(actor, customer_id, cursor, limit)
                    .await?,
            );
            items.extend(
                self.service_orders(actor, customer_id, cursor, limit)
                    .await?,
            );
        }
        if view.legal {
            items.extend(self.legal(actor, customer_id, cursor, limit).await?);
        }
        if view.finance {
            items.extend(self.invoices(actor, customer_id, cursor, limit).await?);
        }
        items.extend(self.crm_events(actor, customer_id, cursor, limit).await?);
        items.extend(
            self.crm_activities(actor, customer_id, cursor, limit)
                .await?,
        );
        // Source queries are individually bounded and ordered by PostgreSQL. Merge
        // those candidates with the same database collation used by their cursor
        // predicates; Rust string ordering is not PostgreSQL collation ordering.
        let times: Vec<i64> = items.iter().map(|item| item.at).collect();
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        let ordinals: Vec<i64> = sqlx::query_scalar(
            "SELECT ordinal FROM unnest($1::bigint[], $2::text[]) WITH ORDINALITY AS e(at,id,ordinal) \
             WHERE ($3::bigint IS NULL OR e.at < $3 OR (e.at = $3 AND e.id < $4)) \
             ORDER BY e.at DESC, e.id DESC, e.ordinal",
        )
        .bind(&times)
        .bind(&ids)
        .bind(cursor.map(|value| value.at))
        .bind(cursor.map(|value| value.id.as_str()))
        .fetch_all(&mut *self.connection)
        .await?;
        if ordinals.len() != items.len() {
            return Err(PgTransactionError::Business(source_invalid()));
        }
        let mut slots: Vec<Option<CustomerTimelineItem>> = items.into_iter().map(Some).collect();
        let mut ordered = Vec::with_capacity(slots.len());
        for ordinal in ordinals {
            let index = usize::try_from(ordinal).ok().and_then(|n| n.checked_sub(1));
            let item = index
                .and_then(|n| slots.get_mut(n))
                .and_then(Option::take)
                .ok_or_else(|| PgTransactionError::Business(source_invalid()))?;
            ordered.push(item);
        }
        Ok(ordered)
    }

    #[allow(clippy::too_many_arguments)]
    async fn orders(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'order:'||o.id id,o.id source_id,'order' source_type,\
               o.kind||' · '||o.number title,o.state status,o.version,(o.updated_at)::bigint at,\
               '/commerce/orders/'||o.id href \
               FROM commerce_orders o \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn fulfillments(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'fulfillment:'||f.id id,f.id source_id,'fulfillment' source_type,\
               'Fulfillment · '||f.number title,'recorded' status,f.version,(f.created_at)::bigint at,\
               '/commerce/orders/'||o.id href \
               FROM commerce_fulfillments f \
               JOIN commerce_orders o ON o.id=f.order_id \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn returns(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'return:'||r.id id,r.id source_id,'return' source_type,\
               'Customer return · '||r.number title,r.status,r.version,(r.updated_at)::bigint at,\
               '/commerce/orders/'||o.id href \
               FROM commerce_returns r \
               JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4 AND r.kind='customer_return'\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn contacts(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'contact:'||h.id id,n.id source_id,'contact' source_type,\
               'Contact · '||n.channel title,h.action status,h.version,(h.created_at)::bigint at,\
               '/commerce/orders/'||o.id||'/notes' href \
               FROM customer_contact_note_history h \
               JOIN customer_contact_notes n ON n.id=h.note_id \
               JOIN commerce_orders o ON o.id=n.order_id AND o.company_id=n.company_id \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn followups(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'followup:'||e.id id,c.id source_id,'followup' source_type,\
               'Customer follow-up' title,e.action status,e.version,(e.created_at)::bigint at,\
               '/customer-followups/'||c.id href \
               FROM customer_followup_events e \
               JOIN customer_followups c ON c.id=e.followup_id \
               JOIN commerce_orders o ON o.id=c.order_id AND o.company_id=c.company_id \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn support(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'support:'||e.id id,t.id source_id,'support' source_type,\
               t.title,e.action status,e.version,(e.created_at)::bigint at,\
               '/support/'||t.id href \
               FROM support_ticket_events e \
               JOIN support_tickets t ON t.id=e.ticket_id \
               JOIN commerce_orders o ON o.id=t.order_id AND o.company_id=t.company_id \
               WHERE o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4 \
               AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=$8 AND g.user_id=$9 AND g.role=$10 \
                 AND g.company_id=t.company_id \
                 AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
                 AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) \
                 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=t.branch_id \
                   OR g.scope_kind='department' AND g.scope_node_id=t.department_id) \
                 AND (iam_role_allows(g.role,g.company_id,'support.read') \
                   OR t.source_kind<>'sales_order' AND t.created_by=g.user_id \
                   AND iam_role_allows(g.role,g.company_id,'records.read')))\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[
                actor.assignment_id.as_str(),
                actor.user_id.as_str(),
                actor.role.as_str(),
            ],
        )
        .await
    }

    async fn service_agreements(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'service-agreement:'||e.id id,g.id source_id,'field_service' source_type,\
               'Field service agreement · '||g.title title,e.action status,e.version,(e.created_at)::bigint at,\
               '/field-service' href \
               FROM field_service_agreement_events e \
               JOIN field_service_agreements g ON g.id=e.agreement_id \
               WHERE g.company_id=$1 AND ($2::text IS NULL OR g.branch_id=$2) \
               AND ($3::text IS NULL OR g.department_id=$3) AND g.customer_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn service_orders(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'service-order:'||e.id id,o.id source_id,'field_service' source_type,\
               'Field service · '||t.title title,e.action status,e.version,(e.created_at)::bigint at,\
               '/field-service/'||o.id href \
               FROM field_service_order_events e \
               JOIN field_service_orders o ON o.id=e.order_id \
               JOIN support_tickets t ON t.id=o.ticket_id \
               JOIN commerce_orders so ON so.id=t.order_id AND so.company_id=t.company_id \
               WHERE so.company_id=$1 AND ($2::text IS NULL OR so.branch_id=$2) \
               AND ($3::text IS NULL OR so.department_id=$3) \
               AND so.kind IN ('sales','quote') AND so.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn legal(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT DISTINCT * FROM (\
               SELECT 'legal:'||e.id id,i.id source_id,'legal' source_type,\
               'Legal · '||i.title title,e.action status,(e.version)::bigint version,(e.created_at)::bigint at,\
               '/legal' href \
               FROM legal_events e \
               JOIN legal_items i ON i.id=e.item_id \
               JOIN legal_native_references n ON n.item_id=i.id AND n.target_module='commerce_order' \
               JOIN commerce_orders o ON o.id=n.target_id AND o.company_id=i.company_id \
               WHERE i.company_id=$1 AND ($2::text IS NULL OR i.branch_id=$2) \
               AND ($3::text IS NULL OR i.department_id=$3) \
               AND (i.classification='internal' OR EXISTS(SELECT 1 FROM legal_members lm WHERE lm.item_id=i.id AND lm.user_id=$8)) \
               AND ($9::text <> 'legal_requester' OR (i.kind='request' AND i.created_by=$8)) \
               AND o.company_id=$1 AND ($2::text IS NULL OR o.branch_id=$2) \
               AND ($3::text IS NULL OR o.department_id=$3) \
               AND o.kind IN ('sales','quote') AND o.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[actor.user_id.as_str(), actor.role.as_str()],
        )
        .await
    }

    async fn invoices(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'invoice:'||i.id id,i.id source_id,'invoice' source_type,\
               'Invoice · '||i.invoice_number title,i.status,i.version,(i.updated_at)::bigint at,\
               '/finance/invoices/'||i.id href \
               FROM finance_invoices i \
               WHERE i.company_id=$1 AND i.party_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn crm_events(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'crm:'||e.id id,l.id source_id,'crm' source_type,\
               l.title,e.action status,e.version,(e.created_at)::bigint at,\
               '/crm/'||l.id href \
               FROM crm_case_events e \
               JOIN crm_leads l ON l.id=e.lead_id \
               JOIN crm_opportunities v ON v.lead_id=l.id \
               WHERE l.company_id=$1 AND ($2::text IS NULL OR l.branch_id=$2) \
               AND ($3::text IS NULL OR l.department_id=$3) \
               AND v.customer_id=$4 AND e.opportunity_id IS NULL\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    async fn crm_activities(
        &mut self,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        self.read(
            "SELECT * FROM (\
               SELECT 'crm-activity:'||x.id id,x.id source_id,'crm_activity' source_type,\
               'CRM activity · '||x.activity_type title,x.status,x.version,(x.updated_at)::bigint at,\
               '/crm/'||l.id||'#crm-activities' href \
               FROM crm_lead_activities x \
               JOIN crm_leads l ON l.id=x.lead_id \
               JOIN crm_opportunities v ON v.lead_id=l.id \
               WHERE l.company_id=$1 AND ($2::text IS NULL OR l.branch_id=$2) \
               AND ($3::text IS NULL OR l.department_id=$3) AND v.customer_id=$4\
             ) e WHERE ($5::bigint IS NULL OR e.at<$5 OR (e.at=$5 AND e.id<$6)) \
             ORDER BY e.at DESC,e.id DESC LIMIT $7",
            actor,
            customer_id,
            cursor,
            limit,
            &[],
        )
        .await
    }

    /// Extra bindings are appended after the common seven parameters.
    async fn read(
        &mut self,
        sql: &'static str,
        actor: &CustomerTimelineActor,
        customer_id: &str,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
        extra: &[&str],
    ) -> Result<Vec<CustomerTimelineItem>, Error> {
        let mut query = sqlx::query(sql)
            .bind(actor.company_id.as_str())
            .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
            .bind(actor.department_id.as_ref().map(|value| value.as_str()))
            .bind(customer_id)
            .bind(cursor.map(|value| value.at))
            .bind(cursor.map(|value| value.id.as_str()))
            .bind(limit as i64);
        for value in extra {
            query = query.bind(*value);
        }
        let rows = query.fetch_all(&mut *self.connection).await?;
        rows.into_iter()
            .map(|row| {
                let item = CustomerTimelineItem {
                    id: row.try_get("id")?,
                    source_id: row.try_get("source_id")?,
                    source_type: row.try_get("source_type")?,
                    title: row.try_get("title")?,
                    status: row.try_get("status")?,
                    version: row.try_get("version")?,
                    at: row.try_get("at")?,
                    href: row.try_get("href")?,
                };
                item.validate().map_err(PgTransactionError::Business)?;
                Ok(item)
            })
            .collect()
    }
}

impl PgCustomerTimelineRead<'_> {
    /// Cross-customer Executive activity search over the same source owners as
    /// the per-customer timeline. The caller retains the serializable unit.
    pub async fn executive_activity(
        &mut self,
        actor: &CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &ExecutiveCustomerActivityQuery,
        cursor: Option<&CustomerTimelineCursor>,
        limit: usize,
    ) -> Result<Vec<ExecutiveCustomerActivityRow>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "WITH params AS (
               SELECT $1::text company_id,$2::text branch_id,$3::text department_id,
               $4::bigint from_at,$5::bigint to_at,$6::text query,$7::bigint cursor_at,
               $8::text cursor_id,$9::bigint limit_n,$10::boolean support,
               $11::boolean field_service,$12::boolean finance,$13::boolean legal,
               $14::text user_id,$15::text role,$16::text assignment_id,$17::text record_id
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
               SELECT g.id,g.customer_id FROM field_service_agreements g, params p 
               WHERE g.company_id=p.company_id AND (p.branch_id IS NULL OR g.branch_id=p.branch_id) 
               AND (p.department_id IS NULL OR g.department_id=p.department_id)
             ), scoped_invoices AS (
               SELECT i.id,i.party_id FROM finance_invoices i, params p 
               WHERE i.company_id=p.company_id
             ), scoped_legal AS (
               SELECT i.id,i.title,i.state,i.version,i.updated_at,n.target_id order_id 
               FROM legal_items i JOIN legal_native_references n ON n.item_id=i.id, params p 
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
               FROM commerce_fulfillments f JOIN scoped_orders o ON o.id=f.order_id 
               JOIN customers c ON c.id=o.party_id
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
               FROM field_service_agreement_events e JOIN field_service_agreements g ON g.id=e.agreement_id 
               JOIN scoped_service sg ON sg.id=g.id JOIN customers c ON c.id=g.customer_id, params p 
               WHERE p.field_service
               UNION ALL 
               SELECT 'service-order:'||e.id,o.id,'field_service','Field service · '||t.title,
               e.action,e.version,(e.created_at)::bigint,'/field-service/'||o.id,so.party_id,c.name 
               FROM field_service_order_events e JOIN field_service_orders o ON o.id=e.order_id 
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
             AND (p.record_id IS NULL OR e.id=p.record_id) 
             AND (p.query IS NULL OR p.query='' OR position(lower(p.query) in lower(concat_ws(' ',e.id,e.source_id,e.source_type,e.title,e.status,e.customer_name)))>0) 
             AND (p.cursor_at IS NULL OR e.at<p.cursor_at OR (e.at=p.cursor_at AND e.id<p.cursor_id)) 
             ORDER BY e.at DESC,e.id DESC LIMIT $9",
        )
        .bind(actor.company_id.as_str())
        .bind(actor.branch_id.as_ref().map(|value| value.as_str()))
        .bind(actor.department_id.as_ref().map(|value| value.as_str()))
        .bind(query.from)
        .bind(query.to)
        .bind(query.query.as_deref())
        .bind(cursor.map(|value| value.at))
        .bind(cursor.map(|value| value.id.as_str()))
        .bind((limit + 1) as i64)
        .bind(view.support)
        .bind(view.field_service)
        .bind(view.finance)
        .bind(view.legal)
        .bind(actor.user_id.as_str())
        .bind(actor.role.as_str())
        .bind(actor.assignment_id.as_str())
        .bind(query.record_id.as_deref())
        .fetch_all(&mut *self.connection)
        .await?;
        rows.iter()
            .map(|row| {
                let item = ExecutiveCustomerActivityRow {
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

impl ExecutiveCustomerActivityRepository for PgCustomerTimelineRead<'_> {
    type Error = Error;

    fn activity<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        view: CustomerTimelineView,
        query: &'a ExecutiveCustomerActivityQuery,
        cursor: Option<&'a CustomerTimelineCursor>,
        limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<ExecutiveCustomerActivityRow>, Self::Error> {
        Box::pin(async move {
            PgCustomerTimelineRead::executive_activity(self, actor, view, query, cursor, limit)
                .await
        })
    }
}

impl CustomerTimelineRepository for PgCustomerTimelineRead<'_> {
    type Error = Error;

    fn customer<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        customer_id: &'a str,
        view: CustomerTimelineView,
    ) -> CustomerTimelineFuture<'a, Option<CustomerAccount>, Self::Error> {
        Box::pin(
            async move { PgCustomerTimelineRead::customer(self, actor, customer_id, view).await },
        )
    }

    fn verify_order<'a>(
        &'a mut self,
        rows: &'a [CustomerTimelineItem],
        cursor: Option<&'a CustomerTimelineCursor>,
    ) -> CustomerTimelineFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            serializable(&mut *self.connection).await?;
            let times: Vec<i64> = rows.iter().map(|item| item.at).collect();
            let ids: Vec<&str> = rows.iter().map(|item| item.id.as_str()).collect();
            let ordinals: Vec<i64> = sqlx::query_scalar(
                "SELECT ordinal FROM unnest($1::bigint[], $2::text[]) WITH ORDINALITY AS e(at,id,ordinal) \
                 WHERE ($3::bigint IS NULL OR e.at < $3 OR (e.at = $3 AND e.id < $4)) \
                 ORDER BY e.at DESC, e.id DESC, e.ordinal",
            )
            .bind(&times)
            .bind(&ids)
            .bind(cursor.map(|value| value.at))
            .bind(cursor.map(|value| value.id.as_str()))
            .fetch_all(&mut *self.connection).await?;
            Ok(ordinals.len() == rows.len()
                && ordinals
                    .iter()
                    .enumerate()
                    .all(|(index, ordinal)| *ordinal == index as i64 + 1))
        })
    }

    fn events<'a>(
        &'a mut self,
        actor: &'a CustomerTimelineActor,
        customer_id: &'a str,
        view: CustomerTimelineView,
        cursor: Option<&'a CustomerTimelineCursor>,
        limit: usize,
    ) -> CustomerTimelineFuture<'a, Vec<CustomerTimelineItem>, Self::Error> {
        Box::pin(async move {
            PgCustomerTimelineRead::events(self, actor, customer_id, view, cursor, limit).await
        })
    }
}
