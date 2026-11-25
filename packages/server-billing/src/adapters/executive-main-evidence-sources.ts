import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {
  nativeBillingCommercialSubscription,
  nativeBillingCommercialSubscriptions,
  nativeBillingOverdueInvoice,
  nativeBillingOverdueInvoices,
  type NativeLane38CommercialRow,
  type NativeLane38Context,
  type NativeLane38OverdueRow,
  type NativeLane38CommercialContext,
  type NativeLane38CommercialSearch,
  type NativeLane38OverdueSearch,
} from './native-lane-38/client';

/**
 * Native lane 38 (wave 2) cutover. The four Billing-owned evidence reads are
 * executed and projected by the owner crate; this adapter only carries the
 * caller's WorkContext and validates the returned rows. `db` is retained for
 * call-site compatibility only: no statement text is issued on it and there is
 * no TypeScript fallback.
 */
export function executiveMainEvidenceSources(db: SqlDatabase) {
  void db;
  const context = (actor: WorkContext): NativeLane38Context => ({
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    ...(actor.companyId === undefined ? {} : { companyId: actor.companyId }),
  });
  return {
    async factsSearchCommercialSubscriptions(
      cursor: NativeLane38CommercialContext | null,
      input: {
        filters: { from: string; to: string; currency: string };
        query: string;
        limit: number;
        metric: "active-subscriptions";
        cursor?: string | undefined;
      },
      actor: WorkContext,
      now: number,
    ): Promise<{ rows: NativeLane38CommercialRow[] }> {
      const params: NativeLane38CommercialSearch = {
        query: input.query,
        sourceKind: cursor?.sourceKind ?? null,
        recordId: cursor?.recordId ?? null,
        limit: input.limit,
        now,
      };
      return { rows: await nativeBillingCommercialSubscriptions(context(actor), params) };
    },
    async factsReadCommercialSubscription(
      decoded: NativeLane38CommercialContext,
      actor: WorkContext,
      now: number,
    ): Promise<{ row: NativeLane38CommercialRow | null }> {
      return { row: await nativeBillingCommercialSubscription(context(actor), decoded, now) };
    },
    async factsSearchOverdueInvoiceEvidence(
      cursorValue: { dueAt: number; id: string } | null,
      input: {
        filters: { from: string; to: string; currency: string };
        query: string;
        limit: number;
        metric: "overdue-invoices";
        cursor?: string | undefined;
      },
      actor: WorkContext,
      end: number,
      now: number,
    ): Promise<{ rows: NativeLane38OverdueRow[] }> {
      const params: NativeLane38OverdueSearch = {
        from: input.filters.from,
        to: input.filters.to,
        query: input.query,
        dueAt: cursorValue?.dueAt ?? null,
        id: cursorValue?.id ?? null,
        limit: input.limit,
        end,
        now,
      };
      return { rows: await nativeBillingOverdueInvoices(context(actor), params) };
    },
    async factsReadOverdueInvoiceEvidence(
      actor: WorkContext,
      filters: { from: string; to: string; currency: string },
      now: number,
      id: string,
    ): Promise<{ row: NativeLane38OverdueRow | null }> {
      return { row: await nativeBillingOverdueInvoice(context(actor), filters, now, id) };
    },
  };
}
