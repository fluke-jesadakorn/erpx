import 'server-only';
import {ApiError} from "@/server/modules/runtime/public/http";
import {taxDocumentCommandSchema} from "@/server/modules/finance/public/hospitality-tax-documents";

/** Both authenticated owner entry points retain the same command and body limits. */
export async function taxDocumentRequest(request:Request){
  const key=request.headers.get('Idempotency-Key');
  if(!key||!/^[a-zA-Z0-9_-]{8,100}$/.test(key))throw new ApiError(422,'COMMAND_KEY_REQUIRED','Use a valid Idempotency-Key.');
  const reader=request.body?.getReader();
  if(!reader)throw new ApiError(422,'INVALID_JSON','Document details are required.');
  let bytes=0,body='';const decoder=new TextDecoder();
  while(true){
    const part=await reader.read();if(part.done)break;
    bytes+=part.value.byteLength;
    if(bytes>4096){await reader.cancel();throw new ApiError(413,'DOCUMENT_TOO_LARGE','Shorten the document details.');}
    body+=decoder.decode(part.value,{stream:true});
  }
  let parsed:unknown;
  try{parsed=JSON.parse(body+decoder.decode());}catch{throw new ApiError(422,'INVALID_JSON','Use valid JSON.');}
  return {command:taxDocumentCommandSchema.parse(parsed),key};
}
