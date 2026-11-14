import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Retained team branch/department topology effects only. Company, staff-level
 * and CEO-position seeding now belongs exclusively to the native onboarding
 * command (`server-manage_erpx/src/adapters/onboarding_command.rs`), so those
 * retired TypeScript writers are not re-exported here. Effects bind the
 * caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareBranch(id:string,slug:string,name:string,company:string,company2:string,id2:string){return db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,?,?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(id, slug, name, company, company2, id2);},
prepareDepartment(value:string,id:string,company:string,id2:string){return db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department','team','Team',?,?,?) ON CONFLICT DO NOTHING",
      )
      .bind(value, id, company, id2);}};}
