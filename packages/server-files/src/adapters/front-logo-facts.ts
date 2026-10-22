import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function frontLogoFacts(db:SqlDatabase){return {
async logoPublication(companyId:string,companyId2:string,logoPublicationId:string){return db.prepare(`SELECT p.derivative_object_key object_key,p.derivative_media_type media_type FROM attachment_publications p
    WHERE p.company_id=? AND p.target_type='company_storefront_brand' AND p.target_id=? AND p.id=? AND p.state='approved'`)
    .bind(companyId,companyId2,logoPublicationId).first<{object_key:string;media_type:string}>();}
};}
