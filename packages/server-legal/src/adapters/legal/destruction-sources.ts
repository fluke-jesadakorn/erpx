import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {legalDestructionSourceFacts} from "@erpx/server-files/public/legal-destruction-source-facts";
import type {DestructionDocument,DestructionUpload,LegalDestructionManifest} from "@erpx/server-legal/domain/destruction-sources";
import {createLegalDestructionApplication} from "@erpx/server-legal/application/destruction-sources";
import { legalDestructionStatements } from '@erpx/server-collaboration/adapters/native-legal-destruction-events';
import {legalDisposedGuardSql} from '../legal-read-tail-r39-sources';
import {legalItemKeyHeadSql} from '../legal-read-tail-r54-sources';
import {legalExternalUploadShareJoinHead} from '../legal-read-tail-r44-sources';
import {legalCleanDocumentGuardHead} from '../legal-read-tail-r49-sources';
export type {LegalDestructionManifest} from "@erpx/server-legal/domain/destruction-sources";
function application(db:SqlDatabase){const files=legalDestructionSourceFacts(db),native=legalDestructionStatements(db);return createLegalDestructionApplication({clock:{now:()=>Date.now()},repository:{
documents:(facts:{itemId:string;})=>db.prepare(legalItemKeyHeadSql('id,checksum,object_key,attachment_source_id,attachment_link_id','legal_documents')+` AND ${legalDisposedGuardSql('disposed','legal_documents.id')}`).bind(facts.itemId).all<DestructionDocument>(),
uploads:(facts:{itemId:string;})=>db.prepare(legalExternalUploadShareJoinHead('upload.id,upload.object_key,upload.checksum,upload.attachment_source_id,upload.attachment_link_id','upload','share')+'WHERE share.item_id=?').bind(facts.itemId).all<DestructionUpload>(),
canonicalSource:facts=>files.canonicalSource(facts),
storedLedger:facts=>files.storedLedger(facts),
documentCurrent:(facts:{id:string;itemId:string;checksum:string;object_key:string | null;})=>db.prepare(legalCleanDocumentGuardHead('','object_key IS NOT DISTINCT FROM ?')).bind(facts.id,facts.itemId,facts.checksum,facts.object_key).first(),
uploadCurrent:(facts:{id:string;itemId:string;checksum:string;object_key:string;})=>db.prepare(legalExternalUploadShareJoinHead('1','upload','share')+'WHERE upload.id=? AND share.item_id=? AND upload.checksum=? AND upload.object_key=?').bind(facts.id,facts.itemId,facts.checksum,facts.object_key).first(),
lockedSource:facts=>files.lockedSource(facts),
linkCurrent:facts=>files.linkCurrent(facts),
sourceShared:facts=>files.sourceShared(facts),
retainedLedger:facts=>files.retainedLedger(facts),
objectShared:facts=>files.objectShared(facts),
retireSource:async facts=>{await (await native.retireSource(facts)).run();},
retireLedger:async facts=>{await (await native.retireLedger(facts)).run();},
recordRetirement:async facts=>{await (await native.recordRetirement(facts)).run();}
}});}
export function legalDestructionManifest(db:SqlDatabase,itemId:string,companyId:string){return application(db).legalDestructionManifest(itemId,companyId);}
export function validateLegalDestructionSources(db:SqlDatabase,itemId:string,companyId:string,manifest:LegalDestructionManifest){return application(db).validateLegalDestructionSources(itemId,companyId,manifest);}
export function sealLegalDestructionSources(db:SqlDatabase,companyId:string,destructionId:string,manifest:LegalDestructionManifest){return application(db).sealLegalDestructionSources(companyId,destructionId,manifest);}
