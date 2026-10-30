import { describe, expect, it, vi } from 'vitest';
import { validateFieldAttachment } from '../../../packages/feature-files/src/lib/attachment-client';
import { watchFieldContentContext } from '../../../packages/feature-files/src/application/field-content-lifetime';
import { createFieldUploadAttempt } from '../../../packages/feature-files/src/application/field-upload-attempt';

describe('Field upload attempt', () => {
  it('retains exact reservation identity and bytes after a lost reply and a later rejection', async () => {
    const file = { bytes: 'original' }, context = { identity: 'account/company/role', generation: 1, valid: true };
    const receipt = { uploadIntentId: 'intent', attachmentLinkId: 'link', uploadUrl: '/upload', expiresAt: 100 };
    const reserve = vi.fn().mockRejectedValueOnce(Error('lost')).mockRejectedValueOnce(Error('revoked')).mockResolvedValue(receipt);
    const upload = vi.fn().mockResolvedValue({ uploadIntentId: 'intent', attachmentLinkId: 'link', sourceId: 'source', state: 'quarantined', status: 'scan_pending' });
    const commands = createFieldUploadAttempt({ context: () => context, terminalRejection:()=>false, key: () => 'original-key', reserve, upload });
    await expect(commands.submit({ scope: 'one', base: '/work', setId: 'set', body: '{"filename":"a"}', file, mediaType: 'image/png' })).rejects.toThrow('lost');
    await expect(commands.retry('one')).rejects.toThrow('revoked');
    await expect(commands.submit({ scope: 'one', base: '/work', setId: 'set', body: '{}', file, mediaType: 'image/png' })).rejects.toThrow('original upload');
    await commands.retry('one');
    expect(reserve.mock.calls.map(([a]) => [a.key, a.body, a.file])).toEqual(Array(3).fill(['original-key', '{"filename":"a"}', file]));
    expect(upload).toHaveBeenCalledOnce(); expect(commands.pending('one')).toBeUndefined();
  });
  it('retains the reservation on a lost upload and refuses a changed context', async () => {
    let context = { identity: 'one', generation: 1, valid: true };
    const reserve = vi.fn().mockResolvedValue({ uploadIntentId: 'i', attachmentLinkId: 'l', uploadUrl: '/u', expiresAt: 5 });
    const upload = vi.fn().mockRejectedValueOnce(Error('lost')).mockResolvedValue({ uploadIntentId: 'i', attachmentLinkId: 'l', sourceId: 's', state: 'quarantined', status: 'scan_pending' });
    const commands = createFieldUploadAttempt({ context: () => context, terminalRejection:()=>false, key: () => 'k', reserve, upload });
    await expect(commands.submit({ scope: 's', base: '/work', setId: 'set', body: '{}', file: 'bytes', mediaType: 'image/png' })).rejects.toThrow('lost');
    context = { identity: 'other', generation: 2, valid: true }; await expect(commands.retry('s')).rejects.toThrow('original account');
    context = { identity: 'one', generation: 1, valid: true }; await commands.retry('s');
    expect(reserve).toHaveBeenCalledOnce(); expect(upload).toHaveBeenCalledTimes(2);
  });
  it('does not accept an unrelated or malformed acknowledgement', async () => {
    const commands = createFieldUploadAttempt({ context: () => ({ identity: 'one', generation: 1, valid: true }), terminalRejection:()=>false, key: () => 'k', reserve: async () => ({ uploadIntentId: 'i', attachmentLinkId: 'l', uploadUrl: '/u', expiresAt: 5 }), upload: async () => ({ uploadIntentId: 'other', attachmentLinkId: 'l', sourceId: 's', state: 'available', status: 'scan_pending' }) });
    await expect(commands.submit({ scope: 's', base: '/work', setId: 'set', body: '{}', file: 'bytes', mediaType: 'image/png' })).rejects.toThrow('confirm');
    expect(commands.pending('s')?.reservation?.uploadIntentId).toBe('i');
  });
  it('invalidates visible content on context changes and unregisters cleanly',()=>{
    const target=new EventTarget(),revoke=vi.fn(),unwatch=watchFieldContentContext(target,revoke);
    target.dispatchEvent(new Event('erpx:context-switch-started'));target.dispatchEvent(new Event('erpx:work-context-changed'));
    expect(revoke).toHaveBeenCalledTimes(2);unwatch();target.dispatchEvent(new Event('erpx:work-context-changed'));expect(revoke).toHaveBeenCalledTimes(2);
  });
  it('releases a definite first rejection but preserves an uncertain attempt after later rejection',async()=>{
    const context=()=>({identity:'one',generation:1,valid:true}),terminalRejection=(error:unknown)=>error instanceof Error&&error.message==='invalid';
    const reserve=vi.fn().mockRejectedValueOnce(Error('invalid')).mockRejectedValueOnce(Error('lost')).mockRejectedValueOnce(Error('invalid'));
    const commands=createFieldUploadAttempt({context,terminalRejection,key:()=> 'original',reserve,upload:async()=>({})});
    const input={scope:'s',base:'/work',setId:'set',body:'{}',file:'bytes',mediaType:'image/png'};
    await expect(commands.submit(input)).rejects.toThrow('invalid');expect(commands.pending('s')).toBeUndefined();
    await expect(commands.submit(input)).rejects.toThrow('lost');await expect(commands.retry('s')).rejects.toThrow('invalid');expect(commands.pending('s')?.key).toBe('original');
  });
  it('rejects PDFs for site photos while retaining other evidence types',()=>{
    const pdf={name:'report.pdf',type:'application/pdf',size:12};
    expect(validateFieldAttachment(pdf,'site_photo')).toMatch(/image/);expect(validateFieldAttachment(pdf,'technician_report')).toBeNull();
  });
  it('releases a definite invalid upload and retains the staged reservation for later refresh',async()=>{
    const reserve=vi.fn().mockResolvedValue({uploadIntentId:'i',attachmentLinkId:'l',uploadUrl:'/u',expiresAt:5});
    const commands=createFieldUploadAttempt({context:()=>({identity:'one',generation:1,valid:true}),terminalRejection:e=>e instanceof Error&&e.message==='invalid bytes',key:()=> 'k',reserve,upload:async()=>{throw Error('invalid bytes');}});
    await expect(commands.submit({scope:'s',base:'/work',setId:'set',body:'{}',file:'bad',mediaType:'image/png'})).rejects.toThrow('invalid bytes');expect(commands.pending('s')).toBeUndefined();
  });
  it('recaptures only the same valid identity on explicit retry after a failed switch',async()=>{
    let context={identity:'account/company/assignment/version/revision',generation:1,valid:true};const file={bytes:'original'};
    const reserve=vi.fn().mockRejectedValueOnce(Error('lost')).mockResolvedValue({uploadIntentId:'i',attachmentLinkId:'l',uploadUrl:'/u',expiresAt:5});
    const upload=vi.fn().mockResolvedValue({uploadIntentId:'i',attachmentLinkId:'l',sourceId:'s',state:'quarantined',status:'scan_pending'});
    const commands=createFieldUploadAttempt({context:()=>context,key:()=> 'same-key',terminalRejection:()=>false,reserve,upload});
    await expect(commands.submit({scope:'s',base:'/work',setId:'set',body:'original',file,mediaType:'image/png'})).rejects.toThrow('lost');
    context={...context,generation:2,valid:false};await expect(commands.retry('s')).rejects.toThrow('original account');
    context={...context,identity:'different company',valid:true};await expect(commands.retry('s')).rejects.toThrow('original account');
    context={...context,identity:'account/company/assignment/version/revision'};await commands.retry('s');
    expect(reserve.mock.calls[1][0]).toMatchObject({key:'same-key',body:'original',file,context:{generation:2}});
  });
  it('blocks a late response from the old epoch before a retry can recapture',async()=>{
    let context={identity:'original',generation:1,valid:true};let release!:(value:unknown)=>void;
    const reserve=vi.fn().mockImplementationOnce(()=>new Promise(resolve=>{release=resolve;})).mockResolvedValue({uploadIntentId:'i',attachmentLinkId:'l',uploadUrl:'/u',expiresAt:5});
    const commands=createFieldUploadAttempt({context:()=>context,key:()=> 'same-key',terminalRejection:()=>false,reserve,upload:async()=>({uploadIntentId:'i',attachmentLinkId:'l',sourceId:'s',state:'quarantined',status:'scan_pending'})});
    const first=commands.submit({scope:'s',base:'/work',setId:'set',body:'body',file:'bytes',mediaType:'image/png'});context={...context,generation:2};
    await expect(commands.retry('s')).rejects.toThrow('Wait');release({uploadIntentId:'old',attachmentLinkId:'old',uploadUrl:'/old',expiresAt:5});await expect(first).rejects.toThrow('original account');
    expect(commands.pending('s')?.reservation).toBeUndefined();await commands.retry('s');expect(reserve).toHaveBeenCalledTimes(2);
  });
});
