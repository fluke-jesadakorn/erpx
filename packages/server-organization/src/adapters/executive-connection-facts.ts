import "server-only";
import {headers} from 'next/headers';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {nativeExecutiveEvidence} from './native-lane-23/client';

type ScopeRow={id:string;kind:string;display_name:string;branch_id:string|null;branch_name:string|null};

/** Native owner read: the scope SQL and projection live in
 * `erpx-server-organization::adapters::native_lane_23`; this adapter transports
 * validated facts only and fails closed. */
export function executiveConnectionOrganizationFacts(_db:SqlDatabase){return {
  async organizationScopes(facts:{
    value1:string;value2:string|null;value3:string|null;value4:string|null;value5:string|null;value6:string|null;
  }){
    const evidence=await nativeExecutiveEvidence(await headers(),{
      kind:'connection-scopes',
      companyId:facts.value1,
      values:[facts.value2,facts.value3,facts.value4,facts.value5,facts.value6],
    });
    return {results:evidence.rows as unknown as ScopeRow[]};
  },
};}
