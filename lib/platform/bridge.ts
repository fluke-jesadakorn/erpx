import 'server-only';

import { controlReadQuery, query } from '@/db';
import {
  buildBridgeReadStatus,
  type BridgeCustomerCounts,
  type BridgeEventHighWatermarks,
  type BridgeIdentityEvidence,
  type FolioBridgeReadStatus,
} from './bridge-contract';

interface SourceCustomerCounts {
  customer_count: string;
  active_customer_count: string;
  identity_digest: string | null;
}

interface TargetCustomerCounts {
  customer_count: string;
  active_customer_count: string;
  linked_customer_count: string;
  unlinked_customer_count: string;
  identity_link_digest: string | null;
}

interface EventHighWatermarkRow {
  event_high_watermark: string | null;
}

interface RelationNameRow {
  relation_name: string | null;
}

function numberOrNull(value: string | number | null | undefined): number | null {
  if (value == null) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

export async function loadFolioBridgeReadStatus(): Promise<FolioBridgeReadStatus> {
  const targetConfigured = Boolean(process.env.ERPX_CONTROL_READ_DATABASE_URL?.trim());
  const observedAt = new Date().toISOString();
  const [source, sourceEvents, target, targetEvents] = await Promise.allSettled([
    query<SourceCustomerCounts>(
      `SELECT count(*)::text AS customer_count,
              count(*) FILTER (WHERE is_active)::text AS active_customer_count,
              md5(COALESCE(string_agg(id::text, ',' ORDER BY id), '')) AS identity_digest
         FROM folio.customers`,
    ),
    query<EventHighWatermarkRow>(
      `SELECT max(id)::text AS event_high_watermark
         FROM folio.domain_events`,
    ),
    controlReadQuery<TargetCustomerCounts>(
      `SELECT count(*)::text AS customer_count,
              count(*) FILTER (WHERE status = 'active')::text AS active_customer_count,
              count(*) FILTER (WHERE legacy_customer_id IS NOT NULL)::text AS linked_customer_count,
              count(*) FILTER (WHERE legacy_customer_id IS NULL)::text AS unlinked_customer_count,
              md5(COALESCE(string_agg(legacy_customer_id::text, ',' ORDER BY legacy_customer_id)
                    FILTER (WHERE legacy_customer_id IS NOT NULL), '')) AS identity_link_digest
         FROM platform.customer_accounts`,
    ),
    loadTargetEventHighWatermark(),
  ]);

  const sourceRow = source.status === 'fulfilled' ? source.value.rows[0] : undefined;
  const targetRow = target.status === 'fulfilled' ? target.value.rows[0] : undefined;
  const sourceEventRow = sourceEvents.status === 'fulfilled' ? sourceEvents.value.rows[0] : undefined;
  const targetEventRow = targetEvents.status === 'fulfilled' ? targetEvents.value : undefined;
  const counts: BridgeCustomerCounts = {
    sourceCustomerCount: numberOrNull(sourceRow?.customer_count),
    sourceActiveCustomerCount: numberOrNull(sourceRow?.active_customer_count),
    targetCustomerCount: numberOrNull(targetRow?.customer_count),
    targetActiveCustomerCount: numberOrNull(targetRow?.active_customer_count),
    targetLinkedCustomerCount: numberOrNull(targetRow?.linked_customer_count),
    targetUnlinkedCustomerCount: numberOrNull(targetRow?.unlinked_customer_count),
  };

  const identityEvidence: BridgeIdentityEvidence = {
    sourceIdentityDigest: sourceRow?.identity_digest ?? null,
    targetIdentityLinkDigest: targetRow?.identity_link_digest ?? null,
  };
  const eventHighWatermarks: BridgeEventHighWatermarks = {
    source: sourceEventRow?.event_high_watermark ?? null,
    target: targetEventRow?.event_high_watermark ?? null,
  };

  return buildBridgeReadStatus({
    targetConfigured,
    sourceAvailable: source.status === 'fulfilled',
    targetAvailable: target.status === 'fulfilled',
    counts,
    identityEvidence,
    eventHighWatermarks,
    observedAt,
  });
}

/**
 * A target event watermark is optional at the storage boundary, but never
 * optional to a reconciled result. The bridge only reads a target event
 * ledger when one actually exists; it deliberately does not use updated_at,
 * row counts, or a made-up watermark as a substitute.
 */
async function loadTargetEventHighWatermark(): Promise<EventHighWatermarkRow> {
  const relationResult = await controlReadQuery<RelationNameRow>(
    `SELECT CASE
       WHEN to_regclass('platform.domain_events') IS NOT NULL THEN 'platform.domain_events'
       ELSE NULL
     END AS relation_name`,
  );
  const relationName = relationResult.rows[0]?.relation_name;
  if (relationName !== 'platform.domain_events') {
    return { event_high_watermark: null };
  }

  const result = await controlReadQuery<EventHighWatermarkRow>(
    `SELECT max(id)::text AS event_high_watermark
       FROM platform.domain_events`,
  );
  return { event_high_watermark: result.rows[0]?.event_high_watermark ?? null };
}
