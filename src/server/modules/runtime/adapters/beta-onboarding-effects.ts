import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Effects bind the caller's transaction; this owner is the only source of its table mutations. */
export function betaOnboardingEffects(db:SqlDatabase){return {prepareBinding(holding_id:string,slug:string,company_name:string,writeBinding:string,read:string,files:string){return db
      .prepare(
        "INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status) VALUES(?,?,?,?,?,?,'active') ON CONFLICT(holding_id) DO NOTHING",
      )
      .bind(
        holding_id,
        slug,
        company_name,
        writeBinding,
        read,
        files,
      );}};}
