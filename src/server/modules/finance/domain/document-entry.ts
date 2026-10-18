/** Exact retained document destinations only. A collection/source label is
 * never a substitute for the original owner's record identity. */
export type FinanceDocumentEntry = {kind:"invoice"|"journal"|"payment"|"refund"|"supplier-batch";id:string};
export function financeDocumentEntry(path:string):FinanceDocumentEntry|null {
 const match=/^finance\/(documents\/invoices|accounting\/journals|cash\/payments|cash\/refunds|cash\/supplier-batches)\/([^/]+)(?:\/overview)?$/.exec(path);
 if(!match||!match[2]||/[\\\/?#\u0000-\u001f]/.test(match[2])||[".",".."].includes(match[2]))return null;
 return {kind:match[1]==="documents/invoices"?"invoice":match[1]==="accounting/journals"?"journal":match[1]==="cash/refunds"?"refund":match[1]==="cash/supplier-batches"?"supplier-batch":"payment",id:match[2]};
}

/** Customer receipts retain their own immutable owner identity, separate from payment documents. */
export function salesCustomerReceiptEntry(path:string):{id:string}|null {
 const match=/^finance\/documents\/sales\/receipts\/([^/]+)$/.exec(path);
 if(!match||/[\\\/?#%\u0000-\u001f]/.test(match[1])||[".",".."].includes(match[1]))return null;
 return {id:match[1]};
}
