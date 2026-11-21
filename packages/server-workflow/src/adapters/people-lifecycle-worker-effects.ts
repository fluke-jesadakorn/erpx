import type { PoolClient } from 'pg';
import {nativeWorkflowWrite} from './native-workflow-write';
export function lifecycleOutboxEffects(client: PoolClient) { return { applyEffect23(value: string, value2: string, id: string, value3: number, value4: string, now: number) { return nativeWorkflowWrite('wf_native_lifecycle_worker_outbox', [value, value2, id, value3, value4, now]); } }; }
