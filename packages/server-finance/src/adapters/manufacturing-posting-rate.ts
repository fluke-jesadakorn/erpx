import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {ApiError} from "@erpx/server-runtime/public/http";
import {manufacturingPostingRateSchema} from "@erpx/server-finance/domain/manufacturing-posting-prerequisites";
import {withManufacturingPostingPrerequisiteCommand} from "@erpx/server-finance/adapters/manufacturing-posting-prerequisites";
import {confirmPostingRateInTransaction} from '@erpx/server-finance/adapters/company-money';

export async function confirmManufacturingPostingRate(actor:WorkContext,raw:unknown,key:string){const request=manufacturingPostingRateSchema.parse(raw);return withManufacturingPostingPrerequisiteCommand(actor,'rate',request,key,async tx=>{const result=await confirmPostingRateInTransaction(tx,actor,request.input,key,request);if(typeof result.id!=='string'||typeof result.currency!=='string'||typeof result.effectiveDate!=='string'||typeof result.currencyPerUsd!=='string'||typeof result.confirmedAt!=='number')throw new ApiError(409,'MANUFACTURING_POSTING_SOURCE_CHANGED','The original manual-rate receipt is incomplete.');return {id:result.id,currency:result.currency,effectiveDate:result.effectiveDate,currencyPerUsd:result.currencyPerUsd,confirmedAt:result.confirmedAt};});}
