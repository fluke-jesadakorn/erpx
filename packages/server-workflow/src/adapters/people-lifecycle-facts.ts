import "server-only";
import {nativeWorkflowWrite} from './native-workflow-write';
import type { PoolClient } from "pg";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleLifecycleWorkflowFacts(database: SqlDatabase, client?: PoolClient) { return { applyLifecycleOutboxEffect(value: unknown, key: unknown, event: unknown, id: unknown, version: unknown, value2: unknown, value3: unknown) { return nativeWorkflowWrite('wf_native_lifecycle_outbox', [value, key, event, id, version, value2, value3]); } }; }
