import type { ProductId } from './product-registry.ts';

const enforcementOwners: Record<ProductId,string> = {
 apps:'APPS',pos:'POS',pms:'PMS',finance:'FINANCE',people:'PEOPLE',sales:'COMMERCE',supply:'COMMERCE',
 assets:'ASSETS',projects:'PROJECTS',recruit:'TALENT',learn:'LEARNING',support:'SUPPORT',service:'SERVICE',legal:'LEGAL',manufacturing:'MANUFACTURING',
};

/** Preserve existing module-authorized development entry at Console cutover.
 * This only defers Commercial admission. It supplies no grants, modules,
 * permissions, readiness or acting-role selection. Owners authorize normally. */
export function developmentModuleAdmissionDeferred(id:ProductId,environment:Readonly<Record<string,string|undefined>>):boolean {
 return Object.hasOwn(enforcementOwners,id)&&environment.ERPX_CONSOLE_RETIRED==='enabled'
  &&environment.ERPX_ENVIRONMENT==='local'&&environment.ERPX_PRODUCT_ENVIRONMENT==='development'
  &&/^https:\/\/console-dev-erpx\.jesadakorn\.com\/?$/.test(environment.ERPX_PUBLIC_ORIGIN??'')
  &&environment.ERPX_PRODUCT_ROUTING==='enabled'&&environment.ERPX_PRODUCT_SSO==='enabled'
  &&(environment[`ERPX_${enforcementOwners[id]}_PACKAGE_ENFORCEMENT`]??'disabled')==='disabled';
}
