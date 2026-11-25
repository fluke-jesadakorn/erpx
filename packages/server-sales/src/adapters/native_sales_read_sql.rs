//! Owner-owned caller-unit read plans for Sales. Generated from the retired
//! TypeScript SELECT literals with their exact positional payloads; the mounted
//! axum `sales/command-plan` route resolves the kind here and returns the
//! statement for the caller to prepare and execute on its own already-open unit.
//!
//! This family covers the small owner-local active price-list reads on the
//! Sales-owned `sales_price_lists` and `sales_price_list_items` relations.
//! Reads carry no receipt/audit/outbox ledger: the caller runs them on the same
//! unit as the surrounding work, so they observe uncommitted effects. No SQL is
//! ever accepted from the wire and no TypeScript SQL remains for these kinds.

/// A planned owner read. `kind` is the allow-listed discriminator, `sql` the
/// exact statement the retired owner literal bound, `bind_count` the positional
/// argument count and `scope` the routing scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeSalesReadStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
    pub scope: &'static str,
}

/// Native successor of the TypeScript `CommerceSalesFacts.readSalesPriceLists1`
/// active price-list read. The caller binds `[company_id]`.
pub const RD_PRICE_LIST_ACTIVE: &str = "sales_rd_price_list_active";
const SQL_PRICE_LIST_ACTIVE: &str =
    "SELECT id,name,customer_id,valid_from,valid_until,max_auto_discount_bps,version FROM sales_price_lists WHERE company_id=? AND status='active' ORDER BY customer_id NULLS LAST,name";
const READ_PRICE_LIST_ACTIVE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICE_LIST_ACTIVE,
    sql: SQL_PRICE_LIST_ACTIVE,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript `CommerceSalesFacts.readSalesPriceListItems1`
/// active price-list item read. The caller binds `[company_id]`.
pub const RD_PRICE_LIST_ITEMS_ACTIVE: &str = "sales_rd_price_list_items_active";
const SQL_PRICE_LIST_ITEMS_ACTIVE: &str =
    "SELECT i.* FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id WHERE l.company_id=? AND l.status='active' ORDER BY i.price_list_id,i.ordinal";
const READ_PRICE_LIST_ITEMS_ACTIVE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICE_LIST_ITEMS_ACTIVE,
    sql: SQL_PRICE_LIST_ITEMS_ACTIVE,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript notification-purchases
/// `customerNotificationReconciliationControls` unknown-attempt read. The caller
/// binds `[notification_id, company_id, attempt]`; the SQL is byte-identical to
/// the retired owner literal.
pub const RD_NOTIFICATION_ATTEMPT: &str = "sales_rd_notification_attempt";
const SQL_NOTIFICATION_ATTEMPT: &str =
    "SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'";
const READ_NOTIFICATION_ATTEMPT: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_NOTIFICATION_ATTEMPT,
    sql: SQL_NOTIFICATION_ATTEMPT,
    bind_count: 3,
    scope: "holding",
};

/// Native successor of the TypeScript notification-purchases
/// `customerNotificationReconciliationControls` reconciliation-installed check.
/// The caller binds no values; the SQL is byte-identical to the retired owner
/// literal.
pub const RD_NOTIFICATION_RECONCILIATION_INSTALLED: &str =
    "sales_rd_notification_reconciliation_installed";
const SQL_NOTIFICATION_RECONCILIATION_INSTALLED: &str =
    "SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed";
const READ_NOTIFICATION_RECONCILIATION_INSTALLED: NativeSalesReadStatement =
    NativeSalesReadStatement {
        kind: RD_NOTIFICATION_RECONCILIATION_INSTALLED,
        sql: SQL_NOTIFICATION_RECONCILIATION_INSTALLED,
        bind_count: 0,
        scope: "holding",
    };

/// Native successor of the TypeScript followups `recipients` read. The
/// caller binds [company, company, order, company, branch, branch, department,
/// department]; every projection and the recipient eligibility predicate is
/// fixed owner SQL, byte-identical to the retired TypeScript statement.
pub const RD_FOLLOWUP_RECIPIENTS: &str = "sales_rd_followup_recipients";
const SQL_FOLLOWUP_RECIPIENTS: &str =
    "SELECT DISTINCT c.owner_id id,p.legal_name name FROM (SELECT DISTINCT user_id owner_id FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) assignments WHERE company_id=?) c JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=c.owner_id AND p.company_id=? JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=? WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\",\"level_id\",\"position_id\",\"self_service_top_level\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=o.company_id AND a.user_id=c.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'commerce.sales') AND iam_role_allows(a.role,a.company_id,'commerce.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ORDER BY p.legal_name";
const READ_FOLLOWUP_RECIPIENTS: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_RECIPIENTS,
    sql: SQL_FOLLOWUP_RECIPIENTS,
    bind_count: 8,
    scope: "holding",
};

/// Native successor of the TypeScript pricing price-list point read: [id, company]. The
/// statement is byte-identical to the retired owner literal.
pub const RD_PRICING_LIST_BY_ID: &str = "sales_rd_pricing_list_by_id";
const SQL_PRICING_LIST_BY_ID: &str = "SELECT * FROM sales_price_lists WHERE id=? AND company_id=?";
const READ_PRICING_LIST_BY_ID: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICING_LIST_BY_ID,
    sql: SQL_PRICING_LIST_BY_ID,
    bind_count: 2,
    scope: "holding",
};

/// Native successor of the TypeScript pricing active price-list read by id: [id, company, company, date, date, party]. The
/// statement is byte-identical to the retired owner literal.
pub const RD_PRICING_LIST_ACTIVE_BY_ID: &str = "sales_rd_pricing_list_active_by_id";
const SQL_PRICING_LIST_ACTIVE_BY_ID: &str = "SELECT * FROM sales_price_lists WHERE id=? AND company_id=? AND status='active' AND currency=(SELECT currency FROM (SELECT \"company_id\",\"chart_id\",\"currency\",\"receivable_account_id\",\"payable_account_id\",\"revenue_account_id\",\"inventory_account_id\",\"clearing_account_id\",\"cogs_account_id\",\"input_tax_account_id\",\"output_tax_account_id\",\"cash_account_id\" FROM commerce_settings) commerce_settings WHERE company_id=?) AND valid_from<=? AND valid_until>=? AND (customer_id IS NULL OR customer_id=?)";
const READ_PRICING_LIST_ACTIVE_BY_ID: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICING_LIST_ACTIVE_BY_ID,
    sql: SQL_PRICING_LIST_ACTIVE_BY_ID,
    bind_count: 6,
    scope: "holding",
};

/// Native successor of the TypeScript pricing active price-list read by Company: [company, company, date, date, party, party]. The
/// statement is byte-identical to the retired owner literal.
pub const RD_PRICING_LIST_ACTIVE: &str = "sales_rd_pricing_list_active";
const SQL_PRICING_LIST_ACTIVE: &str = "SELECT * FROM sales_price_lists WHERE company_id=? AND status='active' AND currency=(SELECT currency FROM (SELECT \"company_id\",\"chart_id\",\"currency\",\"receivable_account_id\",\"payable_account_id\",\"revenue_account_id\",\"inventory_account_id\",\"clearing_account_id\",\"cogs_account_id\",\"input_tax_account_id\",\"output_tax_account_id\",\"cash_account_id\" FROM commerce_settings) commerce_settings WHERE company_id=?) AND valid_from<=? AND valid_until>=? AND (customer_id=? OR customer_id IS NULL) ORDER BY CASE WHEN customer_id=? THEN 0 ELSE 1 END,valid_from DESC,activated_at DESC LIMIT 1";
const READ_PRICING_LIST_ACTIVE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICING_LIST_ACTIVE,
    sql: SQL_PRICING_LIST_ACTIVE,
    bind_count: 6,
    scope: "holding",
};

/// Native successor of the TypeScript pricing price-list items read: [price_list_id]. The
/// statement is byte-identical to the retired owner literal.
pub const RD_PRICING_ITEMS_BY_LIST: &str = "sales_rd_pricing_items_by_list";
const SQL_PRICING_ITEMS_BY_LIST: &str = "SELECT * FROM sales_price_list_items WHERE price_list_id=?";
const READ_PRICING_ITEMS_BY_LIST: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PRICING_ITEMS_BY_LIST,
    sql: SQL_PRICING_ITEMS_BY_LIST,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase source read (followup kind: [id, company, branch, branch, department, department]).
/// Fixed owner projections only; the statement is byte-identical to the retired literal.
pub const RD_FOLLOWUP_SOURCE_FOLLOWUP: &str = "sales_rd_followup_source_followup";
const SQL_FOLLOWUP_SOURCE_FOLLOWUP: &str = "SELECT id,company_id,branch_id,department_id,order_kind,witness::text witness,owner_id,due_date,status,version FROM (SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind,jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id) followup_source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)";
const READ_FOLLOWUP_SOURCE_FOLLOWUP: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_SOURCE_FOLLOWUP,
    sql: SQL_FOLLOWUP_SOURCE_FOLLOWUP,
    bind_count: 6,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase source read (order kind: [id, company, branch, branch, department, department]).
