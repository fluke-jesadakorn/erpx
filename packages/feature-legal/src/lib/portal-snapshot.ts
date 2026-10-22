export type LegalPortalSnapshot={shareId:string;kind:string;doc:{name:string;body:string;version:number;checksum:string};comments:{body:string;created_at:number}[];billing:{id:string;title:string;state:string;data:Record<string,string>}[];envelopes:{id:string;state:string;consent:string;signer_state:string;expires_at:number}[];signingEnabled:boolean;purchase?:{settleExisting:boolean;history:boolean;revision:string}};
const record=(value:unknown)=>value&&typeof value==='object'?value as Record<string,unknown>:{};
const rows=(value:unknown)=>Array.isArray(value)?value.map(record):[];
/** Exact shared-document presentation fields only. No Company grants, actor
 * session, internal comments or source file paths cross this boundary. */
export function legalPortalSnapshot(value:unknown):LegalPortalSnapshot{
 const source=record(value),doc=record(source.doc),purchase=record(source.purchase);
 if(typeof source.shareId!=='string'||typeof doc.name!=='string'||typeof doc.checksum!=='string'||!Number.isSafeInteger(doc.version))throw Error('Shared Legal document is unavailable.');
 return {shareId:source.shareId,kind:String(source.kind??''),doc:{name:doc.name,body:typeof doc.body==='string'?doc.body:'',version:Number(doc.version),checksum:doc.checksum},comments:rows(source.comments).map(row=>({body:String(row.body??''),created_at:Number(row.created_at)})),billing:rows(source.billing).map(row=>({id:String(row.id),title:String(row.title),state:String(row.state),data:Object.fromEntries(Object.entries(record(row.data)).filter(([key])=>['minutes','amount'].includes(key)).map(([key,value])=>[key,String(value??'')]))})),envelopes:rows(source.envelopes).map(row=>({id:String(row.id),state:String(row.state),consent:String(row.consent),signer_state:String(row.signer_state),expires_at:Number(row.expires_at)})),signingEnabled:source.signingEnabled===true,...(source.purchase?{purchase:{settleExisting:purchase.settleExisting===true,history:purchase.history===true,revision:String(purchase.revision??'')}}:{})};
}
export function legalPortalSnapshotMatches(initial:LegalPortalSnapshot|null|undefined,id:string|undefined){return !!initial&&initial.shareId===id;}
