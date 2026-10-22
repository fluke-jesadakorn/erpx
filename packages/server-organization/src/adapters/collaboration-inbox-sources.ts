import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';export function collaborationInboxSources(db:SqlDatabase){return {async audienceNode(id:string,companyId:string){return db.prepare("SELECT id,kind,branch_id FROM organization_nodes WHERE id=? AND company_id=? AND status='active'")
        .bind(id,companyId).first<{id:string;kind:string;branch_id:string|null}>();}};}
