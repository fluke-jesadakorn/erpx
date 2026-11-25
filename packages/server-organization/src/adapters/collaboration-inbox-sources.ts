import "server-only";
import {headers} from 'next/headers';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeNodeReferences} from './native-lane-23/client';

/** Native owner read: the SQL and projection live in
 * `erpx-server-organization::adapters::native_lane_23`; this adapter only
 * transports validated facts and fails closed when unavailable. */
export function collaborationInboxSources(_db: SqlDatabase) {
  return {
    async audienceNode(id: string, companyId: string) {
      return (await nativeNodeReferences(await headers(), { companyId, nodeId: id })).node;
    },
  };
}
