import {expect,vi} from 'vitest';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase,FileStore} from '@erpx/server-runtime/public/runtime/types';
import {seedAttachmentSet} from './attachment-v2';
/** Original native Files outcomes, exercised through the real dispatcher. The
 * helper supplies no authority, audit event, byte source or owner operation. */
export async function retainedUnavailableFileAudits(database:SqlDatabase,files:FileStore,actor:WorkContext,linkId:string,input:Parameters<typeof seedAttachmentSet>[2]){
 const {attachmentContent}=await import('@erpx/server-files/public/attachments/service');
 const missing=vi.spyOn(files,'get').mockResolvedValue(null);
 try{await expect(attachmentContent(database,files,actor,linkId,'original')).rejects.toMatchObject({status:503,code:'FILE_UNAVAILABLE'});expect(missing).toHaveBeenCalledTimes(1);}finally{missing.mockRestore();}
 expect(await database.prepare("SELECT 1 FROM attachment_access_events WHERE company_id=? AND actor_id=? AND link_id=? AND outcome='denied' AND reason='FILE_UNAVAILABLE'").bind(actor.companyId,actor.userId,linkId).first()).not.toBeNull();
 const quarantined=await seedAttachmentSet(database,actor,{...input,sourceState:'quarantined'}),get=vi.spyOn(files,'get');
 try{await expect(attachmentContent(database,files,actor,quarantined.linkId,'original')).rejects.toMatchObject({status:423,code:'ATTACHMENT_NOT_AVAILABLE'});expect(get).not.toHaveBeenCalled();}finally{get.mockRestore();}
 expect(await database.prepare("SELECT 1 FROM attachment_access_events WHERE company_id=? AND actor_id=? AND link_id=? AND outcome='denied' AND reason='ATTACHMENT_NOT_AVAILABLE'").bind(actor.companyId,actor.userId,quarantined.linkId).first()).not.toBeNull();
 expect(await database.prepare("SELECT 1 FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(quarantined.linkId).first()).toBeNull();
}