/// Fixed owner projections only; the statement is byte-identical to the retired literal.
pub const RD_FOLLOWUP_SOURCE_ORDER: &str = "sales_rd_followup_source_order";
const SQL_FOLLOWUP_SOURCE_ORDER: &str = "SELECT id,company_id,branch_id,department_id,order_kind,witness::text witness,NULL::text owner_id,NULL::text due_date,NULL::text status,NULL::bigint version FROM (SELECT actual.*,actual.kind order_kind FROM (SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')) actual) followup_source WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)";
const READ_FOLLOWUP_SOURCE_ORDER: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_SOURCE_ORDER,
    sql: SQL_FOLLOWUP_SOURCE_ORDER,
    bind_count: 6,
    scope: "holding",
};

/// Native successor of the TypeScript metric non-enforced followup metric projection: no binds.
/// Fixed owner projections only; byte-identical to the retired literal.
pub const RD_FOLLOWUP_METRIC_PROJECTION: &str = "sales_rd_followup_metric_projection";
const SQL_FOLLOWUP_METRIC_SELECT: &str = "SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind FROM customer_followups followup JOIN (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id";
const READ_FOLLOWUP_METRIC_SELECT: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_METRIC_PROJECTION,
    sql: SQL_FOLLOWUP_METRIC_SELECT,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript metric enforced followup metric source: [assignment, user, role, assignment].
/// Fixed owner projections only; byte-identical to the retired literal.
pub const RD_FOLLOWUP_METRIC_SOURCE: &str = "sales_rd_followup_metric_source";
const SQL_FOLLOWUP_METRIC_SOURCE: &str = "WITH sales_metric_clock AS MATERIALIZED (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT sales_metric_owned.* FROM (SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind FROM customer_followups followup JOIN (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id WHERE actual.kind IN ('sales','quote')) sales_metric_owned CROSS JOIN sales_metric_clock WHERE EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=sales_metric_owned.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=sales_metric_owned.branch_id OR g.scope_kind='department' AND g.scope_node_id=sales_metric_owned.department_id)) AND ((sales_metric_owned.order_kind='quote' AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=sales_metric_owned.company_id AND starts_at<=sales_metric_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=sales_metric_owned.company_id AND retained_at<=sales_metric_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1)))) OR (sales_metric_owned.order_kind='sales' AND ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=sales_metric_owned.company_id AND starts_at<=sales_metric_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=sales_metric_owned.company_id AND retained_at<=sales_metric_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))))) AND EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('crm'::text)))";
const READ_FOLLOWUP_METRIC_SOURCE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_METRIC_SOURCE,
    sql: SQL_FOLLOWUP_METRIC_SOURCE,
    bind_count: 4,
    scope: "holding",
};

/// Native successor of the TypeScript salesNotificationSource read (non-production notice source: [id]).
/// Fixed owner projections; byte-identical to the retired rendered literal.
pub const RD_NOTIFICATION_SOURCE_DEV: &str = "sales_rd_notification_source_dev";
const SQL_NOTIFICATION_SOURCE_DEV: &str = "SELECT n.*,delivery.department_id,delivery.order_state,delivery.company_id source_company,customer.company_id customer_company,customer.status customer_status,customer.notification_email,customer.notification_consent,settings.adapter current_adapter,TRUE release_ready,\n jsonb_build_array(n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.created_by,n.created_at,delivery.fulfillment_id,delivery.order_id,delivery.company_id,delivery.branch_id,delivery.department_id,delivery.party_id,delivery.order_kind,customer.id,customer.company_id,customer.kind)::text image\n FROM commerce_customer_notifications n LEFT JOIN (SELECT f.id fulfillment_id,f.version fulfillment_version,o.id order_id,o.company_id,o.branch_id,o.department_id,o.kind order_kind,o.state order_state,o.currency,o.party_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales') delivery ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id\n LEFT JOIN (SELECT id,company_id,kind,version,status,notification_email,notification_consent FROM finance_parties WHERE kind='customer') customer ON customer.id=n.party_id AND customer.company_id=n.company_id\n LEFT JOIN commerce_customer_notification_settings settings ON settings.company_id=n.company_id WHERE n.id=?";
const READ_NOTIFICATION_SOURCE_DEV: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_NOTIFICATION_SOURCE_DEV,
    sql: SQL_NOTIFICATION_SOURCE_DEV,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript salesNotificationSource read (production notice source: [id]).
/// Fixed owner projections; byte-identical to the retired rendered literal.
pub const RD_NOTIFICATION_SOURCE_PROD: &str = "sales_rd_notification_source_prod";
const SQL_NOTIFICATION_SOURCE_PROD: &str = "SELECT n.*,delivery.department_id,delivery.order_state,delivery.company_id source_company,customer.company_id customer_company,customer.status customer_status,customer.notification_email,customer.notification_consent,settings.adapter current_adapter,FALSE release_ready,\n jsonb_build_array(n.id,n.company_id,n.branch_id,n.order_id,n.fulfillment_id,n.party_id,n.party_version,n.recipient_email,n.subject,n.body_text,n.adapter,n.created_by,n.created_at,delivery.fulfillment_id,delivery.order_id,delivery.company_id,delivery.branch_id,delivery.department_id,delivery.party_id,delivery.order_kind,customer.id,customer.company_id,customer.kind)::text image\n FROM commerce_customer_notifications n LEFT JOIN (SELECT f.id fulfillment_id,f.version fulfillment_version,o.id order_id,o.company_id,o.branch_id,o.department_id,o.kind order_kind,o.state order_state,o.currency,o.party_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales') delivery ON delivery.fulfillment_id=n.fulfillment_id AND delivery.order_id=n.order_id AND delivery.company_id=n.company_id AND delivery.party_id=n.party_id\n LEFT JOIN (SELECT id,company_id,kind,version,status,notification_email,notification_consent FROM finance_parties WHERE kind='customer') customer ON customer.id=n.party_id AND customer.company_id=n.company_id\n LEFT JOIN commerce_customer_notification_settings settings ON settings.company_id=n.company_id WHERE n.id=?";
const READ_NOTIFICATION_SOURCE_PROD: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_NOTIFICATION_SOURCE_PROD,
    sql: SQL_NOTIFICATION_SOURCE_PROD,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript calendar follow-up JSON id-list membership
/// fragment. The single ? placeholder is bound by the caller's larger statement;
/// the planner carries no values of its own.
pub const RD_CALENDAR_JSONB_MEMBER: &str = "sales_rd_calendar_jsonb_member";
const SQL_CALENDAR_JSONB_MEMBER: &str = "IN (SELECT jsonb_array_elements_text(?::jsonb))";
const READ_CALENDAR_JSONB_MEMBER: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_CALENDAR_JSONB_MEMBER,
    sql: SQL_CALENDAR_JSONB_MEMBER,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript calendar follow-up HR people owner membership
/// fragment. The single ? placeholder is bound by the caller's larger statement;
/// the planner carries no values of its own.
pub const RD_CALENDAR_OWNER_MEMBER: &str = "sales_rd_calendar_owner_member";
const SQL_CALENDAR_OWNER_MEMBER: &str = "IN(SELECT user_id FROM (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) hr_people WHERE id=? AND company_id=c.company_id)";
const READ_CALENDAR_OWNER_MEMBER: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_CALENDAR_OWNER_MEMBER,
    sql: SQL_CALENDAR_OWNER_MEMBER,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase orders source.
/// Fixed CRM owner projections; no binds, byte-identical to the retired literal.
pub const RD_FOLLOWUP_ORDERS: &str = "sales_rd_followup_orders";
const SQL_FOLLOWUP_ORDERS: &str = "SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')";
const READ_FOLLOWUP_ORDERS: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_FOLLOWUP_ORDERS,
    sql: SQL_FOLLOWUP_ORDERS,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase projection c_TRUE.
/// Seven ? placeholders are bound by the caller; the planner carries no values.
pub const RD_PURCHASE_PROJECTION_C_READ: &str = "sales_rd_purchase_projection_c_read";
const SQL_PURCHASE_PROJECTION_C_READ: &str = "EXISTS(WITH followup_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM (SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind,jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id) followup_native CROSS JOIN followup_clock WHERE followup_native.id=c.id AND followup_native.company_id=c.company_id AND TRUE AND (?::text IS NULL OR followup_native.branch_id=?) AND (?::text IS NULL OR followup_native.department_id=?) AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=followup_native.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=followup_native.branch_id OR g.scope_kind='department' AND g.scope_node_id=followup_native.department_id)) AND CASE WHEN followup_native.order_kind='quote' THEN ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) ELSE ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) END)";
const READ_PURCHASE_PROJECTION_C_READ: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PURCHASE_PROJECTION_C_READ,
    sql: SQL_PURCHASE_PROJECTION_C_READ,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase projection c_FALSE.
