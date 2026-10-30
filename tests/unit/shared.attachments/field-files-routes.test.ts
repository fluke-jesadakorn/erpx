import {describe,it,expect} from 'vitest';
import {fieldFilesRoute} from '../../../packages/native-sdk/src/field-service-files-routes';
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
