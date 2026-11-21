import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import { nativeClearCommandAssertions } from '@erpx/server-collaboration/adapters/native-command-assertions';

export function betaInvitationSources(db:SqlDatabase){return {prepareAcceptanceAudit(value:string,key:string,id:string,value2:string,id2:string,now:number){return db
        .prepare(
          "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,'beta.member.join','beta_member',?,1,'Verified invitation acceptance',?)",
        )
        .bind(
          value,
          key,
          id,
          value2,
          id2,
          now,
        );},
clearAcceptanceAssertions(key:string,companyId:string){return nativeClearCommandAssertions(db,companyId,key);}};}
