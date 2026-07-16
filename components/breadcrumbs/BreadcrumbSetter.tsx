'use client';

import { useEffect, useRef } from 'react';
import type { ReactNode } from 'react';
import type { Crumb } from '../breadcrumbs';

interface Props {
  crumbs: Crumb[];
}

const subscribers = new Set<(c: Crumb[]) => void>();
let current: Crumb[] = [];

export function setBreadcrumbs(c: Crumb[]) {
  current = Array.isArray(c) ? c : [];
  subscribers.forEach((fn) => fn(current));
}

export function clearBreadcrumbs() {
  current = [];
  subscribers.forEach((fn) => fn(current));
}

export function getBreadcrumbs(): Crumb[] {
  return current;
}

export function BreadcrumbSetter({ crumbs }: Props) {
  const last = useRef<Crumb[] | null>(null);
  useEffect(() => {
    setBreadcrumbs(crumbs);
    last.current = crumbs;
    return () => {
      if (last.current === crumbs) clearBreadcrumbs();
    };
  }, [crumbs]);
  return null;
}

export default BreadcrumbSetter;
