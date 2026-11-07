/** Revoke visible private content immediately when rendered authority changes. */
export function watchFieldContentContext(target: Pick<EventTarget,'addEventListener'|'removeEventListener'>, invalidate: () => void) {
 const listener=()=>invalidate();
 target.addEventListener('erpx:context-switch-started',listener);
 target.addEventListener('erpx:work-context-changed',listener);
 return ()=>{target.removeEventListener('erpx:context-switch-started',listener);target.removeEventListener('erpx:work-context-changed',listener);};
}
