/** Private Field Files paths. IDs are path segments, never arbitrary URLs. */
export type FieldFilesRoute =
 | {kind:'list';query:string}
 | {kind:'create'}
 | {kind:'reserve';setId:string}
 | {kind:'upload';intentId:string}
 | {kind:'status';linkId:string}
 | {kind:'content';linkId:string;variant:'original'|'preview'};
const id=(value:string)=>{if(!value || /[\u0000-\u001f\u007f/\\]/.test(value))throw Error('NATIVE_FIELD_FILES_ROUTE_INVALID');return encodeURIComponent(value);};
export function fieldFilesRoute(slug:string,route:FieldFilesRoute):{method:'GET'|'POST'|'PUT';path:string}{
 if(!/^[a-z0-9_-]{1,128}$/.test(slug))throw Error('NATIVE_FIELD_FILES_ROUTE_INVALID');
 const base=`/api/v1/holdings/${encodeURIComponent(slug)}/field-service`;
 switch(route.kind){
  case 'list': if(route.query && (!route.query.startsWith('?') || /[#\u0000-\u001f\u007f]/.test(route.query) || new TextEncoder().encode(route.query).length>8192))throw Error('NATIVE_FIELD_FILES_ROUTE_INVALID');return {method:'GET',path:`${base}/attachment-sets${route.query}`};
  case 'create':return {method:'POST',path:`${base}/attachment-sets`};
  case 'reserve':return {method:'POST',path:`${base}/attachment-sets/${id(route.setId)}/files`};
  case 'upload':return {method:'PUT',path:`${base}/uploads/${id(route.intentId)}/content`};
  case 'status':return {method:'GET',path:`${base}/attachment-links/${id(route.linkId)}/status`};
  case 'content':if(route.variant!=='original' && route.variant!=='preview')throw Error('NATIVE_FIELD_FILES_ROUTE_INVALID');return {method:'GET',path:`${base}/attachment-links/${id(route.linkId)}/content?variant=${route.variant}`};
 }
}

export type FinancePaymentFilesRoute=FieldFilesRoute|{kind:'withdraw';linkId:string};
/** Separate finite compiled owner namespace; the server still proves lineage. */
export function financePaymentFilesRoute(slug:string,route:FinancePaymentFilesRoute):{method:'GET'|'POST'|'PUT';path:string}{
 if(route.kind==='withdraw'){
  if(!/^[a-z0-9_-]{1,128}$/.test(slug))throw Error('NATIVE_FIELD_FILES_ROUTE_INVALID');
  return {method:'POST',path:`/api/v1/holdings/${encodeURIComponent(slug)}/finance/payment-files/attachment-links/${id(route.linkId)}/withdraw`};
 }
 const target=fieldFilesRoute(slug,route);
 return {...target,path:target.path.replace('/field-service/','/finance/payment-files/')};
}

export type FinanceJournalFilesRoute=FinancePaymentFilesRoute|{kind:'bind';setId:string};
/** Supporting evidence uses the same compiled Files protocol and a Finance-owned
 * metadata CAS; payment proof deliberately has no standalone bind route. */
export function financeJournalFilesRoute(slug:string,route:FinanceJournalFilesRoute):{method:'GET'|'POST'|'PUT';path:string}{
 if(route.kind==='bind'){
  if(!/^[a-z0-9_-]{1,128}$/.test(slug))throw Error('NATIVE_FIELD_FILES_ROUTE_INVALID');
  return {method:'POST',path:`/api/v1/holdings/${encodeURIComponent(slug)}/finance/journal-files/attachment-sets/${id(route.setId)}/bind`};
 }
 const target=financePaymentFilesRoute(slug,route);
 return {...target,path:target.path.replace('/finance/payment-files/','/finance/journal-files/')};
}
