import {firstPageSearchValues,type PageSearchParams} from "@erpx/feature-shared/lib/page-search-params";
import {recordCanonicalHref} from "@erpx/feature-records/lib/record-drawer";

const listFilters=["q","kind","status","queue","cursor","asOf","chartId","tab","startDate","endDate","currency"] as const;
type ListFilter=typeof listFilters[number];
const salesListFilters=["tab","q","status","queue","cursor","startDate","endDate","currency"] as const;

function selectedFilters(values:Record<string,string|undefined>) {
  return Object.fromEntries(listFilters.flatMap((key:ListFilter)=>values[key]===undefined?[]:[[key,values[key]]])) as Partial<Record<ListFilter,string>>;
}

/** Rebuilds the invoice-list context behind an invoice drawer from its same-scope return URL. */
export function invoiceDrawerListSearch(base:string,searchParams:PageSearchParams) {
  const valuesFromPage=firstPageSearchValues(searchParams),returnTo=valuesFromPage.returnTo,
    fallbackSearch=selectedFilters(valuesFromPage);
  if(!returnTo)return fallbackSearch;
  try{
    const url=new URL(returnTo,"https://erpx.invalid"),salesInvoicesPath=`${base}/finance/documents/sales/invoices`,paths=new Map<string,"payable"|"receivable"|undefined>([
      [`${base}/finance/invoices`,undefined],
      [`${base}/finance/documents/invoices`,undefined],
      [`${base}/finance/documents/payable`,"payable"],
      [`${base}/finance/documents/receivable`,"receivable"],
      [salesInvoicesPath,undefined],
    ] as const);
    if(url.origin!=="https://erpx.invalid"||!paths.has(url.pathname))return fallbackSearch;
    const values:Record<string,string|undefined>={};
    for(const key of listFilters){const value=url.searchParams.get(key);if(value!==null)values[key]=value;}
    const aliasKind=paths.get(url.pathname);
    if(aliasKind&&!values.kind)values.kind=aliasKind;
    if(url.pathname===salesInvoicesPath){values.tab="invoices";delete values.kind;}
    return selectedFilters(values);
  }catch{return fallbackSearch;}
}

/** Return to the canonical Sales invoice list after creating from its drawer. */
export function salesInvoiceListReturnTo(base:string,currentHref:string,inline=false) {
  const listPath=`${base}/finance/documents/sales/invoices`,fallback=`${listPath}?tab=invoices`;
  if(inline)return fallback;
  try{
    const current=new URL(currentHref,"https://erpx.invalid"),listPaths=new Set([listPath,`${base}/finance/sales`]);
    if(!listPaths.has(current.pathname))return fallback;
    const query=new URLSearchParams();
    for(const key of salesListFilters)for(const value of current.searchParams.getAll(key))query.append(key,value);
    query.set("tab","invoices");
    return `${listPath}?${query}`;
  }catch{return fallback;}
}

/** Open the created invoice overview as a drawer over its same-scope Sales list. */
export function salesInvoiceDrawerHref(base:string,id:unknown,currentHref:string,inline=false) {
  const returnTo=salesInvoiceListReturnTo(base,currentHref,inline),detail=new URL(recordCanonicalHref(base,{kind:"invoice",id:String(id),args:{section:"overview"}}),"https://erpx.invalid");
  detail.searchParams.set("returnTo",returnTo);
  return detail.pathname+detail.search;
}
