import {salesGoodsPostingSetupSchema} from '@erpx/server-commerce/domain/sales-goods-posting-setup';
import type {SalesGoodsPostingSetupInput,SalesGoodsPostingSetupResult,SalesGoodsPostingSetupView} from '@erpx/server-commerce/domain/sales-goods-posting-setup';
export const posPostingSetupSchema=salesGoodsPostingSetupSchema;
export type PosPostingSetupInput=SalesGoodsPostingSetupInput;
export type PosPostingSetupResult=SalesGoodsPostingSetupResult;
export type PosPostingSetupView=Omit<SalesGoodsPostingSetupView,'unavailableReason'> & {
 unsupportedExistingMapping:boolean;
 unavailableReason:'authority-required'|'purchase-required'|'history-requires-review'|'sales-defaults-required'|null;
};