/// Seven ? placeholders are bound by the caller; the planner carries no values.
pub const RD_PURCHASE_PROJECTION_C_DENY: &str = "sales_rd_purchase_projection_c_deny";
const SQL_PURCHASE_PROJECTION_C_DENY: &str = "EXISTS(WITH followup_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM (SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind,jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id) followup_native CROSS JOIN followup_clock WHERE followup_native.id=c.id AND followup_native.company_id=c.company_id AND FALSE AND (?::text IS NULL OR followup_native.branch_id=?) AND (?::text IS NULL OR followup_native.department_id=?) AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=followup_native.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=followup_native.branch_id OR g.scope_kind='department' AND g.scope_node_id=followup_native.department_id)) AND CASE WHEN followup_native.order_kind='quote' THEN ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) ELSE ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) END)";
const READ_PURCHASE_PROJECTION_C_DENY: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PURCHASE_PROJECTION_C_DENY,
    sql: SQL_PURCHASE_PROJECTION_C_DENY,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase projection followup_projection_TRUE.
/// Seven ? placeholders are bound by the caller; the planner carries no values.
pub const RD_PURCHASE_PROJECTION_FP_READ: &str = "sales_rd_purchase_projection_fp_read";
const SQL_PURCHASE_PROJECTION_FP_READ: &str = "EXISTS(WITH followup_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM (SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind,jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id) followup_native CROSS JOIN followup_clock WHERE followup_native.id=followup_projection.id AND followup_native.company_id=followup_projection.company_id AND TRUE AND (?::text IS NULL OR followup_native.branch_id=?) AND (?::text IS NULL OR followup_native.department_id=?) AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=followup_native.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=followup_native.branch_id OR g.scope_kind='department' AND g.scope_node_id=followup_native.department_id)) AND CASE WHEN followup_native.order_kind='quote' THEN ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) ELSE ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) END)";
const READ_PURCHASE_PROJECTION_FP_READ: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PURCHASE_PROJECTION_FP_READ,
    sql: SQL_PURCHASE_PROJECTION_FP_READ,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript followup purchase projection followup_projection_FALSE.
/// Seven ? placeholders are bound by the caller; the planner carries no values.
pub const RD_PURCHASE_PROJECTION_FP_DENY: &str = "sales_rd_purchase_projection_fp_deny";
const SQL_PURCHASE_PROJECTION_FP_DENY: &str = "EXISTS(WITH followup_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM (SELECT followup.*,actual.branch_id,actual.department_id,actual.kind order_kind,jsonb_build_object('followup',jsonb_build_array(followup.id,followup.company_id,followup.order_id,followup.created_by),'order',actual.witness) witness FROM customer_followups followup JOIN (SELECT actual.*,jsonb_build_array(actual.id,actual.company_id,actual.branch_id,actual.department_id,actual.kind,actual.party_id,actual.currency) witness FROM (SELECT id,company_id,branch_id,department_id,kind,party_id,currency,source_id FROM commerce_orders) actual JOIN (SELECT id,company_id,kind,status FROM finance_parties) customer ON customer.id=actual.party_id AND customer.company_id=actual.company_id AND customer.kind='customer' WHERE actual.kind IN ('quote','sales')) actual ON actual.id=followup.order_id AND actual.company_id=followup.company_id) followup_native CROSS JOIN followup_clock WHERE followup_native.id=followup_projection.id AND followup_native.company_id=followup_projection.company_id AND FALSE AND (?::text IS NULL OR followup_native.branch_id=?) AND (?::text IS NULL OR followup_native.department_id=?) AND EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=followup_native.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=followup_native.branch_id OR g.scope_kind='department' AND g.scope_node_id=followup_native.department_id)) AND CASE WHEN followup_native.order_kind='quote' THEN ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) ELSE ((EXISTS(SELECT 1 FROM (SELECT package_id,package_version FROM company_package_grants WHERE company_id=followup_native.company_id AND starts_at<=followup_clock.now_ms AND state<>'inactive'\n      UNION SELECT package_id,package_version FROM company_package_history_rights WHERE company_id=followup_native.company_id AND retained_at<=followup_clock.now_ms\n        AND retained_at>=0 AND retained_at<=9007199254740991 AND source_grant_version>0 AND source_event_id<>'') purchase WHERE purchase.package_id='sales' AND purchase.package_version=1))) END)";
const READ_PURCHASE_PROJECTION_FP_DENY: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PURCHASE_PROJECTION_FP_DENY,
    sql: SQL_PURCHASE_PROJECTION_FP_DENY,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript CRM membership predicate (plain).
/// The single ? bind is supplied by the caller; the planner carries no values.
pub const RD_CRM_ASSIGNMENT_PLAIN: &str = "sales_rd_crm_assignment_plain";
const SQL_CRM_ASSIGNMENT_PLAIN: &str = "EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('crm'::text)))";
const READ_CRM_ASSIGNMENT_PLAIN: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_CRM_ASSIGNMENT_PLAIN,
    sql: SQL_CRM_ASSIGNMENT_PLAIN,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript CRM membership predicate (selected).
/// The single ? bind is supplied by the caller; the planner carries no values.
pub const RD_CRM_ASSIGNMENT_ALIASED: &str = "sales_rd_crm_assignment_aliased";
const SQL_CRM_ASSIGNMENT_ALIASED: &str = "EXISTS(SELECT 1 FROM assignments selected WHERE selected.id=? AND (jsonb_array_length(selected.modules_json::jsonb)=0 OR selected.modules_json::jsonb @> jsonb_build_array('crm'::text)))";
const READ_CRM_ASSIGNMENT_ALIASED: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_CRM_ASSIGNMENT_ALIASED,
    sql: SQL_CRM_ASSIGNMENT_ALIASED,
    bind_count: 0,
    scope: "holding",
};

