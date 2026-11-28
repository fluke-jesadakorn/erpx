import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {attachmentsByIdCompanySql} from './read-tail-r26-source-sql';
export function collaborationMessageSource(db:SqlDatabase,id:string,companyId:string){return db.prepare(attachmentsByIdCompanySql('object_key,checksum_sha256')).bind(id,companyId).first<{object_key:string;checksum_sha256:string}>();}
