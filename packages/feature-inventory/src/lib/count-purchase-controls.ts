export type CountPurchaseControls = {newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export function countCommandPurpose(path:string):"new-work"|"settle-existing"|null {
  if (path === "" || /^\/[^/]+$/.test(path) || /^\/[^/]+\/submit$/.test(path)) return "new-work";
  return /^\/[^/]+\/(?:approve|reject|post|cancel)$/.test(path) ? "settle-existing" : null;
}
export function countCommandAllowed(purchase:CountPurchaseControls|undefined,path:string) {
  const purpose=countCommandPurpose(path);return !!purpose && (!purchase || (purpose === "new-work" ? purchase.newWork : purchase.settleExisting));
}
