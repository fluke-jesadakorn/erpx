export const manufacturingInitialPhysicalStockCountLanding="production/manufacturing/settings/initial-stock";
export type ManufacturingInitialPhysicalStockCountEntry={countId?:string};
/** Pure exact destination only; native owner proof supplies all authority. */
export function manufacturingInitialPhysicalStockCountEntry(destination:string):ManufacturingInitialPhysicalStockCountEntry|null{
 const root=manufacturingInitialPhysicalStockCountLanding,path=destination.startsWith('/')?destination.slice(1):destination;
 if(path===root)return {};
 if(!path.startsWith(`${root}/`))return null;
 let countId:string;try{countId=decodeURIComponent(path.slice(root.length+1));}catch{return null;}
 if(/[\\/\u0000-\u001f\u007f]/.test(countId))return null;
 return /^count-[A-Za-z0-9][A-Za-z0-9._:-]{0,190}$/.test(countId)?{countId}:null;
}
