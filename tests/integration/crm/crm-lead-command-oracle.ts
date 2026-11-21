/**
 * Native-seeding oracle for retained CRM lead qualification and PMS win reads.
 *
 * The TypeScript `qualify` and `win-pms` writers were retired with the native
 * CRM lead command (`packages/server-crm/src/adapters/leads_native.rs` + the
 * composition `CrmLeadCommand` effects). This helper is test-only. It writes
 * the same `crm_leads`, `crm_opportunities` and immutable `crm_case_events`
 * rows as the native owner adapter so the retained CRM detail, Work, forecast
 * and analytics assertions remain meaningful.
 *
 * It also re-reads the owner witnesses the native composition supplies:
 * Finance customer status through the reviewed account link, Commerce
 * company currency, current Sales recipient eligibility and the confirmed PMS
 * reservation/property/guest-sharing source. It intentionally does not
 * emulate Workflow receipts, package purchase enforcement, Account session
 * resolution or collaboration notifications; those native command paths are
 * covered by the Rust owner and composition tests. Do not import this module
 * outside tests.
 */
import type {SqlDatabase,TransactionalSqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {salesRecipientEligibility} from "@erpx/server-sales/public/sales-recipient";

function failure(code: string, status: number): never {
  throw Object.assign(new Error(code), {code, status});
}

type LeadRow = Readonly<{
  id: string;
  owner_id: string;
  status: string;
  version: number;
  branch_id: string;
  department_id: string | null;
}>;

type OpportunityRow = Readonly<{
  id: string;
  stage: string;
  version: number;
}>;

export type NativeLeadScope = Readonly<{
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  actorId: string;
  now?: number;
}>;

const leadForUpdate = (
  tx: SqlDatabase,
  leadId: string,
  scope: NativeLeadScope,
): Promise<LeadRow | null> =>
  tx
    .prepare(
      "SELECT l.id,l.owner_id,l.status,l.version,l.branch_id,l.department_id FROM crm_leads l WHERE l.id=? AND l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) FOR UPDATE OF l",
    )
    .bind(leadId, scope.companyId, scope.branchId, scope.branchId, scope.departmentId, scope.departmentId)
    .first<LeadRow>();

const requireRecipient = async (
  tx: SqlDatabase,
  leadId: string,
): Promise<void> => {
  const eligible = await tx
    .prepare(
      `SELECT EXISTS(SELECT 1 FROM crm_leads l WHERE l.id=? AND ${salesRecipientEligibility("l", "l.owner_id")}) eligible`,
    )
    .bind(leadId)
    .first<{eligible: boolean}>();
  if (!eligible?.eligible) failure("CRM_RECIPIENT_REQUIRED", 403);
};

export type NativeQualify = Readonly<{
  key: string;
  leadId: string;
  customerId: string;
  expectedVersion: number;
  expectedValue: string;
  closeDate: string;
  reason: string;
  scope: NativeLeadScope;
}>;

/**
 * Write the native qualification after-image: one opportunity, the qualified
 * lead and both immutable case events. The opportunity id is the native
 * deterministic `opportunity-${key}`.
 */
export async function qualifyNativeLead(
  db: TransactionalSqlDatabase,
  input: NativeQualify,
): Promise<{id: string; version: number; opportunityId: string}> {
  return db.transaction(async (tx) => {
    const {scope} = input;
    const opportunityId = `opportunity-${input.key}`;
    const dealEvent = `crm-event-${input.key}-deal`;
    const leadEvent = `crm-event-${input.key}-lead`;
    const lead = await leadForUpdate(tx, input.leadId, scope);
    if (!lead) failure("CRM_LEAD_NOT_FOUND", 404);
    const prior = await tx
      .prepare("SELECT 1 FROM crm_case_events WHERE id=?")
      .bind(dealEvent)
      .first();
    if (prior)
      return {
        id: input.leadId,
        version: Number(lead.version),
        opportunityId,
      };
    if (Number(lead.version) !== input.expectedVersion)
      failure("CRM_LEAD_VERSION_CHANGED", 409);
    if (lead.owner_id !== scope.actorId) failure("CRM_OWNER_REQUIRED", 403);
    if (lead.status !== "open") failure("CRM_STATE_CHANGED", 409);
    await requireRecipient(tx, input.leadId);
    const customer = await tx
      .prepare(
        "SELECT p.id FROM crm_leads l JOIN crm_accounts a ON a.id=l.account_id AND a.company_id=l.company_id JOIN finance_parties p ON p.id=a.finance_party_id AND p.company_id=l.company_id WHERE l.id=? AND l.company_id=? AND p.id=? AND p.kind='customer' AND p.status='active' AND a.reviewed_by IS NOT NULL",
      )
      .bind(input.leadId, scope.companyId, input.customerId)
      .first<{id: string}>();
    if (!customer) failure("CRM_SOURCE_CHANGED", 409);
    const settings = await tx
      .prepare("SELECT currency FROM commerce_settings WHERE company_id=?")
      .bind(scope.companyId)
      .first<{currency: string}>();
    if (!settings?.currency) failure("CRM_CURRENCY_REQUIRED", 409);
    const [whole, fraction = ""] = input.expectedValue.split(".");
    const expectedMinor =
      Number(whole) * 100 + Number((fraction + "00").slice(0, 2));
    const now = scope.now ?? Date.now();
    const reason = input.reason.trim();
    await tx
      .prepare(
        "INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        opportunityId,
        input.leadId,
        input.customerId,
        expectedMinor,
        settings.currency,
        input.closeDate,
        scope.actorId,
        now,
        now,
      )
      .run();
    const updated = await tx
      .prepare(
        "UPDATE crm_leads SET status='qualified',version=version+1,updated_at=? WHERE id=? AND company_id=? AND owner_id=? AND status='open' AND version=?",
      )
      .bind(now, input.leadId, scope.companyId, scope.actorId, input.expectedVersion)
      .run();
    if (updated.meta.changes !== 1) failure("CRM_SOURCE_CHANGED", 409);
    const snapshot = JSON.stringify({
      action: "qualify",
      expectedVersion: input.expectedVersion,
      customerId: input.customerId,
      expectedValue: input.expectedValue,
      closeDate: input.closeDate,
      reason,
    });
    await tx
      .prepare(
        "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,?,1,'qualify',?,?,?,?)",
      )
      .bind(
        dealEvent,
        input.leadId,
        opportunityId,
        scope.actorId,
        reason,
        snapshot,
        now,
      )
      .run();
    await tx
      .prepare(
        "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,NULL,?,'qualify',?,?,?,?)",
      )
      .bind(
        leadEvent,
        input.leadId,
        input.expectedVersion + 1,
        scope.actorId,
        reason,
        snapshot,
        now,
      )
      .run();
    return {
      id: input.leadId,
      version: input.expectedVersion + 1,
      opportunityId,
    };
  });
}

export type NativeWinPms = Readonly<{
  key: string;
  leadId: string;
  reservationId: string;
  propertyId: string;
  expectedVersion: number;
  leadVersion: number;
  reason: string;
  scope: NativeLeadScope;
}>;

/**
 * Write the native PMS win after-image, including the reservation total from
 * the PMS stay records and both immutable case events.
 */
export async function winNativePmsLead(
  db: TransactionalSqlDatabase,
  input: NativeWinPms,
): Promise<{
  id: string;
  version: number;
  leadVersion: number;
  stage: string;
}> {
  return db.transaction(async (tx) => {
    const {scope} = input;
    const lead = await leadForUpdate(tx, input.leadId, scope);
    if (!lead) failure("CRM_LEAD_NOT_FOUND", 404);
    const opportunity = await tx
      .prepare(
        "SELECT id,stage,version FROM crm_opportunities WHERE lead_id=? FOR UPDATE OF crm_opportunities",
      )
      .bind(input.leadId)
      .first<OpportunityRow>();
    if (!opportunity) failure("CRM_OPPORTUNITY_NOT_FOUND", 404);
    const dealEvent = `crm-event-${input.key}-deal`;
    const leadEvent = `crm-event-${input.key}-lead`;
    const prior = await tx
      .prepare("SELECT 1 FROM crm_case_events WHERE id=?")
      .bind(dealEvent)
      .first();
    if (prior)
      return {
        id: input.leadId,
        version: Number(opportunity.version),
        leadVersion: Number(lead.version),
        stage: "won",
      };
    if (Number(lead.version) !== input.leadVersion)
      failure("CRM_LEAD_VERSION_CHANGED", 409);
    if (lead.status !== "qualified") failure("CRM_STATE_CHANGED", 409);
    if (Number(opportunity.version) !== input.expectedVersion)
      failure("CRM_OPPORTUNITY_VERSION_CHANGED", 409);
    if (lead.owner_id !== scope.actorId) failure("CRM_OWNER_REQUIRED", 403);
    if (!["discovery", "proposal", "negotiation"].includes(opportunity.stage))
      failure("CRM_STATE_CHANGED", 409);
    await requireRecipient(tx, input.leadId);
    const contact = await tx
      .prepare("SELECT contact_id FROM crm_leads WHERE id=? AND company_id=?")
      .bind(input.leadId, scope.companyId)
      .first<{contact_id: string | null}>();
    if (!contact?.contact_id) failure("CRM_SOURCE_CHANGED", 409);
    const reservation = await tx
      .prepare(
        "SELECT r.id,COALESCE((SELECT SUM((price->>'net')::bigint) FROM pms_records s CROSS JOIN LATERAL jsonb_array_elements(s.data_json::jsonb->'prices') price WHERE s.kind='stay' AND s.property_id=p.id AND s.data_json::jsonb->>'reservationId'=r.id),0)::bigint expected_minor FROM pms_records r JOIN pms_properties p ON p.id=r.property_id JOIN crm_guest_links g ON g.guest_id=r.data_json::jsonb->>'guestId' AND g.property_id=p.id AND g.company_id=p.company_id WHERE r.id=? AND r.kind='reservation' AND r.property_id=? AND r.data_json::jsonb->>'state'='confirmed' AND p.company_id=? AND p.branch_id=? AND g.sharing_state='granted' AND g.contact_id=?",
      )
      .bind(
        input.reservationId,
        input.propertyId,
        scope.companyId,
        lead.branch_id,
        contact.contact_id,
      )
      .first<{id: string; expected_minor: number}>();
    if (!reservation) failure("CRM_SOURCE_CHANGED", 409);
    const now = Date.now();
    const reason = input.reason.trim();
    const updated = await tx
      .prepare(
        "UPDATE crm_opportunities SET reservation_id=?,win_source='pms',won_at=?,expected_minor=?,stage='won',version=version+1,updated_at=? WHERE id=? AND version=? AND stage IN ('discovery','proposal','negotiation')",
      )
      .bind(
        input.reservationId,
        now,
        Number(reservation.expected_minor),
        now,
        opportunity.id,
        input.expectedVersion,
      )
      .run();
    if (updated.meta.changes !== 1) failure("CRM_SOURCE_CHANGED", 409);
    const leadUpdated = await tx
      .prepare(
        "UPDATE crm_leads SET version=version+1,updated_at=? WHERE id=? AND company_id=? AND version=?",
      )
      .bind(now, input.leadId, scope.companyId, input.leadVersion)
      .run();
    if (leadUpdated.meta.changes !== 1) failure("CRM_LEAD_STATE_CHANGED", 409);
    const snapshot = JSON.stringify({
      action: "win-pms",
      expectedVersion: input.expectedVersion,
      leadVersion: input.leadVersion,
      reservationId: input.reservationId,
      propertyId: input.propertyId,
      reason,
      stage: "won",
    });
    await tx
      .prepare(
        "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,NULL,?,'opportunity-win-pms',?,?,?,?)",
      )
      .bind(
        leadEvent,
        input.leadId,
        input.leadVersion + 1,
        scope.actorId,
        reason,
        snapshot,
        now,
      )
      .run();
    await tx
      .prepare(
        "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,?,?,'win-pms',?,?,?,?)",
      )
      .bind(
        dealEvent,
        input.leadId,
        opportunity.id,
        input.expectedVersion + 1,
        scope.actorId,
        reason,
        snapshot,
        now,
      )
      .run();
    return {
      id: input.leadId,
      version: input.expectedVersion + 1,
      leadVersion: input.leadVersion + 1,
      stage: "won",
    };
  });
}
