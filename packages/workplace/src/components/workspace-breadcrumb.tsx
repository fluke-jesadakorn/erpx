"use client";
import { Button } from "@erpx/ui/components/foundation";

import Link from "@erpx/workplace/components/workspace-link";
import {useProductNavigationScope,useProductPathname} from './product-link-context';
import {productNavigationHref} from '@erpx/feature-work_projection/lib/product-navigation';
import {workspacePathParts} from '@erpx/feature-work_projection/lib/destination-registry';
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { ChevronRight, MoreHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  navigationTrail,
  workplaceNavigation,
  type NavigationContext,
} from "@erpx/feature-work_projection/lib/workplace-navigation";
import styles from "@erpx/workplace/components/shell.module.css";

export type WorkspaceCrumb = {
  href?: string;
  label: string;
  fullNavigation?: boolean;
};
type Crumb = WorkspaceCrumb;

export function WorkspaceBreadcrumb({
  workspaceBase,
  ancestors,
  navigation,
}: {
  workspaceBase: string;
  ancestors: WorkspaceCrumb[];
  navigation: NavigationContext;
}) {
  const pathname = useProductPathname();
  const productScope=useProductNavigationScope(),parsedPath=workspacePathParts(pathname,productScope.holdingSlug);
  const searchParams = useSearchParams();
  const overflow = useRef<HTMLLIElement>(null);
  const overflowButton = useRef<HTMLButtonElement>(null);
  const [overflowOpen, setOverflowOpen] = useState(false);
  const routeCrumbs: Crumb[] = navigationTrail(
    workplaceNavigation(navigation),
    workspaceBase,
    parsedPath?`${parsedPath.base}/${parsedPath.path}`:pathname,
    searchParams,
  );
  const crumbs = [...ancestors, ...routeCrumbs];
  const currentCrumb = crumbs.at(-1);
  const hiddenCrumbs = crumbs.slice(0, -1);

  useEffect(() => {
    if (!overflowOpen) return;
    function close(event: MouseEvent) {
      if (!overflow.current?.contains(event.target as Node)) setOverflowOpen(false);
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      setOverflowOpen(false);
      overflowButton.current?.focus();
    }
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [overflowOpen]);

  function crumbContent(crumb: Crumb, current: boolean) {
    if (crumb.href && !current)
      return crumb.fullNavigation ? (
        <a data-product-switch href={productNavigationHref(crumb.href,productScope).href} title={crumb.label}>
          {crumb.label}
        </a>
      ) : (
        <Link href={crumb.href} title={crumb.label}>
          {crumb.label}
        </Link>
      );
    return (
      <span aria-current={current ? "page" : undefined} title={crumb.label}>
        {crumb.label}
      </span>
    );
  }

  return (
    <nav className={styles.breadcrumb} aria-label="Breadcrumb" data-condensed={crumbs.length > 4 || undefined}>
      <ol>
        {crumbs.map((crumb, index) => {
          const current = index === crumbs.length - 1;
          return (
            <li
              key={`${crumb.label}-${index}`}
              className={`${styles.breadcrumbItem} ${styles.breadcrumbFullItem}`}
            >
              {index > 0 && (
                <ChevronRight size={14} aria-hidden="true" className={styles.breadcrumbSeparator} />
              )}
              {crumbContent(crumb, current)}
            </li>
          );
        })}
        {hiddenCrumbs.length > 0 && currentCrumb && (
          <li
            className={`${styles.breadcrumbItem} ${styles.breadcrumbCompactOverflow}`}
            ref={overflow}
          >
            <Button appearance="standard"
              variant="ghost"
              ref={overflowButton}
              type="button"
              className={styles.breadcrumbOverflowButton}
              aria-label="Show breadcrumb path"
              aria-expanded={overflowOpen}
              onClick={() => setOverflowOpen((open) => !open)}
            >
              <MoreHorizontal size={18} aria-hidden="true" />
            </Button>
            {overflowOpen && (
              <div
                className={styles.breadcrumbOverflowMenu}
                role="group"
                aria-label="Breadcrumb path"
              >
                {hiddenCrumbs.map((crumb, index) =>
                  crumb.href ? (
                    crumb.fullNavigation ? (
                      <a
                        data-product-switch
                        key={`${crumb.label}-${index}`}
                        href={crumb.href}
                        onClick={() => setOverflowOpen(false)}
                      >
                        {crumb.label}
                      </a>
                    ) : (
                      <Link
                        key={`${crumb.label}-${index}`}
                        href={crumb.href}
                        onClick={() => setOverflowOpen(false)}
                      >
                        {crumb.label}
                      </Link>
                    )
                  ) : (
                    <span key={`${crumb.label}-${index}`}>{crumb.label}</span>
                  ),
                )}
              </div>
            )}
            <ChevronRight size={14} aria-hidden="true" className={styles.breadcrumbSeparator} />
          </li>
        )}
        {currentCrumb && (
          <li className={`${styles.breadcrumbItem} ${styles.breadcrumbCompactCurrent}`}>
            {crumbContent(currentCrumb, true)}
          </li>
        )}
      </ol>
    </nav>
  );
}
