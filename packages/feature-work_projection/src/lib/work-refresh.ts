let lastChange = 0;
export const lastWorkChange = () => lastChange;
export function announceWorkChange() {
  lastChange = Date.now();
  window.dispatchEvent(new Event("erpx:work-changed"));
}
/** Coalesce focus/visibility/mutation bursts, including development effect replay. */
export function scheduleWorkRefresh(refresh: () => void, events = ["focus", "visibilitychange", "erpx:work-changed"]) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const schedule = () => {
    clearTimeout(timer);
    if (document.visibilityState === "visible") timer = setTimeout(refresh, 80);
  };
  for (const event of events) (event === "visibilitychange" ? document : window).addEventListener(event, schedule);
  return () => {
    clearTimeout(timer);
    for (const event of events) (event === "visibilitychange" ? document : window).removeEventListener(event, schedule);
  };
}