/// Native successor of the TypeScript sales package-settlement facts read.
/// Fixed owner sources; the single ? is bound by the caller's company id.
pub const RD_PACKAGE_SETTLEMENT_SALES: &str = "sales_rd_package_settlement_sales";
const SQL_PACKAGE_SETTLEMENT_SALES: &str = "WITH scope AS(SELECT ?::text company_id),orders AS MATERIALIZED(SELECT native.* FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native,scope WHERE native.company_id=scope.company_id),deliveries AS MATERIALIZED(SELECT native.* FROM (SELECT delivery.*,parent.company_id,parent.branch_id,parent.department_id,parent.party_id,parent.kind order_kind,parent.currency order_currency,parent.warehouse_id FROM commerce_fulfillments delivery JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') native,scope WHERE native.company_id=scope.company_id)\n SELECT 'order' kind,o.id,to_jsonb(o) fact,\n NOT COALESCE((o.state='cancelled' AND NOT EXISTS(SELECT 1 FROM deliveries f WHERE f.order_id=o.id) AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=o.id AND audit.record_version=o.version AND audit.action=receipt.operation WHERE receipt.operation='commerce.order.cancel' AND receipt.record_id=o.id AND receipt.authority_json::jsonb->>'companyId'=o.company_id AND receipt.result_json::jsonb->>'state'='cancelled' AND receipt.result_json::jsonb->>'version'=o.version::text))\n OR(o.kind='quote' AND o.state='converted' AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN orders child ON child.id=receipt.result_json::jsonb->>'salesId' AND child.kind='sales' AND child.source_id=o.id AND child.party_id=o.party_id AND child.branch_id=o.branch_id AND child.department_id IS NOT DISTINCT FROM o.department_id WHERE receipt.record_id=o.id AND receipt.operation='commerce.order.convert' AND receipt.authority_json::jsonb->>'companyId'=o.company_id AND receipt.result_json::jsonb->>'state'='converted' AND receipt.result_json::jsonb->>'version'=o.version::text))\n OR(o.kind='sales' AND o.state='fulfilled' AND EXISTS(SELECT 1 FROM deliveries f WHERE f.order_id=o.id) AND NOT EXISTS(SELECT 1 FROM (SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')) line WHERE line.order_id=o.id AND (line.quantity_milli<>line.fulfilled_milli OR line.quantity_milli<>COALESCE((SELECT sum(delivered.quantity_milli) FROM (SELECT line.*,delivery.order_id,original.product_id FROM commerce_fulfillment_lines line JOIN commerce_fulfillments delivery ON delivery.id=line.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id LEFT JOIN commerce_order_lines original ON original.id=line.order_line_id WHERE parent.kind='sales') delivered WHERE delivered.order_line_id=line.id AND delivered.order_id=o.id),0))) AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=o.id AND audit.record_version::text=receipt.result_json::jsonb->>'version' AND audit.action=receipt.operation WHERE receipt.record_id=o.id AND receipt.operation='commerce.order.fulfill' AND receipt.authority_json::jsonb->>'companyId'=o.company_id AND (receipt.result_json::jsonb->>'version')::bigint<=o.version AND EXISTS(SELECT 1 FROM deliveries delivered WHERE delivered.id=receipt.result_json::jsonb->>'fulfillmentId' AND delivered.order_id=o.id))),false) pending FROM orders o\n UNION ALL SELECT 'line',line.id,to_jsonb(line),product.id IS NULL OR product.company_id<>o.company_id FROM (SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')) line JOIN orders o ON o.id=line.order_id LEFT JOIN (SELECT id,company_id,item_type,tracking_mode,status FROM commerce_products) product ON product.id=line.product_id\n UNION ALL SELECT 'delivery-line',line.id,to_jsonb(line),original.id IS NULL OR original.order_id<>delivery.order_id OR original.product_id IS DISTINCT FROM line.product_id FROM (SELECT line.*,delivery.order_id,original.product_id FROM commerce_fulfillment_lines line JOIN commerce_fulfillments delivery ON delivery.id=line.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id LEFT JOIN commerce_order_lines original ON original.id=line.order_line_id WHERE parent.kind='sales') line JOIN deliveries delivery ON delivery.id=line.fulfillment_id LEFT JOIN (SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')) original ON original.id=line.order_line_id\n UNION ALL SELECT 'delivery-evidence',concat(evidence.fulfillment_id,':',evidence.attachment_id),to_jsonb(evidence),false FROM commerce_fulfillment_evidence evidence JOIN deliveries delivery ON delivery.id=evidence.fulfillment_id\n UNION ALL SELECT 'return-line',line.id,to_jsonb(line),false FROM (SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') line JOIN (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') source ON source.id=line.return_id CROSS JOIN scope WHERE source.company_id=scope.company_id OR source.source_company=scope.company_id\n UNION ALL SELECT 'return-lot',lot.id,to_jsonb(lot),false FROM (SELECT allocation.* FROM commerce_return_lot_allocations allocation JOIN commerce_return_lines line ON line.id=allocation.return_line_id JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') lot JOIN (SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') line ON line.id=lot.return_line_id JOIN (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') source ON source.id=line.return_id CROSS JOIN scope WHERE source.company_id=scope.company_id OR source.source_company=scope.company_id\n UNION ALL SELECT 'goods-valuation',valuation.fulfillment_id,to_jsonb(valuation),valuation.company_id<>delivery.company_id FROM (SELECT valuation.*,parent.company_id source_company FROM commerce_goods_valuations valuation JOIN commerce_fulfillments delivery ON delivery.id=valuation.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') valuation JOIN deliveries delivery ON delivery.id=valuation.fulfillment_id\n UNION ALL SELECT 'invoice-carrying',carrying.invoice_id,to_jsonb(carrying),carrying.company_id<>carrying.source_company FROM (SELECT carrying.*,parent.company_id source_company FROM commerce_invoice_goods_carrying carrying JOIN commerce_fulfillments delivery ON delivery.invoice_id=carrying.invoice_id JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') carrying,scope WHERE carrying.company_id=scope.company_id OR carrying.source_company=scope.company_id\n UNION ALL SELECT 'price-item',item.id,to_jsonb(item),false FROM sales_price_list_items item JOIN sales_price_lists list ON list.id=item.price_list_id CROSS JOIN scope WHERE list.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'contact-note',note.id,to_jsonb(note),NOT EXISTS(SELECT 1 FROM orders original WHERE original.id=note.order_id AND original.company_id=note.company_id AND original.party_id=note.customer_id) FROM customer_contact_notes note,scope WHERE note.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'contact-note-history',event.id,to_jsonb(event),false FROM customer_contact_note_history event JOIN customer_contact_notes note ON note.id=event.note_id CROSS JOIN scope WHERE note.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'followup-history',event.id,to_jsonb(event),false FROM customer_followup_events event JOIN customer_followups followup ON followup.id=event.followup_id CROSS JOIN scope WHERE followup.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'warehouse',o.id,to_jsonb(warehouse),warehouse.id IS NULL OR warehouse.company_id<>o.company_id OR warehouse.branch_id<>o.branch_id FROM orders o LEFT JOIN (SELECT id,company_id,branch_id,status FROM commerce_warehouses) warehouse ON warehouse.id=o.warehouse_id\n UNION ALL SELECT 'delivery',f.id,to_jsonb(f),f.invoice_id IS NULL OR NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.order.fulfill' AND receipt.record_id=f.order_id AND receipt.authority_json::jsonb->>'companyId'=f.company_id AND receipt.result_json::jsonb->>'fulfillmentId'=f.id) FROM deliveries f\n UNION ALL SELECT 'return',r.id,to_jsonb(r),NOT COALESCE(r.company_id=scope.company_id AND r.source_company=scope.company_id AND r.order_kind='sales' AND r.kind='customer_return' AND EXISTS(SELECT 1 FROM deliveries f WHERE f.id=r.fulfillment_id AND f.order_id=r.order_id AND f.invoice_id=r.invoice_id) AND r.status IN ('posted','rejected') AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=r.id AND audit.record_version=r.version AND audit.action=receipt.operation WHERE receipt.record_id=r.id AND receipt.operation=CASE r.status WHEN 'posted' THEN 'commerce.return.complete' ELSE 'commerce.return.reject' END AND receipt.authority_json::jsonb->>'companyId'=r.company_id AND receipt.result_json::jsonb->>'status'=r.status AND receipt.result_json::jsonb->>'version'=r.version::text),false) FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') r,scope WHERE r.company_id=scope.company_id OR r.source_company=scope.company_id\n UNION ALL SELECT 'pricing',list.id,to_jsonb(list),list.status='draft' OR NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=list.id AND audit.record_version=list.version WHERE receipt.record_id=list.id AND receipt.operation=CASE list.status WHEN 'active' THEN 'sales.price-list.activate' WHEN 'cancelled' THEN 'sales.price-list.cancel' END AND receipt.authority_json::jsonb->>'companyId'=list.company_id AND receipt.result_json::jsonb->>'version'=list.version::text AND receipt.result_json::jsonb->>'status'=list.status AND (list.status<>'active' OR receipt.actor_id=list.approved_by AND list.approved_by<>list.prepared_by)) FROM sales_price_lists list,scope WHERE list.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'followup',followup.id,to_jsonb(followup),followup.status='open' OR NOT EXISTS(SELECT 1 FROM orders o JOIN customer_followup_events event ON event.followup_id=followup.id AND event.version=followup.version AND event.action=CASE followup.status WHEN 'completed' THEN 'complete' WHEN 'cancelled' THEN 'cancel' END JOIN command_receipts receipt ON event.id='followup-event-'||receipt.command_id AND receipt.operation='customer.followup.'||event.action AND receipt.record_id=followup.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=followup.company_id WHERE o.id=followup.order_id AND receipt.result_json::jsonb->>'status'=followup.status AND receipt.result_json::jsonb->>'version'=followup.version::text) FROM customer_followups followup,scope WHERE followup.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'customer-notice',notice.id,to_jsonb(notice),notice.status NOT IN ('sent','cancelled') OR NOT EXISTS(SELECT 1 FROM deliveries f WHERE f.id=notice.fulfillment_id AND f.order_id=notice.order_id AND f.company_id=notice.company_id AND f.party_id=notice.party_id) OR NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.customer-notification.queue' AND receipt.record_id=notice.fulfillment_id AND receipt.actor_id=notice.created_by AND receipt.authority_json::jsonb->>'companyId'=notice.company_id AND receipt.result_json::jsonb->>'id'=notice.id) OR (notice.status='sent' AND COALESCE(notice.provider_message_id,'')='') OR EXISTS(SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.notification_id=notice.id AND (attempt.company_id<>notice.company_id OR (attempt.state='sending' OR attempt.state='unknown' AND NOT EXISTS(SELECT 1 FROM sales_notification_reconciliations resolution JOIN command_receipts receipt ON receipt.command_id=resolution.command_id AND receipt.record_id=notice.id AND receipt.operation='commerce.customer-notification.reconcile' AND receipt.actor_id=resolution.actor_id AND receipt.authority_json::jsonb->>'companyId'=notice.company_id JOIN audit_events audit ON audit.command_id=resolution.command_id AND audit.record_id=notice.id AND audit.record_version=resolution.notification_version AND audit.action=receipt.operation JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id WHERE resolution.attempt_id=attempt.id AND resolution.notification_id=notice.id AND resolution.company_id=notice.company_id AND resolution.source_hash=attempt.source_hash AND resolution.provider_message_id=attempt.provider_message_id AND resolution.provider_message_id=notice.provider_message_id AND resolution.actor_id<>authority.actor_id AND resolution.actor_id<>notice.created_by AND resolution.notification_version=notice.version AND resolution.outcome=notice.status AND receipt.result_json::jsonb->>'reconciliationId'=resolution.id AND receipt.result_json::jsonb->>'status'=resolution.outcome AND receipt.result_json::jsonb->>'version'=resolution.notification_version::text AND resolution.provider_observation_json::jsonb->>'id'=resolution.provider_message_id AND resolution.provider_observation_json::jsonb->'to'=jsonb_build_array(notice.recipient_email) AND resolution.provider_observation_json::jsonb->>'subject'=notice.subject AND resolution.provider_observation_json::jsonb->>'text'=notice.body_text AND resolution.provider_observation_json::jsonb->>'last_event'=resolution.provider_event AND resolution.outcome=CASE WHEN resolution.provider_event IN ('sent','delivered','opened','clicked') THEN 'sent' ELSE 'cancelled' END)))) FROM commerce_customer_notifications notice,scope WHERE notice.company_id=scope.company_id AND TRUE\n UNION ALL SELECT 'notice-authorization',authority.command_id,to_jsonb(authority),authority.company_id<>notice.company_id FROM sales_notification_execution_authorizations authority JOIN commerce_customer_notifications notice ON notice.id=authority.notification_id CROSS JOIN scope WHERE (authority.company_id=scope.company_id OR notice.company_id=scope.company_id) AND TRUE\n UNION ALL SELECT 'notice-attempt',attempt.id,to_jsonb(attempt),attempt.company_id<>notice.company_id FROM sales_notification_execution_attempts attempt JOIN commerce_customer_notifications notice ON notice.id=attempt.notification_id CROSS JOIN scope WHERE (attempt.company_id=scope.company_id OR notice.company_id=scope.company_id) AND TRUE\n UNION ALL SELECT 'notice-reconciliation',resolution.id,to_jsonb(resolution),resolution.company_id<>notice.company_id OR NOT EXISTS(SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.id=resolution.attempt_id AND attempt.notification_id=notice.id AND attempt.company_id=notice.company_id AND attempt.state='unknown' AND EXISTS(SELECT 1 FROM sales_notification_reconciliations resolution JOIN command_receipts receipt ON receipt.command_id=resolution.command_id AND receipt.record_id=notice.id AND receipt.operation='commerce.customer-notification.reconcile' AND receipt.actor_id=resolution.actor_id AND receipt.authority_json::jsonb->>'companyId'=notice.company_id JOIN audit_events audit ON audit.command_id=resolution.command_id AND audit.record_id=notice.id AND audit.record_version=resolution.notification_version AND audit.action=receipt.operation JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id WHERE resolution.attempt_id=attempt.id AND resolution.notification_id=notice.id AND resolution.company_id=notice.company_id AND resolution.source_hash=attempt.source_hash AND resolution.provider_message_id=attempt.provider_message_id AND resolution.provider_message_id=notice.provider_message_id AND resolution.actor_id<>authority.actor_id AND resolution.actor_id<>notice.created_by AND resolution.notification_version=notice.version AND resolution.outcome=notice.status AND receipt.result_json::jsonb->>'reconciliationId'=resolution.id AND receipt.result_json::jsonb->>'status'=resolution.outcome AND receipt.result_json::jsonb->>'version'=resolution.notification_version::text AND resolution.provider_observation_json::jsonb->>'id'=resolution.provider_message_id AND resolution.provider_observation_json::jsonb->'to'=jsonb_build_array(notice.recipient_email) AND resolution.provider_observation_json::jsonb->>'subject'=notice.subject AND resolution.provider_observation_json::jsonb->>'text'=notice.body_text AND resolution.provider_observation_json::jsonb->>'last_event'=resolution.provider_event AND resolution.outcome=CASE WHEN resolution.provider_event IN ('sent','delivered','opened','clicked') THEN 'sent' ELSE 'cancelled' END)) FROM sales_notification_reconciliations resolution JOIN commerce_customer_notifications notice ON notice.id=resolution.notification_id CROSS JOIN scope WHERE (resolution.company_id=scope.company_id OR notice.company_id=scope.company_id) AND TRUE\n UNION ALL SELECT 'notice-provider',settings.company_id,to_jsonb(settings),settings.adapter<>'disabled' FROM commerce_customer_notification_settings settings,scope WHERE settings.company_id=scope.company_id AND TRUE ORDER BY kind,id";
const READ_PACKAGE_SETTLEMENT_SALES: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PACKAGE_SETTLEMENT_SALES,
    sql: SQL_PACKAGE_SETTLEMENT_SALES,
    bind_count: 1,
    scope: "holding",
};

