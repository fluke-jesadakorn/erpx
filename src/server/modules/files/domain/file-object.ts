export type FileObjectMetadata={key:string;size:number;etag:string;httpEtag:string;uploaded:Date;httpMetadata:{contentType?:string;cacheControl?:string;contentDisposition?:string};customMetadata:Record<string,string>};
export type FileObject=FileObjectMetadata&{body:ReadableStream<Uint8Array>;range?:{offset:number;length:number};arrayBuffer():Promise<ArrayBuffer>;text():Promise<string>};
export interface AttachmentByteStore<Context>{
readonly supportsTransactionDeletion?:boolean;
put(key:string,value:string|ArrayBuffer|ArrayBufferView|ReadableStream<Uint8Array>|Blob,options?:{onlyIf?:{etagDoesNotMatch:string};httpMetadata?:FileObjectMetadata['httpMetadata'];customMetadata?:Record<string,string>}):Promise<FileObjectMetadata|null>;
get(key:string,options?:{range?:{offset?:number;length?:number;suffix?:number}}):Promise<FileObject|null>;
head(key:string):Promise<FileObjectMetadata|null>;
delete(key:string|string[],authorization?:{unit:Context}):Promise<void>;
}
