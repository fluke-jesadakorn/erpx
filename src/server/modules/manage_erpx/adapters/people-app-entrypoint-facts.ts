import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function ownerOnboardingSlot(db:SqlDatabase,userId:string){return db.prepare(
    "SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?",
  )
    .bind(userId)
    .first<{
      slug: string;
      company_name: string;
      branch_name: string;
      state: string;
    }>();}
