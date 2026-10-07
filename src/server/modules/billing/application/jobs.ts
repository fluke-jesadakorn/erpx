import type {InvoiceLine} from '@/lib/billing';
import {stripeUrl,stripeList,testObject,type BillingStripe,type StripeObject} from '../domain/stripe';
import type {BillingAccount,BillingInvoiceRow,BillingJobRow} from '../domain/billing';
import type {LegacyStripeEvent} from '../domain/beta-billing';
export type BillingInboxRow={id:string;payload_json:string};
export interface BillingJobRepository{invoice(v0:string):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
mirrorInvoiceState(v0:string,v1:unknown,v2:string|null,v3:string|null,v4:number|null,v5:unknown,v6:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
releaseVoidedCosts(v0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
markVoidedPeriodIncomplete(v0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
retainTestFinanceProjection(v0:string,v1:string,v2:string,v3:string,v4:number,v5:number,v6:number,v7:number,v8:number,v9:string,v10:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
jobInvoice(v0:string):Promise<{rows:BillingInvoiceRow[];rowCount:number|null}>;
jobAccount(v0:string):Promise<{rows:BillingAccount[];rowCount:number|null}>;
retainJobProgress(v0:string,v1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
retainStripeInvoice(v0:string,v1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
retainInvoiceMode(v0:string,v1:"manual"|"auto"):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
completeJob(v0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
tryHoldingLock(v0:string):Promise<{rows:{locked:boolean}[];rowCount:number|null}>;
begin():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
commit():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
currentAccount(v0:string):Promise<{rows:BillingAccount[];rowCount:number|null}>;
retainCurrentCard(v0:string,v1:string|null):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
pendingJobs(v0:string,v1:number):Promise<{rows:BillingJobRow[];rowCount:number|null}>;
rescheduleJob(v0:string,v1:string,v2:number,v3:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
pendingEvents(v0:number,v1:string):Promise<{rows:BillingInboxRow[];rowCount:number|null}>;
eventInvoices(v0:string,v1:unknown):Promise<{rows:{id:string}[];rowCount:number|null}>;
setupCustomer(v0:string):Promise<{rows:{customer_id:string|null}[];rowCount:number|null}>;
retainSetupState(v0:string,v1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
verifiedSetupCustomer(v0:string):Promise<{rows:{customer_id:string|null}[];rowCount:number|null}>;
retainSetupCard(v0:string,v1:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
applyEvent(v0:string,v1:number):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
rescheduleEvent(v0:string,v1:number,v2:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
staleInvoices(v0:string,v1:number):Promise<{rows:{id:string}[];rowCount:number|null}>;
paidTargetInvoices(v0:string,v1:number):Promise<{rows:{id:string}[];rowCount:number|null}>;
rollback():Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;
unlockHolding(v0:string):Promise<{rows:Record<string,unknown>[];rowCount:number|null}>;release():void;advance(holdingId:string,now:number):Promise<void>;}
export interface BillingJobsPorts{accountHoldings():Promise<{rows:{holding_id:string}[];rowCount:number|null}>;
legacyEvents(v0:number):Promise<{rows:BillingInboxRow[];rowCount:number|null}>;now():number;stripe:BillingStripe;promptpay:boolean;session():Promise<BillingJobRepository>;errorCode(error:unknown):string;financeReady():Promise<{ready:boolean}|null>;syncFinance(local:BillingInvoiceRow & {stripe_id:string},stripeInvoice:StripeObject,notes:StripeObject[],now:number):Promise<{companyId:string;invoiceSatang:number;receivableSatang:number;receivedSatang:number;reversedSatang:number;creditedSatang:number;stripeStatus:string}>;applyLegacyEvent(event:LegacyStripeEvent):Promise<unknown>;retryLegacy(id:string,nextAt:number,code:string):Promise<unknown>;}
export function createBillingJobs(ports:BillingJobsPorts){
function invoicePaymentMethods(mode: "manual" | "auto", promptpay: boolean) {
  if (!promptpay) return {};
  return {
    "payment_settings[payment_method_types][0]": "card",
    ...(mode === "manual"
      ? { "payment_settings[payment_method_types][1]": "promptpay" }
      : {}),
  };
}

async function mirrorInvoice(
  c: BillingJobRepository,
  invoiceId: string,
  stripe: BillingStripe,
  now: number,
  projectionEnabled=false,
) {
  const local = (
    await c.invoice(invoiceId)
  ).rows[0];
  if (!local?.stripe_id) return;
  const s = testObject(
    await stripe(`invoices/${local.stripe_id}?expand[]=payment_intent`),
  );
  if (
    !["draft", "open", "paid", "void", "uncollectible"].includes(
      String(s.status),
    )
  )
    throw new Error("INVALID_INVOICE_STATUS");
  const pi = s.payment_intent as StripeObject | null;
  // Current Stripe retrieval is serialized per Holding, so delayed webhooks never replay their old payload.
  await c.mirrorInvoiceState(invoiceId,s.status,stripeUrl(s.hosted_invoice_url, ["invoice.stripe.com"]),stripeUrl(s.invoice_pdf, ["pay.stripe.com", "invoice.stripe.com"]),typeof s.due_date === "number" ? s.due_date * 1000 : null,pi?.status ?? null,now);
  if (s.status === "void" && local.status !== "void") {
    await c.releaseVoidedCosts(invoiceId);
    await c.markVoidedPeriodIncomplete(local.period_id);
  }
  if (local.rating_model === "target" && projectionEnabled) {
    const list = await stripe(`credit_notes?invoice=${encodeURIComponent(local.stripe_id)}&limit=100`);
    if (list.has_more === true) throw new Error("CREDIT_NOTE_PAGINATION_REQUIRED");
    const ledger = await ports.syncFinance(local as BillingInvoiceRow & {stripe_id:string}, s,
      stripeList(list).map(testObject), now);
    await c.retainTestFinanceProjection(local.id,local.stripe_id,local.holding_id,ledger.companyId,ledger.invoiceSatang,ledger.receivableSatang,ledger.receivedSatang,ledger.reversedSatang,ledger.creditedSatang,ledger.stripeStatus,now);
  }
}

async function processJob(
  c: BillingJobRepository,
  job: BillingJobRow,
  stripe: BillingStripe,
  now: number,
  promptpay: boolean,
  projectionEnabled=false,
) {
  const invoice = (
    await c.jobInvoice(job.invoice_id)
  ).rows[0];
  const a = (
    await c.jobAccount(job.holding_id)
  ).rows[0];
  const progress = JSON.parse(String(job.progress_json)) as {
    created?: boolean;
    items?: number;
    finalized?: boolean;
    sent?: boolean;
    operationAt?: number;
  };
  const save = async () => {
    await c.retainJobProgress(job.id,JSON.stringify(progress));
  };
  if (job.kind === "issue") {
    if (invoice.rating_model === "target" && projectionEnabled) {
      const ready=await ports.financeReady();
      if (!ready?.ready) throw new Error("TEST_FINANCE_LEDGER_UNAVAILABLE");
    }
    const mode =
      a.payment_mode === "auto" && a.consent_at && a.card_json
        ? "auto"
        : "manual";
    if (!invoice.stripe_id) {
      // Stripe retains keys at least 24h. Stop ambiguous creation before that window expires.
      if (progress.operationAt && now - progress.operationAt > 23 * 3600000)
        throw new Error("IDEMPOTENCY_WINDOW_EXPIRED");
      if (!progress.operationAt) {
        progress.operationAt = now;
        await save();
      }
      const s = testObject(
        await stripe(
          "invoices",
          {
            customer: a.customer_id!,
            auto_advance: "false",
            collection_method:
              invoice.payment_mode === "auto"
                ? "charge_automatically"
                : "send_invoice",
            ...(invoice.payment_mode === "manual"
              ? { days_until_due: "7" }
              : {}),
            ...invoicePaymentMethods(invoice.payment_mode, promptpay),
            "metadata[erpx_invoice_id]": invoice.id,
            "metadata[holding_id]": a.holding_id,
            description: invoice.rating_model === "target"
              ? "ERPX Sandbox — verified monthly platform and usage charges"
              : "ERPX Sandbox — reviewed actual costs + Support",
            pending_invoice_items_behavior: "exclude",
          },
          `erpx-invoice-${invoice.id}`,
        ),
      );
      invoice.stripe_id = s.id;
      await c.retainStripeInvoice(invoice.id,s.id);
    }
    const lines = JSON.parse(invoice.lines_json) as InvoiceLine[];
    for (let i = progress.items ?? 0; i < lines.length; i++) {
      if (progress.operationAt && now - progress.operationAt > 23 * 3600000)
        throw new Error("IDEMPOTENCY_WINDOW_EXPIRED");
      const line = lines[i];
      testObject(
        await stripe(
          "invoiceitems",
          {
            customer: a.customer_id!,
            invoice: invoice.stripe_id,
            currency: "thb",
            amount: String(line.amount),
            description: line.description,
            "period[start]": String(Math.floor(line.start / 1000)),
            "period[end]": String(Math.floor(line.end / 1000)),
          },
          `erpx-invoice-${invoice.id}-line-${i}`,
        ),
      );
      progress.items = i + 1;
      await save();
    }
    // Recheck withdrawal before finalization; draft collection method can still change.
    const current = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (current.status === "draft") {
      if (Number(current.total) !== Number(invoice.total_satang))
        throw new Error("STRIPE_TOTAL_MISMATCH");
      await stripe(
        `invoices/${invoice.stripe_id}`,
        {
          collection_method:
            mode === "auto" ? "charge_automatically" : "send_invoice",
          ...(mode === "manual" ? { days_until_due: "7" } : {}),
          ...invoicePaymentMethods(mode, promptpay),
          auto_advance: "false",
        },
        `erpx-configure-${invoice.id}-${mode}`,
      );
      await stripe(
        `invoices/${invoice.stripe_id}/finalize`,
        { auto_advance: "false" },
        `erpx-finalize-${invoice.id}`,
      );
    }
    progress.finalized = true;
    await save();
    await c.retainInvoiceMode(invoice.id,mode);
    // Stripe owns payment retries. ERPX never calls /pay repeatedly.
    const finalized = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (finalized.status === "open" && mode === "auto")
      await stripe(
        `invoices/${invoice.stripe_id}`,
        { auto_advance: "true" },
        `erpx-collect-${invoice.id}`,
      );
    else if (finalized.status === "open" && !progress.sent) {
      await stripe(
        `invoices/${invoice.stripe_id}/send`,
        {},
        `erpx-send-${invoice.id}`,
      );
      progress.sent = true;
      await save();
    }
  } else if (job.kind === "void") {
    const s = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (s.status === "paid") throw new Error("PAID_INVOICE_IMMUTABLE");
    if (s.status !== "void")
      await stripe(
        `invoices/${invoice.stripe_id}/void`,
        {},
        `erpx-void-${invoice.id}`,
      );
  } else if (job.kind === "manual") {
    const s = testObject(await stripe(`invoices/${invoice.stripe_id}`));
    if (s.status === "open")
      await stripe(
        `invoices/${invoice.stripe_id}`,
        { auto_advance: "false" },
        `erpx-manual-${job.id}`,
      );
  }
  await mirrorInvoice(c, invoice.id, stripe, now, projectionEnabled);
  await c.completeJob(job.id);
}

async function drainBilling(
  now = ports.now(),
  stripe: BillingStripe = ports.stripe,
) {
  const promptpay=ports.promptpay;
  const ids = (await ports.accountHoldings())
    .rows;
  for (const { holding_id: id } of ids) {
    const c = await ports.session();
    let locked = false;
    try {
      locked = (
        await c.tryHoldingLock(`billing:${id}`)
      ).rows[0].locked;
      if (!locked) continue;
      await c.begin();
      await c.advance(id,now);
      await c.commit();
      // Portal changes and detached cards are reflected without storing full card data.
      const account = (
        await c.currentAccount(id)
      ).rows[0];
      if (account.customer_id) {
        try {
          const customer = testObject(
            await stripe(
              `customers/${account.customer_id}?expand[]=invoice_settings.default_payment_method`,
            ),
          );
          const settings = customer.invoice_settings as
            | { default_payment_method?: StripeObject | string | null }
            | undefined;
          const preferred = settings?.default_payment_method;
          const card =
            typeof preferred === "object" && preferred
              ? preferred
              : typeof preferred === "string"
                ? testObject(await stripe(`payment_methods/${preferred}`))
                : stripeList(
                    await stripe(
                      `payment_methods?customer=${encodeURIComponent(account.customer_id)}&type=card&limit=1`,
                    ),
                  )[0];
          const details = card?.card as Record<string, unknown> | undefined;
          const masked = details
            ? JSON.stringify({
                id: card.id,
                brand: details.brand,
                last4: details.last4,
                exp_month: details.exp_month,
                exp_year: details.exp_year,
              })
            : null;
          if (masked !== account.card_json)
            await c.retainCurrentCard(id,masked);
        } catch {
          /* A temporary Stripe outage must not erase the previously verified card. */
        }
      }
      const jobs = (
        await c.pendingJobs(id,now)
      ).rows;
      for (const job of jobs) {
        try {
          await processJob(c, job, stripe, now, promptpay, true);
        } catch (error) {
          const code=ports.errorCode(error);
          await c.rescheduleJob(job.id,job.attempts >= 5 ||
              [
                "IDEMPOTENCY_WINDOW_EXPIRED",
                "STRIPE_TOTAL_MISMATCH",
                "PAID_INVOICE_IMMUTABLE",
                "SANDBOX_REQUIRED",
                "STRIPE_AUTHENTICATION_REQUIRED",
                "STRIPE_PERMISSION_REQUIRED",
              ].includes(code)
                ? "attention"
                : "pending",now + Math.min(3600000, 30000 * 2 ** job.attempts),/^[A-Z_]+$/.test(code) ? code : "STRIPE_RETRY_REQUIRED");
        }
      }
      const events = (
        await c.pendingEvents(now,id)
      ).rows;
      for (const event of events) {
        try {
          const e = JSON.parse(event.payload_json) as LegacyStripeEvent & {data:{object:{id:string;invoice?:unknown}}},
            obj = e.data.object;
          const matches = (
            await c.eventInvoices(id,e.type.startsWith("invoice.")
                  ? obj.id
                  : typeof obj.invoice === "string"
                    ? obj.invoice
                    : null)
          ).rows;
          for (const m of matches) await mirrorInvoice(c, m.id, stripe, now, true);
          if (e.type.startsWith("setup_intent.")) {
            const setup = testObject(await stripe(`setup_intents/${obj.id}`));
            if (
              setup.customer !==
              (
                await c.setupCustomer(id)
              ).rows[0].customer_id
            )
              throw new Error("SETUP_CUSTOMER_MISMATCH");
            await c.retainSetupState(id,String(setup.status));
            if (
              setup.status === "succeeded" &&
              setup.customer ===
                (
                  await c.verifiedSetupCustomer(id)
                ).rows[0].customer_id &&
              typeof setup.payment_method === "string"
            ) {
              const card = testObject(
                await stripe(`payment_methods/${setup.payment_method}`),
              );
              const details = card.card as Record<string, unknown> | undefined;
              if (details)
                await c.retainSetupCard(id,JSON.stringify({
                      id: card.id,
                      brand: details.brand,
                      last4: details.last4,
                      exp_month: details.exp_month,
                      exp_year: details.exp_year,
                    }));
            }
          }
          if (e.type.startsWith("customer.subscription."))
            await ports.applyLegacyEvent(e);
          await c.applyEvent(event.id,now);
        } catch {
          await c.rescheduleEvent(event.id,now + 60000,"STRIPE_RECONCILIATION_REQUIRED");
        }
      }
      // Recover missed deliveries and keep payment state current, independent of redirects.
      const stale = (
        await c.staleInvoices(id,now - 300000)
      ).rows;
      for (const row of stale) {
        try {
          await mirrorInvoice(c, row.id, stripe, now, true);
        } catch {
          /* Next scheduled pass retries. */
        }
      }
      // A missed credit-note webhook can arrive after an invoice is already
      // paid. Re-read recent target invoices periodically to mirror reversals.
      const paidTarget = (await c.paidTargetInvoices(id,now - 6 * 3600000)).rows;
      for (const row of paidTarget) {
        try {
          await mirrorInvoice(c, row.id, stripe, now, true);
        } catch {
          /* Signed events and the next periodic pass retain recovery paths. */
        }
      }
    } catch (error) {
      await c.rollback().catch(() => {});
      throw error;
    } finally {
      if (locked)
        await c.unlockHolding(`billing:${id}`);
      c.release();
    }
  }
  // Legacy-only customers also need durable reconciliation after the endpoint acknowledges.
  const legacy = (
    await ports.legacyEvents(now)
  ).rows;
  for (const event of legacy) {
    try {
      await ports.applyLegacyEvent(JSON.parse(event.payload_json));
    } catch {
      await ports.retryLegacy(event.id,now+60000,"STRIPE_RECONCILIATION_REQUIRED");
    }
  }
}
return {mirrorInvoice,drainBilling};}
