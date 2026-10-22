import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareBilling(holding_id:string,now:number){return db
      .prepare(
        "INSERT INTO beta_billing(holding_id,updated_at) VALUES(?,?) ON CONFLICT DO NOTHING",
      )
      .bind(holding_id, now);}};}
