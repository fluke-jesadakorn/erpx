import type {WorkSnapshot} from "@erpx/server-work_projection/domain/work-items";
export type CacheFence = { revision: string; database: string; validUntil: number };
// Route handlers, RSC and HMR share the same in-flight computation in this process.
export type InFlight = { result: Promise<WorkSnapshot>; validUntil: number };
export type CachedResult = { text: string; validUntil: number; bytes: number };
