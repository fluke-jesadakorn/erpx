"use client";
import { Suspense, useEffect, useRef, useState, type ReactNode } from "react";

/** Children (including lazy imports and their requests) mount once, near the viewport. */
export function DeferredRegion({ children, label, minHeight = 180 }: {
  children: ReactNode; label: string; minHeight?: number;
}) {
  const target = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(false);
  useEffect(() => {
    if (typeof IntersectionObserver === "undefined") {
      const timer = setTimeout(() => setActive(true), 0);
      return () => clearTimeout(timer);
    }
    const observer = new IntersectionObserver(entries => {
      if (entries.some(entry => entry.isIntersecting)) { setActive(true); observer.disconnect(); }
    }, { rootMargin: "400px" });
    if (target.current) observer.observe(target.current);
    return () => observer.disconnect();
  }, []);
  const placeholder = <div style={{ minHeight }} aria-label={label} aria-busy={active}>
    <p>{label}</p>
  </div>;
  return <div ref={target} data-deferred-region={label} data-active={active}>
    {active ? <Suspense fallback={placeholder}>{children}</Suspense> : placeholder}
  </div>;
}
