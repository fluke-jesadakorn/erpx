import "server-only";
import {headers} from 'next/headers';
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {nativeNodeReferences} from './native-lane-23/client';

/** Native owner read: unit references are projected in the Rust owner crate;
 * this adapter only transports the validated facts and fails closed. */
export async function accessUnitFacts(_database: SqlDatabase, companyId: string) {
  return (await nativeNodeReferences(await headers(), { companyId })).units;
}
