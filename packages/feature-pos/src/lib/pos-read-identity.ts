/** Server seed identity stays internal; API response shapes and URLs do not change. */
export type PosInitialRead = { sale?: string; q?: string; tab?: string; cursor?: string; finance?: boolean };
export function posReadKey(base: string, scope: string, input: PosInitialRead): string {
  return JSON.stringify([base, scope, input.sale ?? "", input.q ?? "", input.tab ?? "sale", input.cursor ?? "", Boolean(input.finance)]);
}
export function isLivePosTab(tab: string): boolean { return ["sale", "kitchen", "shifts"].includes(tab); }
/** Strict Mode may repeat the initial effect. Returning to a filter later must reload. */
export function createPosSeedReadGate(initialKey:string|null) {
  let leftInitial=false;
  return {shouldRead(key:string){
    if(!leftInitial && key===initialKey)return false;
    leftInitial=true;
    return true;
  }};
}
