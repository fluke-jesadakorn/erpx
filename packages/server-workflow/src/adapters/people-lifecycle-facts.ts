import "server-only";
import type { PoolClient } from "pg";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function peopleLifecycleWorkflowFacts(database: SqlDatabase, client?: PoolClient) { return { applyLifecycleOutboxEffect(value: unknown, key: unknown, event: unknown, id: unknown, version: unknown, value2: unknown, value3: unknown) { return client!.query<Record<string, unknown>>("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,'people_lifecycle',$4,$5,$6,$7)", [value, key, event, id, version, value2, value3]); } }; }
