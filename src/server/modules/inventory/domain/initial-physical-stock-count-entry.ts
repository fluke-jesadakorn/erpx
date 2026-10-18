export const initialPhysicalStockCountLanding='customers-sales/sales/settings/initial-stock';
const root=initialPhysicalStockCountLanding;
/** Pure exact source destination parser; record policy is always re-read by
 * the native owner. A route string never supplies acting authority. */
export type InitialPhysicalStockCountEntry={countId?:string};
export function initialPhysicalStockCountEntry(destination:string):InitialPhysicalStockCountEntry|null{
 const path=destination.startsWith('/')?destination.slice(1):destination;
 if(path===root)return {};
 if(!path.startsWith(`${root}/`))return null;
 let countId:string;try{countId=decodeURIComponent(path.slice(root.length+1));}catch{return null;}
 if(/[\\/\u0000-\u001f\u007f]/.test(countId))return null;
 return /^count-[A-Za-z0-9][A-Za-z0-9._:-]{0,190}$/.test(countId)?{countId}:null;
}
