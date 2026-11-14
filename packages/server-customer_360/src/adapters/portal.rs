//! PostgreSQL adapter for the native customer portal.
//!
//! The SQL reads only the existing customer portal relations
//! (`customer_access_bindings`, `customer_access_identities`,
//! `customer_portal_actions`, `customer_access_reads`) plus the current
//! Commerce order and fulfillment facts. The caller owns the serializable
//! customer-portal owned unit. Notification, audit, outbox and attachment
//! evidence remain explicit composition ports.
use crate::application::portal::{CustomerPortalRepository, PortalFuture};
use crate::domain::portal::{
    CustomerPortalItem, PortalAction, PortalActionFact, PortalActionRow, PortalBinding,
    PortalCommandFacts, PortalCustomerActor, PortalFulfillment, PortalPurchaseEnforcement,
    PortalPurchaseScope, projection_invalid, source_invalid,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

async fn serializable(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("CUSTOMER_PORTAL_TRANSACTION_REQUIRED").into());
    }
    if sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
        .fetch_one(&mut *connection)
        .await?
        != "serializable"
    {
        return Err(PersistenceError::new("CUSTOMER_PORTAL_SERIALIZABLE_REQUIRED").into());
    }
    Ok(())
}

pub struct PgCustomerPortal<'a> {
    connection: &'a mut PgConnection,
}

impl<'a> PgCustomerPortal<'a> {
    pub fn new(connection: &'a mut PgConnection) -> Self {
        Self { connection }
    }

    pub async fn projection(
        &mut self,
        actor: &PortalCustomerActor,
        scope: &PortalPurchaseScope,
        now: i64,
        read_id: &str,
    ) -> Result<Vec<CustomerPortalItem>, Error> {
        serializable(&mut *self.connection).await?;
        let rows = sqlx::query(
            "WITH accessible AS (\
               SELECT b.id binding_id,b.version binding_version,b.order_id,b.number,b.kind,\
               b.state,b.due_date,b.currency,b.updated_at \
               FROM (\
                 SELECT b.id,b.company_id,b.version,b.order_id,o.number,o.kind,o.state,\
                 o.due_date,o.currency,o.updated_at \
                 FROM customer_access_bindings b \
                 JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=$1 AND lower(i.email)=$2 \
                 JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
                 WHERE lower(b.email)=$2 AND b.status='approved' AND b.expires_at>$3 \
                 AND o.party_id=b.customer_id AND o.kind IN ('sales','quote') \
                 AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') \
                 AND ($4::boolean OR b.company_id = ANY($5::text[])) \
                 LIMIT 202 FOR SHARE OF b,o\
               ) b\
             ), audit AS (\
               INSERT INTO customer_access_reads(id,binding_id,binding_version,user_id,read_at,projection_version) \
               SELECT $6||':'||binding_id,binding_id,binding_version,$1,$3,'customer-order-v1' FROM accessible \
               RETURNING binding_id\
             ) \
             SELECT a.binding_id,a.binding_version,a.order_id,a.number,a.kind,a.state,\
             a.due_date,a.currency,a.updated_at,\
             COALESCE((SELECT jsonb_agg(jsonb_build_object('number',f.number,'createdAt',f.created_at,'recorded',true) ORDER BY f.created_at) \
               FROM commerce_fulfillments f WHERE f.order_id=a.order_id),'[]'::jsonb) fulfillments,\
             COALESCE((SELECT jsonb_agg(jsonb_build_object('action',pa.action,'createdAt',pa.created_at) ORDER BY pa.created_at) \
               FROM customer_portal_actions pa WHERE pa.binding_id=a.binding_id AND pa.user_id=$1),'[]'::jsonb) actions \
             FROM accessible a JOIN audit x ON x.binding_id=a.binding_id \
             ORDER BY a.updated_at DESC,a.binding_id",
        )
        .bind(actor.user_id.as_str())
        .bind(actor.email.as_str())
        .bind(now)
        .bind(scope.enforcement == PortalPurchaseEnforcement::Disabled)
        .bind(&scope.company_ids)
        .bind(read_id)
        .fetch_all(&mut *self.connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                let fulfillments: serde_json::Value = row.try_get("fulfillments")?;
                let actions: serde_json::Value = row.try_get("actions")?;
                let item = CustomerPortalItem {
                    binding_id: row.try_get("binding_id")?,
                    binding_version: row.try_get("binding_version")?,
                    order_id: row.try_get("order_id")?,
                    number: row.try_get("number")?,
                    kind: row.try_get("kind")?,
                    state: row.try_get("state")?,
                    due_date: row.try_get("due_date")?,
                    currency: row.try_get("currency")?,
                    updated_at: row.try_get("updated_at")?,
                    fulfillments: serde_json::from_value::<Vec<PortalFulfillment>>(fulfillments)
                        .map_err(|_| PgTransactionError::Business(projection_invalid()))?,
                    actions: serde_json::from_value::<Vec<PortalActionFact>>(actions)
                        .map_err(|_| PgTransactionError::Business(projection_invalid()))?,
                    shared_files: Vec::new(),
                    purchase: None,
                    support_threads: None,
                };
                item.validate().map_err(PgTransactionError::Business)?;
                Ok(item)
            })
            .collect()
    }

    pub async fn replay(&mut self, key: &str) -> Result<Option<PortalActionRow>, Error> {
        serializable(&mut *self.connection).await?;
        self.action(
            "SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id \
             FROM customer_portal_actions WHERE request_key=$1",
            key,
        )
        .await
    }

    pub async fn existing(
        &mut self,
        binding_id: &str,
        action: PortalAction,
    ) -> Result<Option<PortalActionRow>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id \
             FROM customer_portal_actions WHERE binding_id=$1 AND action=$2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(binding_id)
        .bind(action.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(parse_action_row).transpose()
    }

    async fn action(
        &mut self,
        sql: &'static str,
        key: &str,
    ) -> Result<Option<PortalActionRow>, Error> {
        let row = sqlx::query(sql)
            .bind(key)
            .fetch_optional(&mut *self.connection)
            .await?;
        row.map(parse_action_row).transpose()
    }

    pub async fn binding(
        &mut self,
        binding_id: &str,
        email: &str,
    ) -> Result<Option<PortalBinding>, Error> {
        serializable(&mut *self.connection).await?;
        let row = sqlx::query(
            "SELECT b.id binding_id,b.company_id,b.order_id,b.customer_id,b.email,b.status,\
             b.expires_at,b.version binding_version,o.version order_version,o.number,o.state \
             FROM customer_access_bindings b \
             JOIN commerce_orders o ON o.id=b.order_id AND o.company_id=b.company_id \
             WHERE b.id=$1 AND lower(b.email)=$2 AND o.party_id=b.customer_id \
             AND o.kind IN ('sales','quote') \
             FOR UPDATE OF b,o",
        )
        .bind(binding_id)
        .bind(email)
        .fetch_optional(&mut *self.connection)
        .await?;
        row.map(|row| {
            let binding = PortalBinding {
                binding_id: row.try_get("binding_id")?,
                company_id: row.try_get("company_id")?,
                order_id: row.try_get("order_id")?,
                customer_id: row.try_get("customer_id")?,
                email: row.try_get("email")?,
                status: row.try_get("status")?,
                expires_at: row.try_get("expires_at")?,
                binding_version: row.try_get("binding_version")?,
                order_version: row.try_get("order_version")?,
                number: row.try_get("number")?,
                state: row.try_get("state")?,
            };
            binding.validate().map_err(PgTransactionError::Business)?;
            Ok(binding)
        })
        .transpose()
    }

    pub async fn identity(
        &mut self,
        binding_id: &str,
        actor: &PortalCustomerActor,
    ) -> Result<bool, Error> {
        serializable(&mut *self.connection).await?;
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM customer_access_identities \
             WHERE binding_id=$1 AND user_id=$2 AND lower(email)=$3)",
        )
        .bind(binding_id)
        .bind(actor.user_id.as_str())
        .bind(actor.email.as_str())
        .fetch_one(&mut *self.connection)
        .await?)
    }

    pub async fn record(&mut self, command: &PortalCommandFacts) -> Result<(), Error> {
        serializable(&mut *self.connection).await?;
        sqlx::query(
            "INSERT INTO customer_portal_actions(\
             id,company_id,binding_id,order_id,customer_id,user_id,action,note,source_version,\
             request_key,request_hash,status,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'recorded',$12)",
        )
        .bind(command.id.as_str())
        .bind(command.binding.company_id.as_str())
        .bind(command.binding.binding_id.as_str())
        .bind(command.binding.order_id.as_str())
        .bind(command.binding.customer_id.as_str())
        .bind(command.actor.user_id.as_str())
        .bind(command.input.action.as_str())
        .bind(command.input.note.as_str())
        .bind(command.binding.order_version)
        .bind(command.key.as_str())
        .bind(command.request_hash.as_str())
        .bind(command.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(())
    }
}

