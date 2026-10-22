"use client";
import { useCallback, useEffect, useState } from "react";

/** Replay intent through initialOpen after import; keep the editor mounted to retain drafts. */
export function useToolActivation(id: "compose" | "handover" | "plan") {
  const [activated, setActivated] = useState(false);
  const activateOnly = useCallback(() => setActivated(true), []);
  const activate = useCallback(() => {
    setActivated(true);
    window.dispatchEvent(new Event(`erpx:open-${id}`));
  }, [id]);
  useEffect(() => {
    const hash = () => { if (window.location.hash === `#${id}`) activate(); };
    const click = (event: MouseEvent) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      const link = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>(`a[href$="#${id}"]`) : null;
      if (link && new URL(link.href).pathname === window.location.pathname) activate();
    };
    const timer = setTimeout(hash, 0);
    window.addEventListener("hashchange", hash);
    window.addEventListener(`erpx:open-${id}`, activateOnly);
    document.addEventListener("click", click, true);
    return () => { clearTimeout(timer); window.removeEventListener("hashchange", hash);
      window.removeEventListener(`erpx:open-${id}`, activateOnly); document.removeEventListener("click", click, true); };
  }, [id, activate, activateOnly]);
  return { activated, activate };
}
