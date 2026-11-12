/** Declared isolated Quality inspection specification source for tests and local
 * simulations.
 *
 * Specification save/activate/cancel are owned by the native `quality.spec.*`
 * commands. This fixture writes the same rows the native activation commits — the
 * draft header, its ordered metric snapshots and, for an active source, the
 * independent approval stamped by a different manager — without minting a command
 * receipt. Metric rows are inserted while the header is still a draft because
 * `quality_spec_metric_edit_guard` freezes them after activation.
 */
import {type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';

export type QualitySpecMetricSeed = {
  code: string;
  label: string;
  unit: string;
  minimum?: string;
  maximum?: string;
};

export type QualitySpecSeed = {
  productId: string;
  name: string;
  metrics: QualitySpecMetricSeed[];
  status?: 'draft' | 'active';
  approvedBy?: string;
  approvalReason?: string;
  createdAt?: number;
};

export type QualitySpecSeedReceipt = {id: string; status: 'draft' | 'active'; version: number};

export async function seedNativeQualitySpec(
  db: SqlDatabase,
  actor: WorkContext,
  input: QualitySpecSeed,
  key: string,
): Promise<QualitySpecSeedReceipt> {
  const status = input.status ?? 'draft';
  const now = input.createdAt ?? Date.now();
  const id = `quality-spec-${key}`;
  if (status === 'active' && (!input.approvedBy || input.approvedBy === actor.userId))
    throw new Error('NATIVE_QUALITY_SPEC_REQUIRES_INDEPENDENT_APPROVER');
  await db
    .prepare(
      'INSERT INTO inventory_quality_specs(id,company_id,product_id,name,status,version,created_by,created_at,updated_at) VALUES(?,?,?,?,\'draft\',1,?,?,?)',
    )
    .bind(id, actor.companyId, input.productId, input.name, actor.userId, now, now)
    .run();
  for (const [ordinal, metric] of input.metrics.entries())
    await db
      .prepare(
        'INSERT INTO inventory_quality_spec_metrics(id,spec_id,code,label,unit,minimum_value,maximum_value,ordinal) VALUES(?,?,?,?,?,?,?,?)',
      )
      .bind(
        `${id}-metric-${ordinal}`,
        id,
        metric.code,
        metric.label,
        metric.unit,
        metric.minimum === undefined || metric.minimum === '' ? null : metric.minimum,
        metric.maximum === undefined || metric.maximum === '' ? null : metric.maximum,
        ordinal,
      )
      .run();
  if (status === 'draft') return {id, status, version: 1};
  await db
    .prepare(
      "UPDATE inventory_quality_specs SET status='active',version=2,approved_by=?,approval_reason=?,activated_at=?,updated_at=? WHERE id=?",
    )
    .bind(input.approvedBy, input.approvalReason ?? 'Independent review of the retained measurement plan', now, now, id)
    .run();
  return {id, status, version: 2};
}