/// Native successor of the TypeScript ecommerce package-settlement facts read.
/// Fixed owner sources; the single ? is bound by the caller's company id.
pub const RD_PACKAGE_SETTLEMENT_ECOMMERCE: &str = "sales_rd_package_settlement_ecommerce";
const SQL_PACKAGE_SETTLEMENT_ECOMMERCE: &str = "WITH scope AS(SELECT ?::text company_id),orders AS MATERIALIZED(SELECT native.* FROM (SELECT native.* FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native WHERE native.id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) native,scope WHERE native.company_id=scope.company_id),deliveries AS MATERIALIZED(SELECT native.* FROM (SELECT native.* FROM (SELECT delivery.*,parent.company_id,parent.branch_id,parent.department_id,parent.party_id,parent.kind order_kind,parent.currency order_currency,parent.warehouse_id FROM commerce_fulfillments delivery JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) native,scope WHERE native.company_id=scope.company_id)\n SELECT 'order' kind,o.id,to_jsonb(o) fact,\n NOT COALESCE((o.state='cancelled' AND NOT EXISTS(SELECT 1 FROM deliveries f WHERE f.order_id=o.id) AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=o.id AND audit.record_version=o.version AND audit.action=receipt.operation WHERE receipt.operation='commerce.order.cancel' AND receipt.record_id=o.id AND receipt.authority_json::jsonb->>'companyId'=o.company_id AND receipt.result_json::jsonb->>'state'='cancelled' AND receipt.result_json::jsonb->>'version'=o.version::text))\n OR(o.kind='quote' AND o.state='converted' AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN orders child ON child.id=receipt.result_json::jsonb->>'salesId' AND child.kind='sales' AND child.source_id=o.id AND child.party_id=o.party_id AND child.branch_id=o.branch_id AND child.department_id IS NOT DISTINCT FROM o.department_id WHERE receipt.record_id=o.id AND receipt.operation='commerce.order.convert' AND receipt.authority_json::jsonb->>'companyId'=o.company_id AND receipt.result_json::jsonb->>'state'='converted' AND receipt.result_json::jsonb->>'version'=o.version::text))\n OR(o.kind='sales' AND o.state='fulfilled' AND EXISTS(SELECT 1 FROM deliveries f WHERE f.order_id=o.id) AND NOT EXISTS(SELECT 1 FROM (SELECT native.* FROM (SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')) native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) line WHERE line.order_id=o.id AND (line.quantity_milli<>line.fulfilled_milli OR line.quantity_milli<>COALESCE((SELECT sum(delivered.quantity_milli) FROM (SELECT native.* FROM (SELECT line.*,delivery.order_id,original.product_id FROM commerce_fulfillment_lines line JOIN commerce_fulfillments delivery ON delivery.id=line.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id LEFT JOIN commerce_order_lines original ON original.id=line.order_line_id WHERE parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) delivered WHERE delivered.order_line_id=line.id AND delivered.order_id=o.id),0))) AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=o.id AND audit.record_version::text=receipt.result_json::jsonb->>'version' AND audit.action=receipt.operation WHERE receipt.record_id=o.id AND receipt.operation='commerce.order.fulfill' AND receipt.authority_json::jsonb->>'companyId'=o.company_id AND (receipt.result_json::jsonb->>'version')::bigint<=o.version AND EXISTS(SELECT 1 FROM deliveries delivered WHERE delivered.id=receipt.result_json::jsonb->>'fulfillmentId' AND delivered.order_id=o.id))),false) pending FROM orders o\n UNION ALL SELECT 'line',line.id,to_jsonb(line),product.id IS NULL OR product.company_id<>o.company_id FROM (SELECT native.* FROM (SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')) native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) line JOIN orders o ON o.id=line.order_id LEFT JOIN (SELECT id,company_id,item_type,tracking_mode,status FROM commerce_products) product ON product.id=line.product_id\n UNION ALL SELECT 'delivery-line',line.id,to_jsonb(line),original.id IS NULL OR original.order_id<>delivery.order_id OR original.product_id IS DISTINCT FROM line.product_id FROM (SELECT native.* FROM (SELECT line.*,delivery.order_id,original.product_id FROM commerce_fulfillment_lines line JOIN commerce_fulfillments delivery ON delivery.id=line.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id LEFT JOIN commerce_order_lines original ON original.id=line.order_line_id WHERE parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) line JOIN deliveries delivery ON delivery.id=line.fulfillment_id LEFT JOIN (SELECT native.* FROM (SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')) native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) original ON original.id=line.order_line_id\n UNION ALL SELECT 'delivery-evidence',concat(evidence.fulfillment_id,':',evidence.attachment_id),to_jsonb(evidence),false FROM commerce_fulfillment_evidence evidence JOIN deliveries delivery ON delivery.id=evidence.fulfillment_id\n UNION ALL SELECT 'return-line',line.id,to_jsonb(line),false FROM (SELECT native.* FROM (SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.return_id IN (SELECT id FROM (SELECT native.* FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source)) line JOIN (SELECT native.* FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source ON source.id=line.return_id CROSS JOIN scope WHERE source.company_id=scope.company_id OR source.source_company=scope.company_id\n UNION ALL SELECT 'return-lot',lot.id,to_jsonb(lot),false FROM (SELECT native.* FROM (SELECT allocation.* FROM commerce_return_lot_allocations allocation JOIN commerce_return_lines line ON line.id=allocation.return_line_id JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.return_line_id IN (SELECT id FROM (SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') line WHERE line.return_id IN (SELECT id FROM (SELECT native.* FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source))) lot JOIN (SELECT native.* FROM (SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.return_id IN (SELECT id FROM (SELECT native.* FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source)) line ON line.id=lot.return_line_id JOIN (SELECT native.* FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source ON source.id=line.return_id CROSS JOIN scope WHERE source.company_id=scope.company_id OR source.source_company=scope.company_id\n UNION ALL SELECT 'goods-valuation',valuation.fulfillment_id,to_jsonb(valuation),valuation.company_id<>delivery.company_id FROM (SELECT native.* FROM (SELECT valuation.*,parent.company_id source_company FROM commerce_goods_valuations valuation JOIN commerce_fulfillments delivery ON delivery.id=valuation.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') native WHERE native.fulfillment_id IN (SELECT id FROM (SELECT native.* FROM (SELECT delivery.*,parent.company_id,parent.branch_id,parent.department_id,parent.party_id,parent.kind order_kind,parent.currency order_currency,parent.warehouse_id FROM commerce_fulfillments delivery JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source)) valuation JOIN deliveries delivery ON delivery.id=valuation.fulfillment_id\n UNION ALL SELECT 'invoice-carrying',carrying.invoice_id,to_jsonb(carrying),carrying.company_id<>carrying.source_company FROM (SELECT native.* FROM (SELECT carrying.*,parent.company_id source_company FROM commerce_invoice_goods_carrying carrying JOIN commerce_fulfillments delivery ON delivery.invoice_id=carrying.invoice_id JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') native WHERE native.invoice_id IN (SELECT invoice_id FROM (SELECT native.* FROM (SELECT delivery.*,parent.company_id,parent.branch_id,parent.department_id,parent.party_id,parent.kind order_kind,parent.currency order_currency,parent.warehouse_id FROM commerce_fulfillments delivery JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) source)) carrying,scope WHERE carrying.company_id=scope.company_id OR carrying.source_company=scope.company_id\n UNION ALL SELECT 'price-item',item.id,to_jsonb(item),false FROM sales_price_list_items item JOIN sales_price_lists list ON list.id=item.price_list_id CROSS JOIN scope WHERE list.company_id=scope.company_id AND FALSE\n UNION ALL SELECT 'contact-note',note.id,to_jsonb(note),NOT EXISTS(SELECT 1 FROM orders original WHERE original.id=note.order_id AND original.company_id=note.company_id AND original.party_id=note.customer_id) FROM customer_contact_notes note,scope WHERE note.company_id=scope.company_id AND EXISTS(SELECT 1 FROM orders o WHERE o.id=note.order_id)\n UNION ALL SELECT 'contact-note-history',event.id,to_jsonb(event),false FROM customer_contact_note_history event JOIN customer_contact_notes note ON note.id=event.note_id CROSS JOIN scope WHERE note.company_id=scope.company_id AND EXISTS(SELECT 1 FROM orders o WHERE o.id=note.order_id)\n UNION ALL SELECT 'followup-history',event.id,to_jsonb(event),false FROM customer_followup_events event JOIN customer_followups followup ON followup.id=event.followup_id CROSS JOIN scope WHERE followup.company_id=scope.company_id AND EXISTS(SELECT 1 FROM orders o WHERE o.id=followup.order_id)\n UNION ALL SELECT 'warehouse',o.id,to_jsonb(warehouse),warehouse.id IS NULL OR warehouse.company_id<>o.company_id OR warehouse.branch_id<>o.branch_id FROM orders o LEFT JOIN (SELECT id,company_id,branch_id,status FROM commerce_warehouses) warehouse ON warehouse.id=o.warehouse_id\n UNION ALL SELECT 'delivery',f.id,to_jsonb(f),f.invoice_id IS NULL OR NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.order.fulfill' AND receipt.record_id=f.order_id AND receipt.authority_json::jsonb->>'companyId'=f.company_id AND receipt.result_json::jsonb->>'fulfillmentId'=f.id) FROM deliveries f\n UNION ALL SELECT 'return',r.id,to_jsonb(r),NOT COALESCE(r.company_id=scope.company_id AND r.source_company=scope.company_id AND r.order_kind='sales' AND r.kind='customer_return' AND EXISTS(SELECT 1 FROM deliveries f WHERE f.id=r.fulfillment_id AND f.order_id=r.order_id AND f.invoice_id=r.invoice_id) AND r.status IN ('posted','rejected') AND EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=r.id AND audit.record_version=r.version AND audit.action=receipt.operation WHERE receipt.record_id=r.id AND receipt.operation=CASE r.status WHEN 'posted' THEN 'commerce.return.complete' ELSE 'commerce.return.reject' END AND receipt.authority_json::jsonb->>'companyId'=r.company_id AND receipt.result_json::jsonb->>'status'=r.status AND receipt.result_json::jsonb->>'version'=r.version::text),false) FROM (SELECT native.* FROM (SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales') native WHERE native.order_id IN (SELECT native.id FROM (SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')) native JOIN (SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart) root ON root.order_id=native.id AND root.company_id=native.company_id)) r,scope WHERE r.company_id=scope.company_id OR r.source_company=scope.company_id\n UNION ALL SELECT 'pricing',list.id,to_jsonb(list),list.status='draft' OR NOT EXISTS(SELECT 1 FROM command_receipts receipt JOIN audit_events audit ON audit.command_id=receipt.command_id AND audit.record_id=list.id AND audit.record_version=list.version WHERE receipt.record_id=list.id AND receipt.operation=CASE list.status WHEN 'active' THEN 'sales.price-list.activate' WHEN 'cancelled' THEN 'sales.price-list.cancel' END AND receipt.authority_json::jsonb->>'companyId'=list.company_id AND receipt.result_json::jsonb->>'version'=list.version::text AND receipt.result_json::jsonb->>'status'=list.status AND (list.status<>'active' OR receipt.actor_id=list.approved_by AND list.approved_by<>list.prepared_by)) FROM sales_price_lists list,scope WHERE list.company_id=scope.company_id AND FALSE\n UNION ALL SELECT 'followup',followup.id,to_jsonb(followup),followup.status='open' OR NOT EXISTS(SELECT 1 FROM orders o JOIN customer_followup_events event ON event.followup_id=followup.id AND event.version=followup.version AND event.action=CASE followup.status WHEN 'completed' THEN 'complete' WHEN 'cancelled' THEN 'cancel' END JOIN command_receipts receipt ON event.id='followup-event-'||receipt.command_id AND receipt.operation='customer.followup.'||event.action AND receipt.record_id=followup.id AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=followup.company_id WHERE o.id=followup.order_id AND receipt.result_json::jsonb->>'status'=followup.status AND receipt.result_json::jsonb->>'version'=followup.version::text) FROM customer_followups followup,scope WHERE followup.company_id=scope.company_id AND EXISTS(SELECT 1 FROM orders o WHERE o.id=followup.order_id)\n UNION ALL SELECT 'customer-notice',notice.id,to_jsonb(notice),notice.status NOT IN ('sent','cancelled') OR NOT EXISTS(SELECT 1 FROM deliveries f WHERE f.id=notice.fulfillment_id AND f.order_id=notice.order_id AND f.company_id=notice.company_id AND f.party_id=notice.party_id) OR NOT EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='commerce.customer-notification.queue' AND receipt.record_id=notice.fulfillment_id AND receipt.actor_id=notice.created_by AND receipt.authority_json::jsonb->>'companyId'=notice.company_id AND receipt.result_json::jsonb->>'id'=notice.id) OR (notice.status='sent' AND COALESCE(notice.provider_message_id,'')='') OR EXISTS(SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.notification_id=notice.id AND (attempt.company_id<>notice.company_id OR (attempt.state='sending' OR attempt.state='unknown' AND NOT EXISTS(SELECT 1 FROM sales_notification_reconciliations resolution JOIN command_receipts receipt ON receipt.command_id=resolution.command_id AND receipt.record_id=notice.id AND receipt.operation='commerce.customer-notification.reconcile' AND receipt.actor_id=resolution.actor_id AND receipt.authority_json::jsonb->>'companyId'=notice.company_id JOIN audit_events audit ON audit.command_id=resolution.command_id AND audit.record_id=notice.id AND audit.record_version=resolution.notification_version AND audit.action=receipt.operation JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id WHERE resolution.attempt_id=attempt.id AND resolution.notification_id=notice.id AND resolution.company_id=notice.company_id AND resolution.source_hash=attempt.source_hash AND resolution.provider_message_id=attempt.provider_message_id AND resolution.provider_message_id=notice.provider_message_id AND resolution.actor_id<>authority.actor_id AND resolution.actor_id<>notice.created_by AND resolution.notification_version=notice.version AND resolution.outcome=notice.status AND receipt.result_json::jsonb->>'reconciliationId'=resolution.id AND receipt.result_json::jsonb->>'status'=resolution.outcome AND receipt.result_json::jsonb->>'version'=resolution.notification_version::text AND resolution.provider_observation_json::jsonb->>'id'=resolution.provider_message_id AND resolution.provider_observation_json::jsonb->'to'=jsonb_build_array(notice.recipient_email) AND resolution.provider_observation_json::jsonb->>'subject'=notice.subject AND resolution.provider_observation_json::jsonb->>'text'=notice.body_text AND resolution.provider_observation_json::jsonb->>'last_event'=resolution.provider_event AND resolution.outcome=CASE WHEN resolution.provider_event IN ('sent','delivered','opened','clicked') THEN 'sent' ELSE 'cancelled' END)))) FROM commerce_customer_notifications notice,scope WHERE notice.company_id=scope.company_id AND EXISTS(SELECT 1 FROM orders o WHERE o.id=notice.order_id)\n UNION ALL SELECT 'notice-authorization',authority.command_id,to_jsonb(authority),authority.company_id<>notice.company_id FROM sales_notification_execution_authorizations authority JOIN commerce_customer_notifications notice ON notice.id=authority.notification_id CROSS JOIN scope WHERE (authority.company_id=scope.company_id OR notice.company_id=scope.company_id) AND EXISTS(SELECT 1 FROM orders o WHERE o.id=notice.order_id)\n UNION ALL SELECT 'notice-attempt',attempt.id,to_jsonb(attempt),attempt.company_id<>notice.company_id FROM sales_notification_execution_attempts attempt JOIN commerce_customer_notifications notice ON notice.id=attempt.notification_id CROSS JOIN scope WHERE (attempt.company_id=scope.company_id OR notice.company_id=scope.company_id) AND EXISTS(SELECT 1 FROM orders o WHERE o.id=notice.order_id)\n UNION ALL SELECT 'notice-reconciliation',resolution.id,to_jsonb(resolution),resolution.company_id<>notice.company_id OR NOT EXISTS(SELECT 1 FROM sales_notification_execution_attempts attempt WHERE attempt.id=resolution.attempt_id AND attempt.notification_id=notice.id AND attempt.company_id=notice.company_id AND attempt.state='unknown' AND EXISTS(SELECT 1 FROM sales_notification_reconciliations resolution JOIN command_receipts receipt ON receipt.command_id=resolution.command_id AND receipt.record_id=notice.id AND receipt.operation='commerce.customer-notification.reconcile' AND receipt.actor_id=resolution.actor_id AND receipt.authority_json::jsonb->>'companyId'=notice.company_id JOIN audit_events audit ON audit.command_id=resolution.command_id AND audit.record_id=notice.id AND audit.record_version=resolution.notification_version AND audit.action=receipt.operation JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id WHERE resolution.attempt_id=attempt.id AND resolution.notification_id=notice.id AND resolution.company_id=notice.company_id AND resolution.source_hash=attempt.source_hash AND resolution.provider_message_id=attempt.provider_message_id AND resolution.provider_message_id=notice.provider_message_id AND resolution.actor_id<>authority.actor_id AND resolution.actor_id<>notice.created_by AND resolution.notification_version=notice.version AND resolution.outcome=notice.status AND receipt.result_json::jsonb->>'reconciliationId'=resolution.id AND receipt.result_json::jsonb->>'status'=resolution.outcome AND receipt.result_json::jsonb->>'version'=resolution.notification_version::text AND resolution.provider_observation_json::jsonb->>'id'=resolution.provider_message_id AND resolution.provider_observation_json::jsonb->'to'=jsonb_build_array(notice.recipient_email) AND resolution.provider_observation_json::jsonb->>'subject'=notice.subject AND resolution.provider_observation_json::jsonb->>'text'=notice.body_text AND resolution.provider_observation_json::jsonb->>'last_event'=resolution.provider_event AND resolution.outcome=CASE WHEN resolution.provider_event IN ('sent','delivered','opened','clicked') THEN 'sent' ELSE 'cancelled' END)) FROM sales_notification_reconciliations resolution JOIN commerce_customer_notifications notice ON notice.id=resolution.notification_id CROSS JOIN scope WHERE (resolution.company_id=scope.company_id OR notice.company_id=scope.company_id) AND EXISTS(SELECT 1 FROM orders o WHERE o.id=notice.order_id)\n UNION ALL SELECT 'notice-provider',settings.company_id,to_jsonb(settings),settings.adapter<>'disabled' FROM commerce_customer_notification_settings settings,scope WHERE settings.company_id=scope.company_id AND FALSE ORDER BY kind,id";
const READ_PACKAGE_SETTLEMENT_ECOMMERCE: NativeSalesReadStatement = NativeSalesReadStatement {
    kind: RD_PACKAGE_SETTLEMENT_ECOMMERCE,
    sql: SQL_PACKAGE_SETTLEMENT_ECOMMERCE,
    bind_count: 1,
    scope: "holding",
};

