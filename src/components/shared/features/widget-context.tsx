"use client";
import { createContext, type ReactNode, type HTMLAttributes } from "react";
import type { UiAppearance } from "@/components/ui/appearance";
export type FeatureWidgetProps = HTMLAttributes<HTMLElement> & { id: string; title: string; columns?: number; appearance?: UiAppearance };
export const InsideWidget = createContext(false);
export const WidgetRenderer = createContext<((props: FeatureWidgetProps) => ReactNode) | null>(null);
export const PageDrawer = createContext<{ open: boolean; setOpen: (open: boolean) => void } | null>(null);
export const GridLocation = createContext<{ target: string | null } | null>(null);
