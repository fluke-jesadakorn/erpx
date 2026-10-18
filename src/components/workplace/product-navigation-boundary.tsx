"use client";
import {isProductHandoffHref} from "@/lib/work_projection/product-navigation";
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { hasUnsavedWork, setContextSwitching } from "@/lib/work_projection/context-switch";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import type { UiAppearance } from "@/components/ui/appearance";

/** Server-rendered links; only dirty-state/focus interaction belongs on the client. */
export function ProductNavigationBoundary({ children, language = "en", appearance = "standard" }: { children: ReactNode; language?: "en" | "th"; appearance?: UiAppearance }) {
  const router = useRouter();
  const [destination, setDestination] = useState<string | null>(null);
  const trigger = useRef<HTMLAnchorElement | null>(null);
  const th = language === "th";
  useEffect(() => {
    // No mount read: server content is already authoritative for this render.
    const revalidate = () => { if (document.visibilityState === "visible") router.refresh(); };
    const restored = (event: PageTransitionEvent) => {
      setContextSwitching(false);
      // An initial pageshow already has its server snapshot. Only restoring a
      // cached document needs the extra authority refresh.
      if (event.persisted) revalidate();
    };
    const requested=(event:Event)=>{const href=(event as CustomEvent<{href?:unknown}>).detail?.href;if(typeof href!=="string"||!isProductHandoffHref(href))return;
      trigger.current=document.activeElement instanceof HTMLAnchorElement?document.activeElement:null;
      if(hasUnsavedWork())setDestination(href);else window.location.assign(href);
    };
    window.addEventListener("erpx:product-navigate",requested);
    window.addEventListener("focus", revalidate);
    window.addEventListener("pageshow", restored);
    return () => { window.removeEventListener("erpx:product-navigate", requested); window.removeEventListener("focus", revalidate); window.removeEventListener("pageshow", restored); };
  }, [router]);
  return <div onClickCapture={event => {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const target = event.target as HTMLElement;
    const anchor = target.closest<HTMLAnchorElement>("a[data-product-switch]");
    if (!anchor || anchor.target && anchor.target !== "_self" || !hasUnsavedWork()) return;
    event.preventDefault(); event.stopPropagation();
    trigger.current = anchor; setDestination(anchor.href);
  }}>{children}
    <Dialog appearance={appearance} open={destination !== null} onClose={() => setDestination(null)}
      title={th ? "ออกจากหน้าที่มีงานค้าง?" : "Leave unsaved work?"} finalFocus={() => trigger.current}>
      <p>{th ? "ข้อมูลที่แก้ไขในหน้านี้ยังไม่ได้บันทึก หากออกจากหน้าจะสูญเสียข้อมูลเหล่านี้" : "Your changes on this page have not been saved. Leaving will discard them."}</p>
      <div className="ui-form-actions">
        <Button appearance={appearance} data-autofocus onClick={() => setDestination(null)}>{th ? "แก้ไขต่อ" : "Keep editing"}</Button>
        <Button appearance={appearance} variant="secondary" onClick={() => {
          if (!destination) return;
          const href = destination;
          setContextSwitching(true); setDestination(null); window.location.assign(href);
        }}>{th ? "ทิ้งข้อมูลและออกจากหน้า" : "Discard and leave"}</Button>
      </div>
    </Dialog>
  </div>;
}
