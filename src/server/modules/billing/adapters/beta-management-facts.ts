import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function betaManagementFacts(db:SqlDatabase){return {async billing(holdingId:string){return db.prepare(
              "SELECT status FROM beta_billing WHERE holding_id=?",
            )
              .bind(holdingId)
              .first();}};}