const READS: &[NativeSalesReadStatement] = &[
    READ_PRICE_LIST_ACTIVE,
    READ_PRICE_LIST_ITEMS_ACTIVE,
    READ_NOTIFICATION_ATTEMPT,
    READ_NOTIFICATION_RECONCILIATION_INSTALLED,
    READ_FOLLOWUP_RECIPIENTS,
    READ_PRICING_LIST_BY_ID,
    READ_PRICING_LIST_ACTIVE_BY_ID,
    READ_PRICING_LIST_ACTIVE,
    READ_PRICING_ITEMS_BY_LIST,
    READ_FOLLOWUP_SOURCE_FOLLOWUP,
    READ_FOLLOWUP_SOURCE_ORDER,

    READ_FOLLOWUP_METRIC_SELECT,
    READ_FOLLOWUP_METRIC_SOURCE,

    READ_NOTIFICATION_SOURCE_DEV,
    READ_NOTIFICATION_SOURCE_PROD,

    READ_CALENDAR_JSONB_MEMBER,
    READ_CALENDAR_OWNER_MEMBER,

    READ_FOLLOWUP_ORDERS,

    READ_PURCHASE_PROJECTION_C_READ,
    READ_PURCHASE_PROJECTION_C_DENY,
    READ_PURCHASE_PROJECTION_FP_READ,
    READ_PURCHASE_PROJECTION_FP_DENY,

    READ_CRM_ASSIGNMENT_PLAIN,
    READ_CRM_ASSIGNMENT_ALIASED,

    READ_PACKAGE_SETTLEMENT_SALES,
    READ_PACKAGE_SETTLEMENT_ECOMMERCE,

];

