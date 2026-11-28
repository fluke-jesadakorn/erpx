import { nativeIdentityWrite } from '@erpx/identity-access-server/public/native-identity-write';
import {
  nativePeopleTemplateActiveRole,
  nativePeopleTemplateAttributes,
  nativePeopleTemplateGroupReferences,
  type LaneHoldingScope,
} from './native-lane-12/client';
import "server-only";
import type {PoolClient} from 'pg';import type {AttributeRow} from '@erpx/people-payroll-server/public/people-template-contract';

/** Native lane 12 owns the three owner-local reads below. This adapter is
 * transport only: it sends bounded company/role/value inputs and pins the
 * `{rows, rowCount}` response. There is no TypeScript SQL fallback. */
export function peopleTemplateAttributes(_client:PoolClient,scope?:LaneHoldingScope){return {
  attributes(companyId:string){return nativePeopleTemplateAttributes<AttributeRow>(companyId,scope);},
  activeRole(role:string,companyId:string){return nativePeopleTemplateActiveRole(companyId,role,scope);},
  groupReferences(companyId:string,value:string){return nativePeopleTemplateGroupReferences(companyId,value,scope);},
  upsertAttribute(companyId:string,id:string,value:string){return nativeIdentityWrite("w_e1b9831be77a", [companyId, id, value]);},
  attributeHistory(companyId:string,id:string,value:unknown,value2:string,now:number){return nativeIdentityWrite("w_1044fec8e863", [
          companyId,
          id,
          value,
          value2,
          now,
        ]);}
};}
