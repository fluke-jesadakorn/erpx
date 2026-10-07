import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {preparePerson(person:string,id:string,company:string,name:string,email:string,id2:string,now:number,now2:number){return db
      .prepare(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,email,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'TH',?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(person, id, company, name, email, id2, now, now2);},
prepareEmployee(value:string,id:string,company:string,branch:string,department:string,name:string,now:number){return db
      .prepare(
        "INSERT INTO employees(id,user_id,company_id,branch_id,department_id,display_name,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        value,
        id,
        company,
        branch,
        department,
        name,
        now,
      );},
prepareEngagement(value:string,person:string,company:string,branch:string,department:string,id:string,now:number,now2:number,value2:string){return db
      .prepare(
        "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) VALUES(?,?,?,?,?,'employee','salaried','active',to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'),'Team member','THB',?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(
        value,
        person,
        company,
        branch,
        department,
        id,
        now,
        now2,
        value2,
      );}};}