/// Whether a kind is an allow-listed owner read (never a write command).
pub fn is_read(kind: &str) -> bool {
    read_statement(kind).is_some()
}

/// Resolve an owner read kind to its planned statement.
pub fn read_statement(kind: &str) -> Option<&'static NativeSalesReadStatement> {
    READS.iter().find(|statement| statement.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn followup_recipients_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_RECIPIENTS).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_recipients");
        assert_eq!(read.bind_count, 8);
        assert_eq!(read.sql.matches('?').count(), 8);
        assert_eq!(read.sql.len(), 2774);
        assert!(read.sql.starts_with("SELECT DISTINCT c.owner_id id,p.legal_name name"));
        assert!(read.sql.ends_with("ORDER BY p.legal_name"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_RECIPIENTS));
    }

    #[test]
    fn price_list_read_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_PRICE_LIST_ACTIVE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICE_LIST_ACTIVE));
        assert!(!is_read("sales_stmt_1"));
        assert!(read_statement("not_a_native_read").is_none());
    }

    #[test]
    fn price_list_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_PRICE_LIST_ACTIVE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT id,name,customer_id,valid_from,valid_until,max_auto_discount_bps,version FROM sales_price_lists WHERE company_id=? AND status='active' ORDER BY customer_id NULLS LAST,name"
        );
    }

    #[test]
    fn price_list_items_read_plan_is_holding_scoped_and_ledger_free() {
        let read = read_statement(RD_PRICE_LIST_ITEMS_ACTIVE).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICE_LIST_ITEMS_ACTIVE));
    }

    #[test]
    fn price_list_items_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_PRICE_LIST_ITEMS_ACTIVE).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT i.* FROM sales_price_list_items i JOIN sales_price_lists l ON l.id=i.price_list_id WHERE l.company_id=? AND l.status='active' ORDER BY i.price_list_id,i.ordinal"
        );
    }

    #[test]
    fn notification_attempt_read_plan_is_holding_scoped_with_three_binds() {
        let read = read_statement(RD_NOTIFICATION_ATTEMPT).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 3);
        assert_eq!(read.sql.matches('?').count(), 3);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_NOTIFICATION_ATTEMPT));
    }

    #[test]
    fn notification_attempt_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_NOTIFICATION_ATTEMPT).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT attempt.id,attempt.source_hash,attempt.provider_message_id,authority.actor_id FROM sales_notification_execution_attempts attempt JOIN sales_notification_execution_authorizations authority ON authority.command_id=attempt.authorization_command_id AND authority.company_id=attempt.company_id AND authority.notification_id=attempt.notification_id WHERE attempt.notification_id=? AND attempt.company_id=? AND attempt.attempt=? AND attempt.state='unknown'"
        );
        assert_eq!(read.sql.len(), 456);
    }

    #[test]
    fn reconciliation_installed_read_plan_is_holding_scoped_with_no_binds() {
        let read = read_statement(RD_NOTIFICATION_RECONCILIATION_INSTALLED).expect("read kind");
        let upper = read.sql.to_ascii_uppercase();
        assert!(upper.starts_with("SELECT"), "{}", read.kind);
        assert!(!upper.contains("INSERT"));
        assert!(!upper.contains("UPDATE"));
        assert!(!upper.contains("DELETE"));
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 0);
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_NOTIFICATION_RECONCILIATION_INSTALLED));
    }

    #[test]
    fn reconciliation_installed_read_sql_matches_retired_typescript_literal() {
        let read = read_statement(RD_NOTIFICATION_RECONCILIATION_INSTALLED).expect("read kind");
        assert_eq!(
            read.sql,
            "SELECT to_regclass('sales_notification_reconciliations') IS NOT NULL installed"
        );
        assert_eq!(read.sql.len(), 78);
    }

    #[test]
    fn byId_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_PRICING_LIST_BY_ID).expect("read kind");
        assert_eq!(read.kind, "sales_rd_pricing_list_by_id");
        assert_eq!(read.bind_count, 2);
        assert_eq!(read.sql.matches('?').count(), 2);
        assert_eq!(read.sql.len(), 59);
        assert!(read.sql.starts_with("SELECT"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICING_LIST_BY_ID));
    }

    #[test]
    fn byIdActive_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_PRICING_LIST_ACTIVE_BY_ID).expect("read kind");
        assert_eq!(read.kind, "sales_rd_pricing_list_active_by_id");
        assert_eq!(read.bind_count, 6);
        assert_eq!(read.sql.matches('?').count(), 6);
        assert_eq!(read.sql.len(), 493);
        assert!(read.sql.starts_with("SELECT"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICING_LIST_ACTIVE_BY_ID));
    }

    #[test]
    fn active_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_PRICING_LIST_ACTIVE).expect("read kind");
        assert_eq!(read.kind, "sales_rd_pricing_list_active");
        assert_eq!(read.bind_count, 6);
        assert_eq!(read.sql.matches('?').count(), 6);
        assert_eq!(read.sql.len(), 577);
        assert!(read.sql.starts_with("SELECT"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICING_LIST_ACTIVE));
    }

    #[test]
    fn items_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_PRICING_ITEMS_BY_LIST).expect("read kind");
        assert_eq!(read.kind, "sales_rd_pricing_items_by_list");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 58);
        assert!(read.sql.starts_with("SELECT"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PRICING_ITEMS_BY_LIST));
    }

    #[test]
    fn followup_source_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_SOURCE_FOLLOWUP).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_source_followup");
        assert_eq!(read.bind_count, 6);
        assert_eq!(read.sql.matches('?').count(), 6);
        assert_eq!(read.sql.len(), 1066);
        assert!(read.sql.starts_with("SELECT id,company_id,branch_id,department_id"));
        assert!(read.sql.ends_with("AND (?::text IS NULL OR department_id=?)"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_SOURCE_FOLLOWUP));
    }

    #[test]
    fn order_source_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_SOURCE_ORDER).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_source_order");
        assert_eq!(read.bind_count, 6);
        assert_eq!(read.sql.matches('?').count(), 6);
        assert_eq!(read.sql.len(), 815);
        assert!(read.sql.starts_with("SELECT id,company_id,branch_id,department_id"));
        assert!(read.sql.ends_with("AND (?::text IS NULL OR department_id=?)"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_SOURCE_ORDER));
    }

    #[test]
    fn projection_metric_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_METRIC_PROJECTION).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_metric_projection");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 0);
        assert_eq!(read.sql.len(), 297);
        assert!(read.sql.contains("customer_followups followup"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_METRIC_PROJECTION));
    }

    #[test]
    fn enforced_metric_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_METRIC_SOURCE).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_metric_source");
        assert_eq!(read.bind_count, 4);
        assert_eq!(read.sql.matches('?').count(), 4);
        assert_eq!(read.sql.len(), 2666);
        assert!(read.sql.contains("customer_followups followup"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_METRIC_SOURCE));
    }

    #[test]
    fn notification_source_dev_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_NOTIFICATION_SOURCE_DEV).expect("read kind");
        assert_eq!(read.kind, "sales_rd_notification_source_dev");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 1438);
        assert!(read.sql.starts_with("SELECT n.*,delivery.department_id"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_NOTIFICATION_SOURCE_DEV));
    }

    #[test]
    fn notification_source_prod_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_NOTIFICATION_SOURCE_PROD).expect("read kind");
        assert_eq!(read.kind, "sales_rd_notification_source_prod");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 1439);
        assert!(read.sql.starts_with("SELECT n.*,delivery.department_id"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_NOTIFICATION_SOURCE_PROD));
    }

    #[test]
    fn calendar_jsonb_member_fragment_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_CALENDAR_JSONB_MEMBER).expect("read kind");
        assert_eq!(read.kind, "sales_rd_calendar_jsonb_member");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 47);
        assert!(read.sql.starts_with("IN"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_CALENDAR_JSONB_MEMBER));
    }

    #[test]
    fn calendar_owner_member_fragment_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_CALENDAR_OWNER_MEMBER).expect("read kind");
        assert_eq!(read.kind, "sales_rd_calendar_owner_member");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 286);
        assert!(read.sql.starts_with("IN"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_CALENDAR_OWNER_MEMBER));
    }

    #[test]
    fn followup_orders_source_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_FOLLOWUP_ORDERS).expect("read kind");
        assert_eq!(read.kind, "sales_rd_followup_orders");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 0);
        assert_eq!(read.sql.len(), 476);
        assert!(read.sql.starts_with("SELECT actual.*,jsonb_build_array("));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_FOLLOWUP_ORDERS));
    }

    #[test]
    fn purchase_projection_c_true_is_byte_stable() {
        let read = read_statement(RD_PURCHASE_PROJECTION_C_READ).expect("read kind");
        assert_eq!(read.kind, "sales_rd_purchase_projection_c_read");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 7);
        assert_eq!(read.sql.len(), 2920);
        assert!(read.sql.starts_with("EXISTS(WITH followup_clock AS MATERIALIZED("));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PURCHASE_PROJECTION_C_READ));
    }

    #[test]
    fn purchase_projection_c_false_is_byte_stable() {
        let read = read_statement(RD_PURCHASE_PROJECTION_C_DENY).expect("read kind");
        assert_eq!(read.kind, "sales_rd_purchase_projection_c_deny");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 7);
        assert_eq!(read.sql.len(), 2921);
        assert!(read.sql.starts_with("EXISTS(WITH followup_clock AS MATERIALIZED("));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PURCHASE_PROJECTION_C_DENY));
    }

    #[test]
    fn purchase_projection_followup_projection_true_is_byte_stable() {
        let read = read_statement(RD_PURCHASE_PROJECTION_FP_READ).expect("read kind");
        assert_eq!(read.kind, "sales_rd_purchase_projection_fp_read");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 7);
        assert_eq!(read.sql.len(), 2956);
        assert!(read.sql.starts_with("EXISTS(WITH followup_clock AS MATERIALIZED("));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PURCHASE_PROJECTION_FP_READ));
    }

    #[test]
    fn purchase_projection_followup_projection_false_is_byte_stable() {
        let read = read_statement(RD_PURCHASE_PROJECTION_FP_DENY).expect("read kind");
        assert_eq!(read.kind, "sales_rd_purchase_projection_fp_deny");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 7);
        assert_eq!(read.sql.len(), 2957);
        assert!(read.sql.starts_with("EXISTS(WITH followup_clock AS MATERIALIZED("));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PURCHASE_PROJECTION_FP_DENY));
    }

    #[test]
    fn crm_assignment_plain_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_CRM_ASSIGNMENT_PLAIN).expect("read kind");
        assert_eq!(read.kind, "sales_rd_crm_assignment_plain");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 149);
        assert!(read.sql.starts_with("EXISTS(SELECT 1 FROM assignments"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_CRM_ASSIGNMENT_PLAIN));
    }

    #[test]
    fn crm_assignment_aliased_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_CRM_ASSIGNMENT_ALIASED).expect("read kind");
        assert_eq!(read.kind, "sales_rd_crm_assignment_aliased");
        assert_eq!(read.bind_count, 0);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 185);
        assert!(read.sql.starts_with("EXISTS(SELECT 1 FROM assignments"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_CRM_ASSIGNMENT_ALIASED));
    }

    #[test]
    fn package_settlement_sales_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_PACKAGE_SETTLEMENT_SALES).expect("read kind");
        assert_eq!(read.kind, "sales_rd_package_settlement_sales");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 18424);
        assert!(read.sql.starts_with("WITH scope AS"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PACKAGE_SETTLEMENT_SALES));
    }

    #[test]
    fn package_settlement_ecommerce_read_is_allow_listed_and_byte_stable() {
        let read = read_statement(RD_PACKAGE_SETTLEMENT_ECOMMERCE).expect("read kind");
        assert_eq!(read.kind, "sales_rd_package_settlement_ecommerce");
        assert_eq!(read.bind_count, 1);
        assert_eq!(read.sql.matches('?').count(), 1);
        assert_eq!(read.sql.len(), 26250);
        assert!(read.sql.starts_with("WITH scope AS"));
        assert_eq!(read.scope, "holding");
        assert!(is_read(RD_PACKAGE_SETTLEMENT_ECOMMERCE));
    }

    #[test]
    fn only_owner_read_kinds_are_registered() {
        assert_eq!(READS.len(), 26);
        assert!(READS.iter().all(|read| is_read(read.kind)));
        assert!(!is_read("sales_price_lists"));
        assert!(!is_read("SELECT 1"));
    }
}
