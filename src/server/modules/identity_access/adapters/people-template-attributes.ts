import type {PoolClient} from 'pg';import type {AttributeRow} from '../../people_payroll/public/people-template-contract';export function peopleTemplateAttributes(client:PoolClient){return {attributes(companyId:string){return client.query<AttributeRow>(
        "SELECT id,version,definition FROM um_attributes WHERE company_id=$1 ORDER BY id",
        [companyId],
      );},
activeRole(role:string,companyId:string){return client.query<Record<string,unknown>>(
              "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'",
              [role, companyId],
            );},
groupReferences(companyId:string,value:string){return client.query<Record<string,unknown>>(
            "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(SELECT 1 FROM jsonb_array_elements(definition->'rule'->'conditions') x WHERE x->>'field'=$2)",
            [companyId, value],
          );},
upsertAttribute(companyId:string,id:string,value:string){return client.query<Record<string,unknown>>(
        "INSERT INTO um_attributes(company_id,id,version,definition) VALUES($1,$2,1,$3) ON CONFLICT(company_id,id) DO UPDATE SET version=um_attributes.version+1,definition=excluded.definition",
        [companyId, id, value],
      );},
attributeHistory(companyId:string,id:string,value:unknown,value2:string,now:number){return client.query<Record<string,unknown>>(
        "INSERT INTO um_history(company_id,entity,object_id,version,definition,created_at) VALUES($1,'attribute',$2,$3,$4,$5)",
        [
          companyId,
          id,
          value,
          value2,
          now,
        ],
      );}};}
