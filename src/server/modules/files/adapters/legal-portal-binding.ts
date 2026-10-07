import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function legalPortalBindingWrites(db:SqlDatabase){return {
bindLink:(facts:{now:number;key:string;linkId:string;sourceId:string;})=>db.prepare("UPDATE attachment_links SET state='bound',bound_at=?,binding_type='legal_portal_submission',binding_id=? WHERE id=? AND state='draft' AND attachment_id=?").bind(facts.now,facts.key,facts.linkId,facts.sourceId),
bindSet:(facts:{now:number;key:string;now2:number;setId:string;expectedTargetVersion:number;})=>db.prepare("UPDATE attachment_sets SET state='bound',bound_at=?,bound_command_id=?,updated_at=? WHERE id=? AND state='draft' AND expected_target_version=?").bind(facts.now,facts.key,facts.now2,facts.setId,facts.expectedTargetVersion),
bindEvent:(facts:{fact0:string;company_id:string;sourceId:string;linkId:string;setId:string;actorId:string;fact6:"set.bound";fact7:string;now:number;})=>db.prepare("INSERT INTO attachment_events(id,company_id,attachment_id,link_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.fact0,facts.company_id,facts.sourceId,facts.linkId,facts.setId,facts.actorId,facts.fact6,facts.fact7,facts.now)
};}
