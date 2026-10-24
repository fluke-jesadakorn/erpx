import type{SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import{ApiError}from '@erpx/server-runtime/public/http';
type Unit={companyId:string;checks:(()=>Promise<void>)[];attachmentReadPreparing?:boolean};
export const units=new WeakMap<SqlDatabase,Unit>();
export function requireLegalAttachmentReadPreparation(database:SqlDatabase,companyId:string){
 const unit=units.get(database);if(unit?.companyId!==companyId||!unit.attachmentReadPreparing)throw new ApiError(503,'LEGAL_ATTACHMENT_READ_PREPARATION_REQUIRED','Task evidence requires the original fresh Legal preservation constructor.');
}
export function deferLegalAttachmentReadCheck(database:SqlDatabase,companyId:string,check:()=>Promise<void>){
 const unit=units.get(database);if(unit?.companyId!==companyId||unit.attachmentReadPreparing)throw new ApiError(503,'LEGAL_ATTACHMENT_READ_PARENT_REQUIRED','The original complete Legal preservation transaction is required.');
 unit.checks.push(check);
}
