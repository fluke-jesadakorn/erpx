import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { isTransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/**
 * Test-only native-seeding oracle for CRM lead qualification.
 *
 * The TypeScript `crm.lead.qualify` writer is retired: `changeCRMLead` now
 * rejects `qualify` with `CRM_COMMAND_NOT_NATIVE` while the Rust owner
 * (`packages/server-crm/src/adapters/leads_native.rs`) is authoritative.
 * Tests that only need the durable owner rows qualify through this oracle
 * instead of the removed TypeScript effect.
 *
 * It reproduces the exact rows the native command persists in one transaction:
 *  - one `crm_opportunities` row (`opportunity-<commandKey>`, stage discovery,
 *    version 1) using the company currency,
 *  - the `crm_leads` update (status qualified, version+1, updated_at),
 *  - the two immutable `crm_case_events` (deal/version 1 and lead/version n+1)
 *    with the canonical qualify snapshot.
 *
 * Preconditions mirror the native effect: the lead must exist in the company,
 * be owned by the given owner, be `open`, and match `expectedVersion`; the
 * customer must be an active Finance customer whose CRM account has been
 * reviewed and linked. Failures throw with the native error codes so callers
 * still assert real behavior.
 */
export type CrmQualifySeedInput = {
  companyId: string;
  ownerId: string;
  leadId: string;
  customerId: string;
  expectedValue: string;
  closeDate: string;
  reason: string;
  /** Defaults to the company currency in `commerce_settings`. */
  currency?: string;
  /** Defaults to a random key; deterministic ids derive from it. */
  commandKey?: string;
  /** Lead version the caller is qualifying. Defaults to 1. */
  expectedVersion?: number;
  now?: number;
};

export type CrmQualifySeedResult = {
  leadId: string;
  opportunityId: string;
  version: number;
  expectedMinor: number;
  currency: string;
};

function fail(code: string, message: string): never {
  throw Object.assign(new Error(message), { code });
}

function decimalToMinor(value: string): number {
  if (!/^\d{1,12}(\.\d{1,2})?$/.test(value)) fail("CRM_LEAD_INPUT_INVALID", "Use a valid expected value.");
  const [whole, fraction = ""] = value.split(".");
  const minor = BigInt(whole) * 100n + BigInt(fraction.padEnd(2, "0"));
  if (minor > BigInt(Number.MAX_SAFE_INTEGER)) fail("CRM_LEAD_INPUT_INVALID", "Expected value is too large.");
  return Number(minor);
}

export async function seedCrmQualifiedLead(
  database: SqlDatabase,
  input: CrmQualifySeedInput,
): Promise<CrmQualifySeedResult> {
  const expectedVersion = input.expectedVersion ?? 1;
  const commandKey = input.commandKey ?? crypto.randomUUID();
  const now = input.now ?? Date.now();
  const expectedMinor = decimalToMinor(input.expectedValue);

  const run = async (db: SqlDatabase): Promise<CrmQualifySeedResult> => {
    const currency =
      input.currency ??
      (
        await db
          .prepare("SELECT currency FROM commerce_settings WHERE company_id=?")
          .bind(input.companyId)
          .first<string>("currency")
      );
    if (!currency) fail("CRM_CURRENCY_REQUIRED", "The company currency is not configured.");

    const lead = await db
      .prepare("SELECT owner_id,status,version FROM crm_leads WHERE id=? AND company_id=? FOR UPDATE")
      .bind(input.leadId, input.companyId)
      .first<{ owner_id: string; status: string; version: number }>();
    if (!lead) fail("CRM_LEAD_NOT_FOUND", "Lead not found in this scope.");
    if (Number(lead.version) !== expectedVersion)
      fail("CRM_LEAD_VERSION_CHANGED", "The lead changed. Reload and retry.");
    if (lead.owner_id !== input.ownerId)
      fail("CRM_OWNER_REQUIRED", "The assigned salesperson must qualify this lead.");
    if (lead.status !== "open") fail("CRM_STATE_CHANGED", "Review the current lead or opportunity state.");

    const reviewedCustomer = await db
      .prepare(
        `SELECT EXISTS(SELECT 1 FROM crm_leads l
          JOIN crm_accounts ca ON ca.id=l.account_id AND ca.company_id=l.company_id
          JOIN finance_parties p ON p.id=ca.finance_party_id AND p.company_id=l.company_id
          WHERE l.id=? AND l.company_id=? AND p.id=? AND p.kind='customer'
            AND p.status='active' AND ca.reviewed_by IS NOT NULL)`,
      )
      .bind(input.leadId, input.companyId, input.customerId)
      .first<boolean>("exists");
    if (!reviewedCustomer)
      fail("CRM_SOURCE_CHANGED", "The reviewed customer link does not match this lead.");

    const opportunityId = `opportunity-${commandKey}`;
    const leadVersion = expectedVersion + 1;
    const snapshot = JSON.stringify({
      action: "qualify",
      expectedVersion,
      customerId: input.customerId,
      expectedValue: input.expectedValue,
      closeDate: input.closeDate,
      reason: input.reason,
    });

    await db
      .prepare(
        "INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)",
      )
      .bind(
        opportunityId,
        input.leadId,
        input.customerId,
        expectedMinor,
        currency,
        input.closeDate,
        input.ownerId,
        now,
        now,
      )
      .run();

    const updated = await db
      .prepare(
        "UPDATE crm_leads SET status='qualified',version=version+1,updated_at=? WHERE id=? AND company_id=? AND owner_id=? AND status='open' AND version=?",
      )
      .bind(now, input.leadId, input.companyId, input.ownerId, expectedVersion)
      .run();
    if (updated.meta.changes !== 1)
      fail("CRM_SOURCE_CHANGED", "The original CRM source changed.");

    for (const [suffix, opportunity, version] of [
      ["deal", opportunityId, 1],
      ["lead", null, leadVersion],
    ] as const) {
      await db
        .prepare(
          "INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at) VALUES(?,?,?,?,'qualify',?,?,?,?)",
        )
        .bind(
          `crm-event-${commandKey}-${suffix}`,
          input.leadId,
          opportunity,
          version,
          input.ownerId,
          input.reason,
          snapshot,
          now,
        )
        .run();
    }

    return { leadId: input.leadId, opportunityId, version: leadVersion, expectedMinor, currency };
  };

  return isTransactionalSqlDatabase(database) ? database.transaction(run) : run(database);
}
