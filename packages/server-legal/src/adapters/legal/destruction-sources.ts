import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {legalDestructionSourceFacts} from "@erpx/server-files/public/legal-destruction-source-facts";
import type {DestructionDocument,DestructionUpload,LegalDestructionManifest} from "@erpx/server-legal/domain/destruction-sources";
import {createLegalDestructionApplication} from "@erpx/server-legal/application/destruction-sources";
export type {LegalDestructionManifest} from "@erpx/server-legal/domain/destruction-sources";
function application(db:SqlDatabase){const files=legalDestructionSourceFacts(db);return createLegalDestructionApplication({clock:{now:()=>Date.now()},repository:{
documents:(facts:{itemId:string;})=>db.prepare("SELECT id,checksum,object_key,attachment_source_id,attachment_link_id FROM legal_documents WHERE item_id=? AND NOT EXISTS(SELECT 1 FROM legal_disposals disposed WHERE disposed.document_id=legal_documents.id)").bind(facts.itemId).all<DestructionDocument>(),
uploads:(facts:{itemId:string;})=>db.prepare("SELECT upload.id,upload.object_key,upload.checksum,upload.attachment_source_id,upload.attachment_link_id FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE share.item_id=?").bind(facts.itemId).all<DestructionUpload>(),
canonicalSource:facts=>files.canonicalSource(facts),
storedLedger:facts=>files.storedLedger(facts),
documentCurrent:(facts:{id:string;itemId:string;checksum:string;object_key:string | null;})=>db.prepare("SELECT 1 FROM legal_documents WHERE id=? AND item_id=? AND checksum=? AND object_key IS NOT DISTINCT FROM ?").bind(facts.id,facts.itemId,facts.checksum,facts.object_key).first(),
uploadCurrent:(facts:{id:string;itemId:string;checksum:string;object_key:string;})=>db.prepare("SELECT 1 FROM legal_external_uploads upload JOIN legal_shares share ON share.id=upload.share_id WHERE upload.id=? AND share.item_id=? AND upload.checksum=? AND upload.object_key=?").bind(facts.id,facts.itemId,facts.checksum,facts.object_key).first(),
lockedSource:facts=>files.lockedSource(facts),
linkCurrent:facts=>files.linkCurrent(facts),
sourceShared:facts=>files.sourceShared(facts),
retainedLedger:facts=>files.retainedLedger(facts),
objectShared:facts=>files.objectShared(facts),
retireSource:facts=>files.retireSource(facts),
retireLedger:facts=>files.retireLedger(facts),
recordRetirement:facts=>files.recordRetirement(facts)
}});}
export function legalDestructionManifest(db:SqlDatabase,itemId:string,companyId:string){return application(db).legalDestructionManifest(itemId,companyId);}
export function validateLegalDestructionSources(db:SqlDatabase,itemId:string,companyId:string,manifest:LegalDestructionManifest){return application(db).validateLegalDestructionSources(itemId,companyId,manifest);}
export function sealLegalDestructionSources(db:SqlDatabase,companyId:string,destructionId:string,manifest:LegalDestructionManifest){return application(db).sealLegalDestructionSources(companyId,destructionId,manifest);}
