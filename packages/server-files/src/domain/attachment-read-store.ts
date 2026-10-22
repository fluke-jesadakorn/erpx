import type {FileObject,FileObjectMetadata} from '@erpx/server-files/domain/file-object';
export interface AttachmentReadStore{head(key:string):Promise<FileObjectMetadata|null>;get(key:string):Promise<FileObject|null>;list(options?:{prefix?:string;cursor?:string;limit?:number}):Promise<{objects:FileObjectMetadata[];truncated:boolean;cursor?:string}>;}
