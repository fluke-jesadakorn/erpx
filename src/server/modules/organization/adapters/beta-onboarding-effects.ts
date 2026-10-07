import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareBranch(id:string,slug:string,name:string,company:string,company2:string,id2:string){return db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(id, slug, name, company, company2, id2);},
prepareDepartment(value:string,id:string,company:string,id2:string){return db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department','team','Team',?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(value, id, company, id2);},
prepareCompany(company:string,slug:string,company_name:string,company2:string){return db
        .prepare(
          "INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(company, slug, company_name, company2);},
prepareMemberLevel(value:string,company:string,now:number,now2:number){return db
        .prepare(
          "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,created_at,updated_at) VALUES(?,?,'member','Team member',1,NULL,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(value, company, now, now2);},
prepareOrganizationLevel(value:string,company:string,value2:string,name:string | number,value3:number,value4:number,now:number,now2:number){return db
          .prepare(
            "INSERT INTO staff_levels(id,company_id,code,name,rank,level_number,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(company_id,code) DO UPDATE SET name=excluded.name,rank=excluded.rank,level_number=excluded.level_number,active=1,updated_at=excluded.updated_at",
          )
          .bind(
            value,
            company,
            value2,
            name,
            value3,
            value4,
            now,
            now2,
          );},
prepareCeoPosition(value:string,company:string,value2:string,id:string,now:number,now2:number){return db
        .prepare(
          "INSERT INTO organization_positions(id,company_id,branch_id,department_id,title,level_id,reports_to_position_id,is_ceo,active,version,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,'Chief Executive Officer',?,NULL,1,1,1,?,?,?) ON CONFLICT(id) DO UPDATE SET level_id=excluded.level_id,is_ceo=1,active=1,version=organization_positions.version+1,updated_at=excluded.updated_at",
        )
        .bind(value, company, value2, id, now, now2);}};}
