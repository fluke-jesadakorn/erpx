import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { acceptedReleasePredicate, releaseRevisionShareLockSql, releaseRevisionTable, releaseTableExistsSql } from "@erpx/platform-administration-server/public/release-predicate";
import artifact from "@erpx/platform-catalogue/release-artifact.json";
import type { PmsRolloutRelease } from "@erpx/pms-server/public/rollout-contracts";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { productionScope, legalProductionScope } from "@erpx/platform-catalogue/production-scope";
export function pmsRolloutReleaseGuards(db: SqlDatabase, key: string, actor: WorkContext, release: PmsRolloutRelease | null, scope: typeof productionScope.version | typeof legalProductionScope.version) {
    return release?.release ? [
        db.prepare(releaseRevisionShareLockSql(scope)).bind(actor.companyId),
        db.prepare('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE').bind(release.release.id),
        assertion(db, key, 'feature-release', releaseTableExistsSql(`company_release_acceptances r JOIN ${releaseRevisionTable(scope)} c ON c.company_id=r.company_id`, `r.id=? AND r.company_id=? AND r.version=? AND r.revoked_at IS NULL AND r.configuration_revision=c.revision
          AND r.scope_version=? AND r.software_version=?`, `
          `), [release.release.id, actor.companyId, release.release.version, scope, artifact.softwareVersion]),
        assertion(db, key, 'feature-release-reviewers', acceptedReleasePredicate('?', scope), [actor.companyId]),
    ] : [];
}
