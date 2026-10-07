import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
const mediaPredicate=`EXISTS(SELECT 1 FROM attachment_publications p
 JOIN attachment_links l ON l.id=p.source_link_id AND l.state='bound'
 JOIN attachment_sets s ON s.id=l.set_id AND s.company_id=p.company_id AND s.state='bound'
 JOIN attachments a ON a.id=l.attachment_id AND a.company_id=p.company_id AND a.source_state='available'
 WHERE p.id=? AND p.company_id=? AND p.state='approved' AND p.target_type='marketing_asset' AND p.target_id=?
 AND s.target_type=p.target_type AND s.target_id=p.target_id AND s.purpose_key='creative')`;
export function prepareMarketingCreativeGuards(db:SqlDatabase,key:string,companyId:string,publicationId:string,assetId:string):SqlStatement[]{return [db.prepare('SELECT id FROM attachment_publications WHERE id=? AND company_id=? FOR SHARE').bind(publicationId,companyId),assertion(db,key,'form-media',mediaPredicate,[publicationId,companyId,assetId])];}
export function approvedMarketingCreative(db:SqlDatabase,companyId:string,publicationId:string,assetId:string){return db.prepare(`SELECT p.id,p.alt_text,p.revision,p.derivative_media_type FROM attachment_publications p WHERE p.id=? AND ${mediaPredicate}`).bind(publicationId,publicationId,companyId,assetId).first<{id:string;alt_text:string;revision:number;derivative_media_type:string}>();}
