import { nativeIdentityWrite } from '@erpx/server-identity_access/public/native-identity-write';
import { activeIamRoleProbeSql } from './active-role-probe-family-r25';
import type {PoolClient} from 'pg';import type {AttributeRow} from '@erpx/server-people_payroll/public/people-template-contract';export function peopleTemplateAttributes(client:PoolClient){return {attributes(companyId:string){return client.query<AttributeRow>(
        "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id",
        [companyId],
      );},
activeRole(role:string,companyId:string){return client.query<Record<string,unknown>>(
              activeIamRoleProbeSql(false),
              [role, companyId],
            );},
groupReferences(companyId:string,value:string){return client.query<Record<string,unknown>>(
            "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)",
            [companyId, value],
          );},
upsertAttribute(companyId:string,id:string,value:string){return nativeIdentityWrite("w_e1b9831be77a", [companyId, id, value]);},
attributeHistory(companyId:string,id:string,value:unknown,value2:string,now:number){return nativeIdentityWrite("w_1044fec8e863", [
          companyId,
          id,
          value,
          value2,
          now,
        ]);}};}
