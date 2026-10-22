export type FileMetadata = {
  key: string;
  size: number;
  etag: string;
  httpEtag: string;
  uploaded: Date;
  httpMetadata: { contentType?: string; cacheControl?: string; contentDisposition?: string };
  customMetadata: Record<string, string>;
};
export type StoredFile = FileMetadata & {
  body: ReadableStream<Uint8Array>;
  range?: { offset: number; length: number };
  arrayBuffer(): Promise<ArrayBuffer>;
  text(): Promise<string>;
};
export interface StreamedFileContent {
 put(key:string,value:string|ArrayBuffer|ArrayBufferView|ReadableStream<Uint8Array>|Blob,options?:{onlyIf?:{etagDoesNotMatch:string};httpMetadata?:FileMetadata['httpMetadata'];customMetadata?:Record<string,string>}):Promise<FileMetadata|null>;
 get(key:string,options?:{range?:{offset?:number;length?:number;suffix?:number}}):Promise<StoredFile|null>;
}
