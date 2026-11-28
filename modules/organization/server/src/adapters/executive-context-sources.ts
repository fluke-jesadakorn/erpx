import "server-only";
import {headers} from 'next/headers';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {nativeExecutiveEvidence} from './native-lane-23/client';

type NodeRow={id:string;kind:string;display_name:string;parent_id:string|null;branch_id:string|null};
type PositionRow={id:string;title:string;branch_id:string|null;department_id:string|null;level_id:string;reports_to_position_id:string|null;level_number:number|null};

/** Native owner reads: SQL and projection live in the Rust organization crate;
 * these adapters transport validated facts and fail closed when unavailable. */
export async function executiveOrganizationNodes(_db:SqlDatabase,a:WorkContext){
  const evidence=await nativeExecutiveEvidence(await headers(),{
    kind:'context-nodes',
    companyId:a.companyId,
    values:[a.branchId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.departmentId,a.companyId],
  });
  return evidence.rows as unknown as NodeRow[];
}

export async function executiveOrganizationPositions(_db:SqlDatabase,a:WorkContext){
  const evidence=await nativeExecutiveEvidence(await headers(),{
    kind:'context-positions',
    companyId:a.companyId,
    values:[a.branchId,a.branchId,a.departmentId,a.departmentId],
  });
  return evidence.rows as unknown as PositionRow[];
}
