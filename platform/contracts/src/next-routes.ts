import type { ReactNode } from "react";

type Segment<S extends string> = S extends `[[...${infer Key}]]` ? { [K in Key]?: string[] }
  : S extends `[...${infer Key}]` ? { [K in Key]: string[] }
  : S extends `[${infer Key}]` ? { [K in Key]: string } : Record<never, never>;
type Parameters<Path extends string> = Path extends `${infer Head}/${infer Tail}`
  ? Segment<Head> & Parameters<Tail> : Segment<Path>;

/** Route payloads are shared code; Next's generated helpers belong to one app. */
export type PageProps<Path extends string = string> = {
  params: Promise<Parameters<Path>>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
};
export type LayoutProps<Path extends string = string> = {
  children: ReactNode;
  params: Promise<Parameters<Path>>;
  record?: ReactNode;
};
export type RouteContext<Path extends string = string> = { params: Promise<Parameters<Path>> };
