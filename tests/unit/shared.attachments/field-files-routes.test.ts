import {describe,it,expect} from 'vitest';
import {fieldFilesRoute,financePaymentFilesRoute,financeJournalFilesRoute} from '../../../packages/native-sdk/src/field-service-files-routes';
describe('private Field Files routes',()=>{
 it('constructs the agreed endpoints with encoded identifiers',()=>{
  expect(fieldFilesRoute('acme',{kind:'reserve',setId:'set 1'})).toEqual({method:'POST',path:'/api/v1/holdings/acme/field-service/attachment-sets/set%201/files'});
  const longId='link-'+'x'.repeat(215);expect(fieldFilesRoute('acme',{kind:'status',linkId:longId}).path).toBe(`/api/v1/holdings/acme/field-service/attachment-links/${longId}/status`);
  expect(fieldFilesRoute('acme',{kind:'upload',intentId:'i'})).toEqual({method:'PUT',path:'/api/v1/holdings/acme/field-service/uploads/i/content'});
  expect(fieldFilesRoute('acme',{kind:'content',linkId:'l',variant:'preview'})).toEqual({method:'GET',path:'/api/v1/holdings/acme/field-service/attachment-links/l/content?variant=preview'});
 });
 it('rejects unsafe route segments and query fragments',()=>{
  expect(()=>fieldFilesRoute('acme',{kind:'status',linkId:'../bad'})).toThrow();
  expect(()=>fieldFilesRoute('acme',{kind:'list',query:'?x=1#fragment'})).toThrow();
 });
});

it('keeps Finance payment Files in its finite namespace',()=>{
 expect(financePaymentFilesRoute('acme',{kind:'create'})).toEqual({method:'POST',path:'/api/v1/holdings/acme/finance/payment-files/attachment-sets'});
 expect(financePaymentFilesRoute('acme',{kind:'reserve',setId:'s'}).path).toBe('/api/v1/holdings/acme/finance/payment-files/attachment-sets/s/files');
 expect(financePaymentFilesRoute('acme',{kind:'withdraw',linkId:'l 1'})).toEqual({method:'POST',path:'/api/v1/holdings/acme/finance/payment-files/attachment-links/l%201/withdraw'});
 expect(()=>financePaymentFilesRoute('acme',{kind:'withdraw',linkId:'../bad'})).toThrow();
 expect(()=>financePaymentFilesRoute('bad/slug',{kind:'withdraw',linkId:'ok'})).toThrow();
});

it('encodes journal supporting-evidence paths without permitting a payment bind',()=>{
 expect(financeJournalFilesRoute('acme',{kind:'bind',setId:'set 1'})).toEqual({method:'POST',path:'/api/v1/holdings/acme/finance/journal-files/attachment-sets/set%201/bind'});
 expect(financeJournalFilesRoute('acme',{kind:'upload',intentId:'intent'}).path).toBe('/api/v1/holdings/acme/finance/journal-files/uploads/intent/content');
 expect(()=>financeJournalFilesRoute('acme',{kind:'bind',setId:'../bad'})).toThrow();
});
