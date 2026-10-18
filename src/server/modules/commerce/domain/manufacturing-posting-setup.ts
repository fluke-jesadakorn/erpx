import {salesGoodsPostingSetupSchema} from './sales-goods-posting-setup';
import type {SalesGoodsPostingSetupInput,SalesGoodsPostingSetupResult,SalesGoodsPostingSetupView} from './sales-goods-posting-setup';
/** The same original nine real accounts and explicit approved-default edition.
 * No source, entitlement or account role is selected by caller metadata. */
export const manufacturingPostingSetupSchema=salesGoodsPostingSetupSchema;
export type ManufacturingPostingSetupInput=SalesGoodsPostingSetupInput;
export type ManufacturingPostingSetupResult=SalesGoodsPostingSetupResult;
export type ManufacturingPostingSetupView=Omit<SalesGoodsPostingSetupView,'unavailableReason'> & {
 unsupportedExistingMapping:boolean;
 unavailableReason:'authority-required'|'purchase-required'|'history-requires-review'|'sales-defaults-required'|null;
};
