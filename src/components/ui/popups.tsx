"use client";

import { Menu as MenuPrimitive } from "@base-ui/react/menu";
import { Popover as PopoverPrimitive } from "@base-ui/react/popover";
import { Combobox as ComboboxPrimitive } from "@base-ui/react/combobox";
import { Tabs as TabsPrimitive } from "@base-ui/react/tabs";
import { Tooltip as TooltipPrimitive } from "@base-ui/react/tooltip";
import { ChevronDown } from "lucide-react";
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/shared/ui";

export const Menu = MenuPrimitive.Root;
export const MenuTrigger = MenuPrimitive.Trigger;
export function MenuContent({ className, ...props }: Omit<ComponentProps<typeof MenuPrimitive.Popup>, "className"> & { className?: string }) {
  return <MenuPrimitive.Portal><MenuPrimitive.Positioner sideOffset={6} align="end" style={{ zIndex: 100 }}><MenuPrimitive.Popup {...props} data-slot="menu" className={cn("ui-popup", className)} /></MenuPrimitive.Positioner></MenuPrimitive.Portal>;
}
export function MenuItem({ className, ...props }: Omit<ComponentProps<typeof MenuPrimitive.Item>, "className"> & { className?: string }) {
  return <MenuPrimitive.Item {...props} data-slot="menu-item" className={cn("ui-popup-item", className)} />;
}
export function MenuSeparator() { return <MenuPrimitive.Separator className="my-2 border-t border-[var(--ui-line)]" />; }

export const Popover = PopoverPrimitive.Root;
export const PopoverTrigger = PopoverPrimitive.Trigger;
export const PopoverClose = PopoverPrimitive.Close;
export function PopoverContent({ className, ...props }: Omit<ComponentProps<typeof PopoverPrimitive.Popup>, "className"> & { className?: string }) {
  return <PopoverPrimitive.Portal><PopoverPrimitive.Positioner sideOffset={8} align="end" style={{ zIndex: 100 }}><PopoverPrimitive.Popup {...props} data-slot="popover" className={cn("ui-popup", className)} /></PopoverPrimitive.Positioner></PopoverPrimitive.Portal>;
}

export type ComboboxOption = { value: string; label: string };
export function Combobox({ options, value, onValueChange, label, placeholder = "Search…", disabled, name }: {
  options: readonly ComboboxOption[];
  value: string;
  onValueChange: (value: string) => void;
  label: string;
  placeholder?: string;
  disabled?: boolean;
  name?: string;
}) {
  return <ComboboxPrimitive.Root items={options} value={options.find(option => option.value === value) ?? null} onValueChange={option => { if (option) onValueChange(option.value); }} disabled={disabled} name={name}>
    <ComboboxPrimitive.InputGroup className="relative flex min-w-0 items-center">
      <ComboboxPrimitive.Input aria-label={label} placeholder={placeholder} data-slot="input" className="ui-standard-control min-w-0 flex-1 pr-11" />
      <ComboboxPrimitive.Trigger aria-label={`Open ${label}`} className="absolute right-0 grid size-11 place-items-center rounded-[var(--ui-radius-md)] text-[var(--ui-muted)]"><ChevronDown size={16} /></ComboboxPrimitive.Trigger>
    </ComboboxPrimitive.InputGroup>
    <ComboboxPrimitive.Portal><ComboboxPrimitive.Positioner sideOffset={6} align="start" style={{ zIndex: 100 }}><ComboboxPrimitive.Popup className="ui-popup min-w-[var(--anchor-width)]" data-slot="combobox-popup">
      <ComboboxPrimitive.Empty className="p-3 text-[var(--ui-muted)]">No matching options</ComboboxPrimitive.Empty>
      <ComboboxPrimitive.List>{(option: ComboboxOption) => <ComboboxPrimitive.Item key={option.value} value={option} className="ui-popup-item"><ComboboxPrimitive.ItemIndicator aria-hidden="true">✓</ComboboxPrimitive.ItemIndicator>{option.label}</ComboboxPrimitive.Item>}</ComboboxPrimitive.List>
    </ComboboxPrimitive.Popup></ComboboxPrimitive.Positioner></ComboboxPrimitive.Portal>
  </ComboboxPrimitive.Root>;
}

export const LocalTabs = TabsPrimitive.Root;
export function TabsList({ className, ...props }: Omit<ComponentProps<typeof TabsPrimitive.List>, "className"> & { className?: string }) {
  return <TabsPrimitive.List activateOnFocus {...props} data-slot="tabs-list" className={cn("ui-tab-nav", className)} />;
}
export function TabsTrigger({ className, ...props }: Omit<ComponentProps<typeof TabsPrimitive.Tab>, "className"> & { className?: string }) {
  return <TabsPrimitive.Tab {...props} data-slot="tab" className={cn("ui-tab-link", className)} />;
}
export const TabsPanel = TabsPrimitive.Panel;

export function Tooltip({ content, children }: { content: ReactNode; children: ReactNode }) {
  return <TooltipPrimitive.Root><TooltipPrimitive.Trigger render={<span className="inline-flex" />}>{children}</TooltipPrimitive.Trigger><TooltipPrimitive.Portal><TooltipPrimitive.Positioner sideOffset={6} style={{ zIndex: 110 }}><TooltipPrimitive.Popup data-slot="tooltip" className="ui-tooltip">{content}</TooltipPrimitive.Popup></TooltipPrimitive.Positioner></TooltipPrimitive.Portal></TooltipPrimitive.Root>;
}
export const TooltipProvider = TooltipPrimitive.Provider;
