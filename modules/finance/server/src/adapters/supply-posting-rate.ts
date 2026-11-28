import "server-only";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {supplyPostingRateSchema} from "@erpx/finance-server/domain/supply-posting-prerequisites";
import {withSupplyPostingPrerequisiteCommand} from "@erpx/finance-server/adapters/supply-posting-prerequisites";
import {confirmPostingRateInTransaction} from '@erpx/finance-server/adapters/company-money';

export async function confirmSupplyPostingRate(actor:WorkContext,raw:unknown,key:string){
 const request=supplyPostingRateSchema.parse(raw);
 return withSupplyPostingPrerequisiteCommand(actor,'rate',request,key,async tx=>{
  const result=await confirmPostingRateInTransaction(tx,actor,request.input,key,request);
  if(typeof result.id!=='string'||typeof result.currency!=='string'||typeof result.effectiveDate!=='string'||typeof result.currencyPerUsd!=='string'||typeof result.confirmedAt!=='number')throw new ApiError(409,'COMMERCE_POSTING_SOURCE_CHANGED','The original manual-rate receipt is incomplete.');
  return {id:result.id,currency:result.currency,effectiveDate:result.effectiveDate,currencyPerUsd:result.currencyPerUsd,confirmedAt:result.confirmedAt};
 });
}
