import "server-only";
import {one} from '@erpx/platform-runtime-server/public/runtime/sql';
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ORG_NODE_DISPLAY_NAME_SOURCE} from './org-sources';
import {nativeLane24PosBranchOptions, type Lane24Actor} from './native-lane-24/client';

/**
 * POS organization facts. Branch option lists are owned by the native lane-24
 * router; the caller supplies a WorkContext-derived target. The company name
 * read stays owner-local (separate capability, no SQL change here).
 */
export function PosReadOrganizationFacts(db: SqlDatabase) {
  return {
    readOrganizationNodes1: (facts:{company:string;}) => one(db, ORG_NODE_DISPLAY_NAME_SOURCE, facts.company),
    readOrganizationNodes2: (
      facts:{company:string;branch:string | null;branch2:string | null;},
      target?: Lane24Actor,
    ) => nativeLane24PosBranchOptions(target, facts.company, facts.branch, facts.branch2),
  };
}
