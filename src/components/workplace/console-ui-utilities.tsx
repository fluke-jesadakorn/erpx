"use client";
import { usePathname } from "next/navigation";
import type { ComponentProps } from "react";
import { AccountSwitcher } from "@/app/holdings/[holdingSlug]/account-switcher";
import { CurrencySelector } from "@/components/finance/money/currency-provider";
import { WorkplaceNavigation, CommandSearch } from "./workplace-controls";
import type { UiAppearance } from "@/components/ui/appearance";

/** Explicit caller policy: specialized workspaces keep the original utility UI. */
function useConsoleAppearance(): UiAppearance {
  const pathname = usePathname();
  const specialized = /\/(?:pms|pos|hospitality|calendar|executive|planning)(?:\/|$)|\/people(?:$|\/(?:organization|move|roster)(?:\/|$))|\/technology(?:\/|$)/.test(pathname);
  return specialized ? "legacy" : "standard";
}
export function ConsoleAccountSwitcher(props: ComponentProps<typeof AccountSwitcher>) {
  return <AccountSwitcher {...props} appearance={useConsoleAppearance()} />;
}
export function ConsoleCurrencySelector() {
  return <CurrencySelector appearance={useConsoleAppearance()} />;
}

export function ConsoleWorkplaceNavigation(props: ComponentProps<typeof WorkplaceNavigation>) {
 return <WorkplaceNavigation {...props} appearance={useConsoleAppearance()} />;
}
export function ConsoleCommandSearch(props: ComponentProps<typeof CommandSearch>) {
 return <CommandSearch {...props} appearance={useConsoleAppearance()} />;
}
