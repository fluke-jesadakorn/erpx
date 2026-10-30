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
