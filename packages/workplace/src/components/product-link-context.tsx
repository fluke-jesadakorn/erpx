"use client";
import { createContext, useContext, useSyncExternalStore, type ComponentProps, type ReactNode } from "react";
import {usePathname} from 'next/navigation';
import Link from "next/link";
import type { ProductHostKey } from "@erpx/server-commercial/public/rules/product-registry";
import type { ProductNavigationScope } from "@erpx/feature-work_projection/lib/product-navigation";
import { WorkspaceUiNavigation } from "@erpx/workplace/components/ui-navigation";

const Context = createContext<ProductNavigationScope & { appsHref?: string; initialPathname?:string }>({ audience: null });

export function ProductLinkEnvironment({ audience, appsHref, holdingSlug, initialPathname, children }: {
  audience: ProductHostKey | null;
  appsHref?: string;
  holdingSlug?: string;
  initialPathname?:string;
  children: ReactNode;
}) {
  return <Context value={{ audience, appsHref, holdingSlug, initialPathname }}><WorkspaceUiNavigation>{children}</WorkspaceUiNavigation></Context>;
}

export function ProductWorkLinks({ companyId, companySlug, workspaceBase, children }: {
  companyId: string;
  companySlug?: string;
  workspaceBase: string;
  children: ReactNode;
}) {
  const parent = useContext(Context);
  return <Context value={{ ...parent, companyId, companySlug, workspaceBase: workspaceBase.replace(/\/+$/, "") }}>{children}</Context>;
}

export function useProductNavigationScope() {
  return useContext(Context);
}
/** Rewrites can expose the owner path during SSR and the public path in the
 * browser. Use the trusted request path for the first render on both sides. */
export function useProductPathname(){
  const pathname=usePathname(),scope=useContext(Context),mounted=useSyncExternalStore(subscribeHydration,clientHydrated,serverHydrated);
  return !mounted&&scope.initialPathname?scope.initialPathname:pathname;
}
const subscribeHydration=()=>()=>{},clientHydrated=()=>true,serverHydrated=()=>false;

/** The server supplies the configured Apps entry, including SSO across hosts.
 * Brand links never infer a domain from the current browser location. */
export function AppsHomeLink(props: Omit<ComponentProps<typeof Link>, "href" | "prefetch">) {
  const { appsHref,holdingSlug } = useContext(Context);
  if(holdingSlug)return <a {...props as ComponentProps<'a'>} href={appsHref??'/'} data-product-switch aria-label={props['aria-label']??'ERPX Apps'}/>;
  return <Link {...props} href={appsHref ?? "/"} prefetch={false} data-product-switch aria-label={props["aria-label"] ?? "ERPX Apps"} />;
}
