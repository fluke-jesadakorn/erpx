"use client";

import {
  Children,
  cloneElement,
  createContext,
  isValidElement,
  useContext,
  type ReactElement,
  type ReactNode,
} from "react";
import { frontText } from "@erpx/feature-shared/lib/front-language";

export type FrontLanguage = "th" | "en";

export const FrontLanguageContext = createContext<FrontLanguage>("en");

export function useFrontLanguage() {
  return useContext(FrontLanguageContext);
}

function translateNode(node: ReactNode, language: FrontLanguage): ReactNode {
  return Children.map(node, (child) => {
    if (typeof child === "string") {
      const normalized = child.replace(/\s+/g, " ").trim();
      const translated = frontText(normalized, language);
      return translated === normalized ? child : translated;
    }
    if (!isValidElement(child)) return child;

    const element = child as ReactElement<{
      children?: ReactNode;
      title?: string;
      placeholder?: string;
      alt?: string;
      "aria-label"?: string;
    }>;
    const props = { ...element.props };
    if (typeof element.type === "string") {
      for (const key of ["title", "placeholder", "alt", "aria-label"] as const) {
        if (typeof props[key] === "string") props[key] = frontText(props[key]!, language);
      }
    }
    if (props.children !== undefined) props.children = translateNode(props.children, language);
    return cloneElement(element, props);
  });
}

export function FrontLocalizedTree({ children }: { children: ReactNode }) {
  const language = useFrontLanguage();
  return <>{translateNode(children, language)}</>;
}
