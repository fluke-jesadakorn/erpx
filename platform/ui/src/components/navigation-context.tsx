"use client";

import NextLink from "next/link";
import { createContext, useContext, useMemo, type ComponentProps, type ComponentType, type ReactElement, type ReactNode } from "react";

export type UiLinkProps = ComponentProps<typeof NextLink>;
export type UiFilterValues = Record<string, string | string[]>;
export type TableActionElement = ReactElement<{ children?: ReactNode; colSpan?: number; href?: unknown; record?: unknown; kind?: unknown; id?: unknown; "data-record-link"?: string }>;
export type UiNavigationBindings = {
  Link: ComponentType<UiLinkProps>;
  isTableAction: (element: TableActionElement, scope: string) => boolean;
  resolveFilters: (pathname: string, action: string, preserve: UiFilterValues) => { action: string; preserve: UiFilterValues };
  filterHref: (action: string, preserve: UiFilterValues) => string;
  filterNavigation?: { supports: (pathname: string) => boolean; href: (href: string) => string; push: (href: string) => void };
};

const defaults: UiNavigationBindings = {
  Link: NextLink,
  isTableAction: element => !!element.props.record || element.props["data-record-link"] === "true",
  resolveFilters: (_pathname, action, preserve) => ({ action, preserve }),
  filterHref: (action, preserve) => {
    const query = new URLSearchParams();
    for (const [name, value] of Object.entries(preserve)) for (const item of Array.isArray(value) ? value : [value]) query.append(name, item);
    return `${action}${query.size ? `?${query}` : ""}`;
  },
};
const Context = createContext<UiNavigationBindings>(defaults);

/** Client composition roots supply navigation policy; primitives retain native defaults. */
export function UiNavigationProvider({ bindings, children }: { bindings: Partial<UiNavigationBindings>; children: ReactNode }) {
  const parent = useContext(Context);
  const value = useMemo(() => ({ ...parent, ...bindings }), [parent, bindings]);
  return <Context value={value}>{children}</Context>;
}

export function useUiNavigation() { return useContext(Context); }

export function NavigationLink(props: UiLinkProps) {
  const { Link } = useUiNavigation();
  return <Link {...props}/>;
}
