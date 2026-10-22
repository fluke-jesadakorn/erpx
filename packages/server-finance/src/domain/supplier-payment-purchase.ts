export type SupplierPaymentSourcePurchase={capability:"finance.inventory-valuation";settleExisting:boolean;history:boolean;revision:string};
/** Retained Banking visibility and actual Supply settlement are independent.
 * Generic Banking collections never inherit a source mutation flag. */
export function supplierPaymentPresentation(bankHistory:boolean,source:{settleExisting:boolean;history:boolean;revision:string},actualTarget:boolean):{sourceOnly:boolean;purchase?:SupplierPaymentSourcePurchase}{
 const sourceOnly=!bankHistory;
 return {sourceOnly,...(source.history&&(sourceOnly||actualTarget)?{purchase:{capability:"finance.inventory-valuation" as const,settleExisting:source.settleExisting,history:source.history,revision:source.revision}}:{})};
}