fn parse_action_row(row: sqlx::postgres::PgRow) -> Result<PortalActionRow, Error> {
    let action: String = row.try_get("action")?;
    PortalAction::parse(&action).ok_or_else(|| PgTransactionError::Business(source_invalid()))?;
    Ok(PortalActionRow {
        id: row.try_get("id")?,
        binding_id: row.try_get("binding_id")?,
        order_id: row.try_get("order_id")?,
        action,
        status: row.try_get("status")?,
        created_at: row.try_get("created_at")?,
        request_hash: row.try_get("request_hash")?,
        user_id: row.try_get("user_id")?,
    })
}

impl CustomerPortalRepository for PgCustomerPortal<'_> {
    type Error = Error;

    fn projection<'a>(
        &'a mut self,
        actor: &'a PortalCustomerActor,
        scope: &'a PortalPurchaseScope,
        now: i64,
        read_id: &'a str,
    ) -> PortalFuture<'a, Vec<CustomerPortalItem>, Self::Error> {
        Box::pin(
            async move { PgCustomerPortal::projection(self, actor, scope, now, read_id).await },
        )
    }

    fn replay<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PortalFuture<'a, Option<PortalActionRow>, Self::Error> {
        Box::pin(async move { PgCustomerPortal::replay(self, key).await })
    }

    fn binding<'a>(
        &'a mut self,
        binding_id: &'a str,
        email: &'a str,
    ) -> PortalFuture<'a, Option<PortalBinding>, Self::Error> {
        Box::pin(async move { PgCustomerPortal::binding(self, binding_id, email).await })
    }

    fn identity<'a>(
        &'a mut self,
        binding_id: &'a str,
        actor: &'a PortalCustomerActor,
    ) -> PortalFuture<'a, bool, Self::Error> {
        Box::pin(async move { PgCustomerPortal::identity(self, binding_id, actor).await })
    }

    fn existing<'a>(
        &'a mut self,
        binding_id: &'a str,
        action: PortalAction,
    ) -> PortalFuture<'a, Option<PortalActionRow>, Self::Error> {
        Box::pin(async move { PgCustomerPortal::existing(self, binding_id, action).await })
    }

    fn record<'a>(
        &'a mut self,
        command: &'a PortalCommandFacts,
    ) -> PortalFuture<'a, (), Self::Error> {
        Box::pin(async move { PgCustomerPortal::record(self, command).await })
    }
}
