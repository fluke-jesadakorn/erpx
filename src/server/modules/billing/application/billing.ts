import {z} from 'zod';
import {billingMonth,monthBounds,proratedSupport,type BillingView,type InvoiceLine} from '@/lib/billing';
import {roundSatang} from '@/lib/billing-target-pricing';
import {monthBounds as targetMonthBounds} from '@/lib/billing';
import {stripeList,stripeUrl,testObject,type BillingStripe} from '../domain/stripe';
import {billingInput,type BillingActor,type BillingInput} from '../domain/billing';
import type {BillingReceipt,BillingAccount,BillingEnrollment,BillingPeriod,BillingPlan,BillingInvoiceRow,BillingCost,BillingJobRow,BillingPriceRow,BillingMeterRow,BillingViewFacts} from '../domain/billing';
import type {TargetBillingPeriodView} from '../domain/target';
import type {TargetRolloutReadiness} from '../domain/target-rollout';
export interface BillingRepository {recordRatingRun(value0:string,value1:string,value2:string,value3:string,value4:string,value5:string,value6:string,value7:string,value8:number|null,value9:string,value10:string,value11:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
retainRatedPeriod(value0:string,value1:string,value2:string,value3:string,value4:string,value5:string,value6:number,value7:number,value8:string,value9:string,value10:string,value11:number|null,value12:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
activeEnrollmentForAdvance(value0:string):Promise<{rows:BillingEnrollment[];rowCount:number|null}>;
latestEnrollmentPeriod(value0:string):Promise<{rows:BillingPeriod[];rowCount:number|null}>;
recordScheduledChange(value0:string,value1:string,value2:string,value3:string,value4:string,value5:string,value6:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
endEnrollment(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
activatePendingPlan(value0:string,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
effectivePlan(value0:string,value1:string):Promise<{rows:BillingPlan[];rowCount:number|null}>;
createBillingPeriod(value0:string,value1:string,value2:string,value3:string,value4:number,value5:number,value6:string,value7:number,value8:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
closeElapsedPeriods(value0:string,value1:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
begin():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
lockHolding(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
lockCommand(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
receipt(value0:string):Promise<{rows:BillingReceipt[];rowCount:number|null}>;
commitReplay():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
holdingExists(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
ensureAccount(value0:string,value1:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
lockedAccount(value0:string):Promise<{rows:BillingAccount[];rowCount:number|null}>;
activeEnrollment(value0:string):Promise<{rows:BillingEnrollment[];rowCount:number|null}>;
retainCustomer(value0:string,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
selectedPeriod(value0:string | undefined,value1:string):Promise<{rows:BillingPeriod[];rowCount:number|null}>;
enroll(value0:string,value1:string,value2:string | undefined,value3:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
scheduleEnrollmentChange(value0:string,value1:unknown,value2:unknown,value3:unknown):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
retainConsentedCard(value0:string,value1:string,value2:number,value3:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
automaticOpenInvoices(value0:string):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
queueManualInvoice(value0:string,value1:string,value2:string,value3:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
withdrawConsent(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
setPaymentMode(value0:string,value1:"manual" | "auto" | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
originalInvoiceExists(value0:string | undefined,value1:string,value2:string):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
availableCostEvidence(value0:string | null | undefined,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
existingCost(value0:string,value1:string,value2:string):Promise<{rows:BillingCost[];rowCount:number|null}>;
insertCost(value0:string,value1:string,value2:string,value3:string | undefined,value4:number | undefined,value5:number | undefined,value6:number | undefined,value7:string | undefined,value8:unknown,value9:unknown,value10:unknown,value11:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
markCostsIncomplete(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
deleteUnissuedCost(value0:string | undefined,value1:string):Promise<{rows:BillingCost[];rowCount:number|null}>;
markDeletedCostPeriodIncomplete(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
setCostCompletion(value0:string,value1:unknown):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
currentMainInvoice(value0:string):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
unissuedCosts(value0:string):Promise<{rows:BillingCost[];rowCount:number|null}>;
retainedPlan(value0:string,value1:number):Promise<{rows:BillingPlan[];rowCount:number|null}>;
insertLegacyInvoice(value0:string,value1:string,value2:string,value3:string,value4:number,value5:string,value6:"manual"|"auto",value7:number,value8:string,value9:string|null):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
attachCostToInvoice(value0:string,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
queueLegacyIssue(value0:string,value1:string,value2:string,value3:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
voidableInvoice(value0:string | undefined,value1:string):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
queueVoid(value0:string,value1:string,value2:string,value3:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
lockPlanRevision():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
latestPlanRevision(value0:string | undefined):Promise<{rows:BillingPlan[];rowCount:number|null}>;
insertPlanRevision(value0:string | undefined,value1:number,value2:string,value3:number | undefined,value4:string,value5:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
selectedTargetPrice(value0:string | undefined,value1:string | undefined):Promise<{rows:BillingPriceRow[];rowCount:number|null}>;
currentTargetPrice(value0:string | undefined):Promise<{rows:BillingPriceRow[];rowCount:number|null}>;
sandboxHolding(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
approvedRollout(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
assignTargetPrice(value0:string,value1:string | undefined,value2:string,value3:string | undefined,value4:number,value5:string,value6:unknown,value7:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
successfulPlatformMeter(value0:string,value1:string | undefined,value2:string | undefined):Promise<{rows:BillingMeterRow[];rowCount:number|null}>;
availableProviderEvidence(value0:string | null | undefined,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
approvedProviderPeriod(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
existingProviderCost(value0:string,value1:string | undefined,value2:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordProviderCost(value0:string,value1:string | undefined,value2:string | undefined,value3:string | undefined,value4:string | undefined,value5:string | undefined,value6:number | undefined,value7:number | undefined,value8:number | undefined,value9:number | undefined,value10:string | null | undefined,value11:string,value12:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
availableStatementEvidence(value0:string | null | undefined,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
approvedStatementPeriod(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
providerStatementTotal(value0:string,value1:string | undefined,value2:string | undefined,value3:string | undefined,value4:string | undefined):Promise<{rows:{count:number;total:string}[];rowCount:number|null}>;
recordProviderStatement(value0:string,value1:string | undefined,value2:string | undefined,value3:string | undefined,value4:string | null | undefined,value5:number | undefined,value6:string | undefined,value7:string,value8:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
availableOperatingEvidence(value0:string | null | undefined,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordOperatingCost(value0:string,value1:string | undefined,value2:string | undefined,value3:number | undefined,value4:string | null | undefined,value5:string,value6:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
availableCapacityEvidence(value0:string | null | undefined,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordCapacityReview(value0:string,value1:string | undefined,value2:number | undefined,value3:number | undefined,value4:number | undefined,value5:number | undefined,value6:number | undefined,value7:string | null | undefined,value8:string,value9:unknown,value10:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordRolloutApproval(value0:string,value1:string | undefined,value2:string,value3:string,value4:number | undefined,value5:string,value6:unknown,value7:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
extraTargetAssignment(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
approvedExtraPeriod(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
availableExtraEvidence(value0:string | null | undefined,value1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordTargetExtra(value0:string,value1:string,value2:string | undefined,value3:string | undefined,value4:string | undefined,value5:number | undefined,value6:number | undefined,value7:number | undefined,value8:string | null | undefined,value9:string,value10:unknown,value11:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
lockTargetSnapshot(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
targetRolloutEligible(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
approvedTargetPeriod(value0:string,value1:string | undefined):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
legacyEnrollmentOverlap(value0:string,value1:number,value2:number):Promise<{rows:BillingEnrollment[];rowCount:number|null}>;
monthInvoiceExists(value0:string,value1:string | undefined):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
activeTargetEnrollment(value0:string):Promise<{rows:BillingEnrollment[];rowCount:number|null}>;
enrollTarget(value0:string,value1:string,value2:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
retainedTargetEnrollment(value0:string):Promise<{rows:BillingEnrollment[];rowCount:number|null}>;
targetPeriod(value0:string,value1:string | undefined):Promise<{rows:BillingPeriod[];rowCount:number|null}>;
insertTargetInvoice(value0:string,value1:string,value2:string,value3:number|null,value4:string,value5:"manual"|"auto",value6:number,value7:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
approveTargetRating(value0:string,value1:string | undefined,value2:string,value3:number,value4:unknown,value5:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
queueTargetIssue(value0:string,value1:string,value2:string,value3:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
selectedJob(value0:string | undefined,value1:string):Promise<{rows:BillingJobRow[];rowCount:number|null}>;
retryLegacyEvent(value0:string | undefined,value1:number,value2:string):Promise<{rows:BillingAccount[];rowCount:number|null}>;
retryJob(value0:string,value1:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
incrementAccountVersion(value0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordCommandAudit(value0:string,value1:string,value2:string,value3:unknown,value4:unknown,value5:string,value6:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
recordCommandReceipt(value0:string,value1:string,value2:string,value3:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
commit():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
rollback():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;release():void;calculateTarget(holdingId:string,month:string,now:number):Promise<TargetBillingPeriodView>;calculateRollout(holdingId:string,month:string,margin:number,now:number):Promise<TargetRolloutReadiness>;}
export interface BillingPorts {error(status:number,code:string,message:string):Error;hash(value:string):string;id():string;now():number;session():Promise<BillingRepository>;view(holdingId:string):Promise<BillingViewFacts>;}
export function createBillingService(ports:BillingPorts){function fail(code:string,message:string):never{throw ports.error(409,code,message)}
const adminActions = new Set([
  "cost",
  "delete-cost",
  "complete-costs",
  "approve",
  "void",
  "revise-plan",
  "retry",
  "assign-target",
  "verify-provider-cost",
  "verify-statement",
  "approve-target",
  "rate-target",
  "record-operating-cost",
  "review-target-capacity",
  "approve-target-rollout",
  "approve-target-extra",
]);
async function storeTargetRating(
  c: BillingRepository,
  target: TargetBillingPeriodView,
  actor: BillingActor,
  reason: string,
  now: number,
) {
  if (!target.assignment || !target.price)
    fail("TARGET_ASSIGNMENT_REQUIRED", "Assign a target price before rating this month.");
  const bounds = monthBounds(target.month);
  const state = target.completeness.ready ? "ready" : "held";
  const snapshotId = target.completeness.meterSnapshotId ?? "missing";
  await c.recordRatingRun(ports.id(),target.holdingId,target.month,target.price.id,snapshotId,state,JSON.stringify(target.completeness.missing),JSON.stringify(target.lines),target.totalSatang,actor.userId,reason,now);
  await c.retainRatedPeriod(target.holdingId,target.month,target.price.id,target.assignment.effectiveMonth,target.assignment.mode,snapshotId,Math.max(bounds.start, target.assignment.serviceStartedAt),bounds.end,state,JSON.stringify(target.completeness.missing),JSON.stringify(target.lines),target.totalSatang,now);
}

async function advanceBilling(
  c: BillingRepository,
  holdingId: string,
  now: number,
) {
  const e = (
    await c.activeEnrollmentForAdvance(holdingId)
  ).rows[0];
  if (!e) return;
  const last = (
    await c.latestEnrollmentPeriod(e.id)
  ).rows[0];
  let start = last ? Number(last.end_at) : Number(e.started_at);
  for (let safety = 0; start <= now && safety < 1200; safety++) {
    if (e.effective_at && start >= Number(e.effective_at)) {
      await c.recordScheduledChange(ports.id(),holdingId,"system",e.pending_end ? "hosted-ended" : "plan-effective","Scheduled month-end change applied",JSON.stringify({
          enrollmentId: e.id,
          plan: e.pending_plan,
          effectiveAt: e.effective_at,
          sandbox: true,
        }),now);

      if (e.pending_end) {
        await c.endEnrollment(e.id);
        break;
      }
      if (e.pending_plan) e.plan_id = e.pending_plan;
      await c.activatePendingPlan(e.id,e.plan_id);
      e.effective_at = null;
    }
    const month = billingMonth(start),
      bounds = monthBounds(month);
    const plan = (
      await c.effectivePlan(e.plan_id,month)
    ).rows[0];
    if (!plan) throw new Error("BILLING_PLAN_MISSING");
    await c.createBillingPeriod(ports.id(),holdingId,e.id,month,start,bounds.end,plan.id,plan.revision,proratedSupport(Number(plan.support_satang), start, month));
    start = bounds.end;
  }
  await c.closeElapsedPeriods(holdingId,now);
}

async function billingCommand(
  actor: BillingActor,
  holdingId: string,
  input: BillingInput,
  key: string,
  stripe: BillingStripe,
  returnUrl: string,
  now = ports.now(),
) {
  input = billingInput.parse(input);
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
    throw ports.error(
      422,
      "COMMAND_KEY_REQUIRED",
      "Idempotency-Key required.",
    );
  if (
    actor.platform
      ? !adminActions.has(input.action)
      : adminActions.has(input.action) || actor.holdingId !== holdingId
  )
    throw ports.error(
      403,
      "FORBIDDEN",
      "This billing action is unavailable for this role.",
    );
  const digest = ports.hash(JSON.stringify({ actor: actor.userId, holdingId, input }));
  const c = await ports.session();
  try {
    await c.begin();
    await c.lockHolding(`billing:${holdingId}`);
    await c.lockCommand(`billing-command:${key}`);
    const receipt = (
      await c.receipt(key)
    ).rows[0];
    if (receipt) {
      if (receipt.digest !== digest)
        fail("KEY_CONFLICT", "This key belongs to another request.");
      await c.commitReplay();
      return JSON.parse(receipt.result_json) as { ok: boolean; url?: string };
    }
    const exists = (
      await c.holdingExists(holdingId)
    ).rowCount;
    if (!exists) throw ports.error(404, "NOT_FOUND", "Holding not found.");
    await c.ensureAccount(holdingId,now);
    const a = (
      await c.lockedAccount(holdingId)
    ).rows[0];
    if (Number(a.version) !== input.version)
      fail("VERSION_CONFLICT", "Billing changed. Refresh and review again.");
    await advanceBilling(c, holdingId, now);
    const e = (
      await c.activeEnrollment(holdingId)
    ).rows[0];
    const result: { ok: boolean; url?: string } = { ok: true };
    const customer = async () => {
      if (!a.customer_id) {
        const s = testObject(
          await stripe(
            "customers",
            { email: actor.email, "metadata[holding_id]": holdingId },
            `erpx-billing-customer-${holdingId}`,
          ),
        );
        a.customer_id = s.id;
        await c.retainCustomer(holdingId,s.id);
      }
      return a.customer_id as string;
    };
    const period = async () => {
      const p = (
        await c.selectedPeriod(input.periodId,holdingId)
      ).rows[0];
      if (!p) fail("PERIOD_REQUIRED", "Choose a billing period.");
      return p;
    };
    switch (input.action) {
      case "enroll": {
        if (e) fail("ALREADY_ENROLLED", "A Hosted enrollment already exists.");
        if (!input.planId) fail("PLAN_REQUIRED", "Choose a Support plan.");
        const subscriptionList = await stripe(
          `subscriptions?customer=${encodeURIComponent(await customer())}&status=all&limit=100`,
        );
        if (subscriptionList.has_more)
          fail(
            "LEGACY_REVIEW_REQUIRED",
            "Review the full legacy Subscription history in Stripe before enrolling.",
          );
        const subscriptions = stripeList(subscriptionList);
        if (
          subscriptions.some(
            (s) =>
              !["canceled", "incomplete_expired"].includes(String(s.status)),
          )
        )
          fail(
            "LEGACY_ACTIVE",
            "Cancel the legacy test Subscription through Stripe before enrolling.",
          );
        await c.enroll(ports.id(),holdingId,input.planId,now);
        await advanceBilling(c, holdingId, now);
        break;
      }
      case "change-plan":
      case "cancel-hosted":
      case "withdraw": {
        if (!e) fail("NOT_ENROLLED", "No active Hosted service.");
        if (input.action === "change-plan" && !input.planId)
          fail("PLAN_REQUIRED", "Choose a plan.");
        await c.scheduleEnrollmentChange(e.id,input.action === "change-plan" ? input.planId : null,input.action === "cancel-hosted",input.action === "withdraw"
              ? null
              : monthBounds(billingMonth(now)).end);
        break;
      }
      case "portal":
      case "resolve-legacy":
      case "setup": {
        const id = await customer();
        const setup = input.action === "setup";
        const s = await stripe(
          setup ? "checkout/sessions" : "billing_portal/sessions",
          setup
            ? {
                customer: id,
                mode: "setup",
                currency: "thb",
                "payment_method_types[0]": "card",
                success_url: returnUrl,
                cancel_url: returnUrl,
                "setup_intent_data[metadata][holding_id]": holdingId,
              }
            : { customer: id, return_url: returnUrl },
          `erpx-billing-link-${key}`,
        );
        result.url =
          stripeUrl(s.url, ["checkout.stripe.com", "billing.stripe.com"]) ??
          undefined;
        break;
      }
      case "payment-mode": {
        if (!input.paymentMode)
          fail("MODE_REQUIRED", "Choose manual or auto payment.");
        if (input.paymentMode === "auto") {
          if (input.consent !== true)
            fail("CONSENT_REQUIRED", "Explicit Auto-pay consent is required.");
          const card = stripeList(
            await stripe(
              `payment_methods?customer=${encodeURIComponent(await customer())}&type=card&limit=1`,
            ),
          )[0];
          if (!card)
            fail("CARD_REQUIRED", "Save a test card with Stripe first.");
          testObject(card);
          await stripe(
            `customers/${a.customer_id}`,
            { "invoice_settings[default_payment_method]": card.id },
            `erpx-billing-card-${key}`,
          );
          const details = z
            .object({
              brand: z.string(),
              last4: z.string(),
              exp_month: z.number(),
              exp_year: z.number(),
            })
            .parse(card.card);
          await c.retainConsentedCard(holdingId,JSON.stringify({ id: card.id, ...details }),now,actor.userId);
        } else {
          // Pause each open automatic invoice before acknowledging withdrawal. Stable keys survive retries.
          const open = (
            await c.automaticOpenInvoices(holdingId)
          ).rows;
          for (const inv of open) {
            await stripe(
              `invoices/${inv.stripe_id}`,
              { auto_advance: "false" },
              `erpx-stop-auto-${key}-${inv.id}`,
            );
            await c.queueManualInvoice(`manual-${inv.id}-${key}`,holdingId,inv.id,now);
          }
          await c.withdrawConsent(holdingId);
        }
        await c.setPaymentMode(holdingId,input.paymentMode);
        break;
      }
      case "cost": {
        const p = await period();
        if (
          input.amountSatang === undefined ||
          !input.category ||
          !input.note ||
          input.serviceStart === undefined ||
          input.serviceEnd === undefined
        )
          fail(
            "COST_REQUIRED",
            "Amount, category, service dates and notes are required.",
          );
        if (input.category !== "adjustment" && input.amountSatang < 0)
          fail("INVALID_COST", "Only adjustments may be negative.");
        if (
          input.serviceStart < Number(p.start_at) ||
          input.serviceEnd > Number(p.end_at) ||
          input.serviceEnd <= input.serviceStart
        )
          fail(
            "INVALID_SERVICE_DATES",
            "Service dates must be inside the original service period.",
          );
        if (input.category === "adjustment" && !input.originalInvoiceId)
          fail(
            "ORIGINAL_REQUIRED",
            "An adjustment must reference the original invoice.",
          );
        if (
          input.originalInvoiceId &&
          !(
            await c.originalInvoiceExists(input.originalInvoiceId,holdingId,p.id)
          ).rowCount
        )
          fail(
            "INVALID_ORIGINAL",
            "Original invoice must belong to this Holding and period.",
          );
        if (
          input.evidenceId &&
          !(
            await c.availableCostEvidence(input.evidenceId,holdingId)
          ).rowCount
        )
          fail("INVALID_EVIDENCE", "Evidence not found.");
        if (!input.simulated && !input.evidenceId)
          fail(
            "EVIDENCE_REQUIRED",
            "Attach private evidence for an actual provider cost, or explicitly label a simulated fixture.",
          );
        const id = input.costId ?? ports.id();
        if (input.costId) {
          const old = (
            await c.existingCost(id,holdingId,p.id)
          ).rows[0];
          if (!old || old.invoice_id)
            fail("COST_FROZEN", "Issued or missing costs cannot be changed.");
        }
        await c.insertCost(id,holdingId,p.id,input.category,input.amountSatang,input.serviceStart,input.serviceEnd,input.note,input.simulated ?? false,input.evidenceId ?? null,input.originalInvoiceId ?? null,now);
        await c.markCostsIncomplete(p.id);
        break;
      }
      case "delete-cost": {
        const old = (
          await c.deleteUnissuedCost(input.costId,holdingId)
        ).rows[0];
        if (!old)
          fail("COST_FROZEN", "Issued or missing costs cannot be removed.");
        await c.markDeletedCostPeriodIncomplete(old.period_id);
        break;
      }
      case "complete-costs": {
        const p = await period();
        await c.setCostCompletion(p.id,input.complete === true);
        break;
      }
      case "approve": {
        const p = await period();
        if (!p.closed_at)
          fail(
            "MONTH_NOT_CLOSED",
            "Approval is only available after month end.",
          );
        if (!p.costs_complete)
          fail(
            "COSTS_INCOMPLETE",
            "Review and confirm complete costs, including an explicit zero-cost month.",
          );
        const previous = (
          await c.currentMainInvoice(p.id)
        ).rows[0];
        if (previous && ["queued", "draft"].includes(previous.status))
          fail("INVOICE_PENDING", "Wait for the existing invoice to finish.");
        const costs = (
          await c.unissuedCosts(p.id)
        ).rows;
        if (previous && !costs.length)
          fail("ALREADY_ISSUED", "No new reviewed costs to invoice.");
        const plan = (
          await c.retainedPlan(p.plan_id,p.plan_revision)
        ).rows[0];
        const lines: InvoiceLine[] = costs.map((v) => ({
          description: `${v.simulated ? "[Simulated] " : ""}${v.category}: ${v.note}`,
          amount: Number(v.amount_satang),
          start: Number(v.service_start),
          end: Number(v.service_end),
          costId: v.id,
          simulated: v.simulated,
        }));
        if (!previous)
          lines.unshift({
            description: `Sandbox ${plan.name} · revision ${plan.revision}`,
            amount: Number(p.support_satang),
            start: Number(p.start_at),
            end: Number(p.end_at),
          });
        const total = lines.reduce((s, v) => s + v.amount, 0);
        if (lines.length > 250)
          fail(
            "TOO_MANY_LINES",
            "Consolidate the reviewed draft to at most 250 lines before approval.",
          );
        if (total < 0)
          fail(
            "CREDIT_REVIEW_REQUIRED",
            "A net credit requires a separate reviewed Stripe credit note; automatic refunds are not enabled.",
          );
        if (a.payment_mode === "auto" && (!a.consent_at || !a.card_json))
          fail("CONSENT_REQUIRED", "Auto-pay needs a saved card and consent.");
        await customer();
        const id = ports.id();
        await c.insertLegacyInvoice(id,holdingId,p.id,previous ? "adjustment" : "main",total,JSON.stringify(lines),a.payment_mode,now,actor.userId,previous?.id ?? null);
        for (const cost of costs)
          await c.attachCostToInvoice(cost.id,id);
        await c.queueLegacyIssue(`issue-${id}`,holdingId,id,now);
        break;
      }
      case "void": {
        const inv = (
          await c.voidableInvoice(input.invoiceId,holdingId)
        ).rows[0];
        if (!inv)
          fail(
            "VOID_UNAVAILABLE",
            "Only unpaid issued invoices may be voided.",
          );
        await c.queueVoid(`void-${inv.id}`,holdingId,inv.id,now);
        break;
      }
      case "revise-plan": {
        if (input.planId === "infrastructure" && input.supportSatang !== 0)
          fail(
            "INFRASTRUCTURE_SUPPORT_ZERO",
            "Infrastructure Only must have zero Support.",
          );
        if (!input.planId || input.supportSatang === undefined)
          fail("PLAN_REQUIRED", "Choose the plan and new Support price.");
        await c.lockPlanRevision();
        const old = (
          await c.latestPlanRevision(input.planId)
        ).rows[0];
        if (Number(old.revision) !== input.planRevision)
          fail(
            "PLAN_VERSION_CONFLICT",
            "The plan revision changed. Refresh and review the latest price.",
          );
        await c.insertPlanRevision(input.planId,Number(old.revision) + 1,old.name,input.supportSatang,billingMonth(monthBounds(billingMonth(now)).end),now);
        break;
      }
      case "assign-target": {
        if (!input.targetMonth || !input.targetMode)
          fail("TARGET_ASSIGNMENT_REQUIRED", "Choose a target month and mode.");
        const targetBounds = targetMonthBounds(input.targetMonth);
        const price = input.priceId
          ? (await c.selectedTargetPrice(input.priceId,input.targetMonth)).rows[0]
          : (await c.currentTargetPrice(input.targetMonth)).rows[0];
        if (!price) fail("TARGET_PRICE_MISSING", "No approved Sandbox price version applies to this month.");
        if (input.targetMode === "sandbox_invoice") {
          if (input.targetMonth < billingMonth(now))
            fail("TARGET_BACKDATE_FORBIDDEN", "Sandbox invoice assignments cannot be backdated.");
          const designated = (await c.sandboxHolding(holdingId)).rowCount;
          const rollout = input.targetMonth > billingMonth(now) && (await c.approvedRollout(holdingId,input.targetMonth)).rowCount;
          if (!designated && !rollout)
            fail("TARGET_ROLLOUT_NOT_READY", "This Holding needs explicit Sandbox test designation or approved future-month rollout evidence.");
        }
        const serviceStartedAt = input.targetMonth < billingMonth(now)
          ? targetBounds.start : Math.max(now, targetBounds.start);
        const inserted = await c.assignTargetPrice(holdingId,input.targetMonth,price.id,input.targetMode,serviceStartedAt,actor.userId,input.reason,now);
        if (!inserted.rowCount) fail("TARGET_ASSIGNMENT_EXISTS", "This month's assignment is immutable. Choose a later month.");
        break;
      }
      case "verify-provider-cost": {
        if (!input.targetMonth || !input.sourceKey || !input.provider || !input.statementId ||
            !input.originalCurrency || input.originalMinor === undefined ||
            input.fxNumerator === undefined || input.fxDenominator === undefined ||
            input.providerCostSatang === undefined || !input.evidenceId)
          fail("PROVIDER_COST_REQUIRED", "Source, provider, statement, currency, FX, amount and evidence are required.");
        const meter = (await c.successfulPlatformMeter(holdingId,input.targetMonth,input.sourceKey)).rows[0];
        if (!meter || meter.provider !== input.provider ||
            meter.cost_status !== "reconciled" || meter.cost_reconciled_at === null ||
            meter.verified_cost_satang === null ||
            Number(meter.verified_cost_satang) !== input.providerCostSatang)
          fail("PROVIDER_COST_UNVERIFIED", "Match a successful Platform-key meter to verified provider cost first.");
        const expectedSatang = roundSatang(
          BigInt(input.originalMinor) * BigInt(input.fxNumerator),
          BigInt(input.fxDenominator),
        );
        if (expectedSatang !== input.providerCostSatang)
          fail("FX_MISMATCH", "The reviewed THB amount does not match original units and the FX ratio.");
        const evidence = (await c.availableProviderEvidence(input.evidenceId,holdingId)).rowCount;
        if (!evidence) fail("EVIDENCE_REQUIRED", "Attach available private cost evidence.");
        const prior = (await c.approvedProviderPeriod(holdingId,input.targetMonth)).rowCount;
        if (prior) fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be changed.");
        const existing = (await c.existingProviderCost(holdingId,input.targetMonth,input.sourceKey)).rows[0];
        if (existing) fail("PROVIDER_COST_ALREADY_VERIFIED", "The source already has verified cost; preserve its audit trail.");
        await c.recordProviderCost(holdingId,input.targetMonth,input.sourceKey,input.provider,input.statementId,input.originalCurrency,input.originalMinor,input.fxNumerator,input.fxDenominator,input.providerCostSatang,input.evidenceId,actor.userId,now);
        break;
      }
      case "verify-statement": {
        if (!input.targetMonth || !input.provider || !input.statementId ||
            !input.originalCurrency || input.providerTotalMinor === undefined || !input.evidenceId)
          fail("STATEMENT_REQUIRED", "Month, provider, statement, currency, total and evidence are required.");
        const evidence = (await c.availableStatementEvidence(input.evidenceId,holdingId)).rowCount;
        if (!evidence) fail("EVIDENCE_REQUIRED", "Attach an available provider statement.");
        const prior = (await c.approvedStatementPeriod(holdingId,input.targetMonth)).rowCount;
        if (prior) fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be changed.");
        const count = (await c.providerStatementTotal(holdingId,input.targetMonth,input.provider,input.statementId,input.originalCurrency)).rows[0];
        if (!count.count || BigInt(count.total) !== BigInt(input.providerTotalMinor))
          fail("STATEMENT_MISMATCH", "Statement total must equal the matched provider source costs.");
        const inserted = await c.recordProviderStatement(holdingId,input.targetMonth,input.provider,input.statementId,input.evidenceId,input.providerTotalMinor,input.originalCurrency,actor.userId,now);
        if (!inserted.rowCount) fail("STATEMENT_ALREADY_VERIFIED", "The provider statement was already verified.");
        break;
      }
      case "record-operating-cost": {
        if (!input.targetMonth || !input.operatingCategory ||
            input.operatingCostSatang === undefined || !input.evidenceId)
          fail("OPERATING_COST_REQUIRED", "Month, category, actual cost and evidence are required.");
        if (now < monthBounds(input.targetMonth).end)
          fail("MONTH_NOT_CLOSED", "Record actual operating costs only after the month ends.");
        if (!(await c.availableOperatingEvidence(input.evidenceId,holdingId)).rowCount) fail("EVIDENCE_REQUIRED", "Attach available operating cost evidence.");
        const inserted = await c.recordOperatingCost(holdingId,input.targetMonth,input.operatingCategory,input.operatingCostSatang,input.evidenceId,actor.userId,now);
        if (!inserted.rowCount) fail("OPERATING_COST_ALREADY_VERIFIED", "The actual cost is immutable; add a future adjustment with new evidence.");
        break;
      }
      case "review-target-capacity": {
        if (!input.targetMonth || input.projectedOriginalBytes === undefined ||
            input.primaryAvailableBytes === undefined || input.backupAvailableBytes === undefined ||
            input.reservedBytes === undefined || input.companyLimitBytes === undefined ||
            !input.evidenceId)
          fail("CAPACITY_REVIEW_REQUIRED", "Month, primary and backup capacity, Company cap and evidence are required.");
        if (input.targetMonth <= billingMonth(now))
          fail("FUTURE_MONTH_REQUIRED", "Capacity approval must precede a future effective month.");
        if (!(await c.availableCapacityEvidence(input.evidenceId,holdingId)).rowCount) fail("EVIDENCE_REQUIRED", "Attach available capacity evidence.");
        const inserted = await c.recordCapacityReview(holdingId,input.targetMonth,input.projectedOriginalBytes,input.primaryAvailableBytes,input.backupAvailableBytes,input.reservedBytes,input.companyLimitBytes,input.evidenceId,actor.userId,input.reason,now);
        if (!inserted.rowCount) fail("CAPACITY_ALREADY_REVIEWED", "Capacity evidence for this effective month is immutable.");
        break;
      }
      case "approve-target-rollout": {
        if (!input.targetMonth || input.minimumMarginBps === undefined)
          fail("ROLLOUT_REVIEW_REQUIRED", "Future month and required gross-margin threshold are required.");
        if (input.targetMonth <= billingMonth(now))
          fail("FUTURE_MONTH_REQUIRED", "An existing customer may migrate only in a future month.");
        const review = await c.calculateRollout( holdingId, input.targetMonth, input.minimumMarginBps, now,
        );
        if (!review.ready)
          fail("ROLLOUT_EVIDENCE_INCOMPLETE", `Migration remains held: ${review.blockers.join(", ")}.`);
        const inserted = await c.recordRolloutApproval(holdingId,input.targetMonth,review.shadowMonths[0],review.shadowMonths[1],input.minimumMarginBps,actor.userId,input.reason,now);
        if (!inserted.rowCount) fail("ROLLOUT_ALREADY_APPROVED", "This rollout decision is already recorded.");
        break;
      }
      case "approve-target-extra": {
        if (!input.targetMonth || !input.extraKind || input.amountSatang === undefined ||
            input.serviceStart === undefined || input.serviceEnd === undefined ||
            !input.note || !input.evidenceId)
          fail("EXTRA_APPROVAL_REQUIRED", "Service kind, amount, dates, description and approved quote evidence are required.");
        const bounds = monthBounds(input.targetMonth);
        if (input.serviceStart < bounds.start || input.serviceEnd > bounds.end ||
            input.serviceEnd <= input.serviceStart)
          fail("EXTRA_SERVICE_DATES", "Extra service dates must fall inside the chosen month.");
        if (!(await c.extraTargetAssignment(holdingId,input.targetMonth)).rowCount) fail("TARGET_ASSIGNMENT_REQUIRED", "Assign the target offering before approving extra services.");
        if ((await c.approvedExtraPeriod(holdingId,input.targetMonth)).rowCount) fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be changed.");
        if (!(await c.availableExtraEvidence(input.evidenceId,holdingId)).rowCount) fail("EVIDENCE_REQUIRED", "Attach the separately approved quote as available private evidence.");
        await c.recordTargetExtra(ports.id(),holdingId,input.targetMonth,input.extraKind,input.note,input.amountSatang,input.serviceStart,input.serviceEnd,input.evidenceId,actor.userId,input.reason,now);
        break;
      }
      case "rate-target": {
        if (!input.targetMonth) fail("TARGET_MONTH_REQUIRED", "Choose a target billing month.");
        const target = await c.calculateTarget( holdingId, input.targetMonth, now);
        if (target.state === "approved")
          fail("TARGET_PERIOD_FROZEN", "Approved target charges cannot be recalculated.");
        await storeTargetRating(c, target, actor, input.reason, now);
        break;
      }
      case "approve-target": {
        if (!input.targetMonth) fail("TARGET_MONTH_REQUIRED", "Choose a target billing month.");
        // Projection refresh locks this snapshot row before replacing source
        // meters and seats. Hold the same row through invoice freeze so a
        // concurrent refresh cannot change the reviewed billing inputs.
        await c.lockTargetSnapshot(holdingId,input.targetMonth);
        const target = await c.calculateTarget( holdingId, input.targetMonth, now);
        if (target.assignment?.mode !== "sandbox_invoice")
          fail("TARGET_SHADOW_ONLY", "This Holding is configured for shadow rating only.");
        if (!(await c.targetRolloutEligible(holdingId,input.targetMonth)).rowCount)
          fail("TARGET_ROLLOUT_NOT_READY", "This Holding needs Sandbox test designation or approved rollout evidence.");
        if (!target.completeness.ready || target.totalSatang === null)
          fail("TARGET_PERIOD_INCOMPLETE", `Hold the entire invoice: ${target.completeness.missing.join(", ")}.`);
        if (target.invoice || (await c.approvedTargetPeriod(holdingId,input.targetMonth)).rowCount) fail("ALREADY_ISSUED", "The target period is already approved.");
        await storeTargetRating(c, target, actor, input.reason, now);
        const bounds = targetMonthBounds(input.targetMonth);
        await advanceBilling(c, holdingId, now);
        const legacy = (await c.legacyEnrollmentOverlap(holdingId,bounds.start,bounds.end)).rowCount;
        if (legacy) fail("LEGACY_ENROLLMENT_ACTIVE", "Close the grandfathered Support enrollment before switching a month to target billing.");
        const oldInvoices = (await c.monthInvoiceExists(holdingId,input.targetMonth)).rowCount;
        if (oldInvoices) fail("MONTH_ALREADY_INVOICED", "This Holding already has an invoice for the month.");
        let enrollment = (await c.activeTargetEnrollment(holdingId)).rows[0];
        if (!enrollment) {
          await c.enrollTarget(ports.id(),holdingId,Math.max(bounds.start, target.assignment.serviceStartedAt));
          await advanceBilling(c, holdingId, now);
          enrollment = (await c.retainedTargetEnrollment(holdingId)).rows[0];
        }
        const p = (await c.targetPeriod(enrollment.id,input.targetMonth)).rows[0];
        if (!p || !p.closed_at) fail("TARGET_PERIOD_MISSING", "The target service period is not closed.");
        if (a.payment_mode === "auto" && (!a.consent_at || !a.card_json))
          fail("CONSENT_REQUIRED", "Auto-pay needs a saved card and consent.");
        await customer();
        const lines = target.lines;
        const aiAmount = lines.filter(line => line.kind === "ai")
          .reduce((sum, line) => sum + (line.amountSatang ?? 0), 0);
        const extraAmount = lines.filter(line => line.kind === "extra")
          .reduce((sum, line) => sum + (line.amountSatang ?? 0), 0);
        const serviceStart = Math.max(bounds.start, target.assignment.serviceStartedAt);
        const invoiceLines: InvoiceLine[] = [
          { description: `Sandbox Holding platform fee · ${input.targetMonth}`,
            amount: lines.find(line => line.kind === "base")?.amountSatang ?? 0,
            start: serviceStart, end: bounds.end },
          { description: "Active named seats · prorated seat-months",
            amount: lines.find(line => line.kind === "seat" && line.sourceKey === null)?.amountSatang ?? 0,
            start: serviceStart, end: bounds.end },
          { description: "Original file storage over included allowance",
            amount: lines.find(line => line.kind === "storage" && line.sourceKey === null)?.amountSatang ?? 0,
            start: serviceStart, end: bounds.end },
          { description: "Verified Platform-key AI usage plus 10%",
            amount: aiAmount, start: serviceStart, end: bounds.end },
          { description: "Separately approved setup and Support services",
            amount: extraAmount, start: serviceStart, end: bounds.end },
        ].filter(line => line.amount !== 0);
        if (!invoiceLines.length) invoiceLines.push({ description: "Sandbox Holding service", amount: 0,
          start: serviceStart, end: bounds.end });
        const invoiceId = ports.id();
        await c.insertTargetInvoice(invoiceId,holdingId,p.id,target.totalSatang,JSON.stringify(invoiceLines),a.payment_mode,now,actor.userId);
        await c.approveTargetRating(holdingId,input.targetMonth,actor.userId,now,input.reason,invoiceId);
        await c.queueTargetIssue(`issue-${invoiceId}`,holdingId,invoiceId,now);
        break;
      }
      case "retry": {
        const job = (
          await c.selectedJob(input.jobId,holdingId)
        ).rows[0];
        if (!job) {
          const event = await c.retryLegacyEvent(input.jobId,now,holdingId);
          if (!event.rowCount) fail("JOB_MISSING", "Job not found.");
          break;
        }
        if (job.last_error === "IDEMPOTENCY_WINDOW_EXPIRED")
          fail(
            "MANUAL_RECONCILIATION_REQUIRED",
            "Reconcile the ambiguous Stripe operation before retrying. Do not create another invoice.",
          );
        await c.retryJob(job.id,now);
        break;
      }
    }
    await c.incrementAccountVersion(holdingId);
    await c.recordCommandAudit(ports.id(),holdingId,actor.userId,input.action,input.reason,JSON.stringify(input),now);
    await c.recordCommandReceipt(key,digest,JSON.stringify(result),now);
    await c.commit();
    return result;
  } catch (error) {
    await c.rollback();
    throw error;
  } finally {
    c.release();
  }
}
async function billingView(holdingId:string,now=ports.now()):Promise<BillingView>{return {sandbox:true,now,...await ports.view(holdingId)}}
return {advanceBilling,billingView,billingCommand};}
