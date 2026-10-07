import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareAssignment(value:string,id:string,value2:"holding_owner" | "employee",company:string,value3:string | null,now:number,value4:number,value5:"holding" | "branch",value6:string | null,value7:string | null,value8:"manual" | "legacy"){return db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,scope_kind,scope_node_id,parent_id,source) VALUES(?,?,?,'[]',?,?,?,? ,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        value,
        id,
        value2,
        company,
        value3,
        now,
        value4,
        value5,
        value6,
        value7,
        value8,
      );},
preparePrincipal(value:string,id:string,name:string,now:number){return db
      .prepare(
        "INSERT INTO principals(id,user_id,display_name,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(value, id, name, now);}};}
