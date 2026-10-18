import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import {ApiError} from "../../runtime/public/http";
import {salesPostingRateSchema} from "../domain/sales-posting-setup";
import {withSalesPostingCommand} from "./sales-posting-setup";
import {confirmPostingRateInTransaction} from "./company-money";

export async function confirmSalesPostingRate(actor:WorkContext,raw:unknown,key:string){
 const request=salesPostingRateSchema.parse(raw);
 return withSalesPostingCommand(actor,'rate',request,key,async tx=>{
  const result=await confirmPostingRateInTransaction(tx,actor,request.input,key,request);
  if(typeof result.id!=='string'||typeof result.currency!=='string'||typeof result.effectiveDate!=='string'||typeof result.currencyPerUsd!=='string'||typeof result.confirmedAt!=='number')throw new ApiError(409,'SALES_POSTING_SOURCE_CHANGED','The original manual-rate receipt is incomplete.');
  return {id:result.id,currency:result.currency,effectiveDate:result.effectiveDate,currencyPerUsd:result.currencyPerUsd,confirmedAt:result.confirmedAt};
 });
}
