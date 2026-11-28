import "server-only";
import {nativeWorkflowWrite} from './native-workflow-write';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
export type RuntimePendingEvent = { id: string; event_type: string; aggregate_id: string; aggregate_version: number; payload_json: string; attempts: number; lease_token: string };
/** Finite job lease effects on the worker's supplied connection. */
export function runtimeJobEffects(db: SqlDatabase, excludedSourceSql?: string, holdingId?: string) {
  const scope = holdingId ? { holdingId } : undefined;
  return {
    claim(token: string, until: number, readyAt: number, expiredAt: number) {
      // Bind order matches the owner CTE: eligibility window first, then the lease.
      return nativeWorkflowWrite('wf_native_runtime_job_effects_20', [readyAt, expiredAt, token, until]).withContext(scope ?? {}).first<RuntimePendingEvent>();
    },
    renew(until: number, id: string, token: string) { return nativeWorkflowWrite('wf_native_runtime_job_effects_21', [until, id, token]).withContext(scope ?? {}).run(); },
    complete(processedAt: number, publishedAt: number, id: string, token: string) { return nativeWorkflowWrite('wf_native_runtime_job_effects_22', [processedAt, publishedAt, id, token]).withContext(scope ?? {}).run(); },
    failed(retryAt: number, code: string, deadAt: number | null, id: string, token: string) { return nativeWorkflowWrite('wf_native_runtime_job_effects_23', [retryAt, code, deadAt, id, token]).withContext(scope ?? {}).run(); },
  };
}
