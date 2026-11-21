import { nativeLegalJobHealth } from "@erpx/server-collaboration/adapters/native-legal-job-health";
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function runtimeLegalJobHealthEffects(db: SqlDatabase, holdingId: string) {
  const native = nativeLegalJobHealth(db, holdingId);
  return {
    renew(at: number, task: string) { return native.renew(at, task); },
    started(task: string, at: number) { return native.started(task, at); },
    succeeded(heartbeatAt: number, successAt: number, task: string) { return native.succeeded(heartbeatAt, successAt, task); },
    failed(at: number, code: string, task: string) { return native.failed(at, code, task); },
  };
}
