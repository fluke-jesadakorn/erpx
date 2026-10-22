import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { acceptedReleasePredicate, releaseRevisionTable } from "@erpx/server-platform_administration/public/release-acceptance";
import artifact from "@erpx/catalogue/release-artifact.json";
import type { PmsRolloutRelease } from "@erpx/server-pms/public/rollout-contracts";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { productionScope, legalProductionScope } from "@erpx/catalogue/production-scope";
export function pmsRolloutReleaseGuards(db: SqlDatabase, key: string, actor: WorkContext, release: PmsRolloutRelease | null, scope: typeof productionScope.version | typeof legalProductionScope.version) {
    return release?.release ? [
        db.prepare(`SELECT revision FROM ${releaseRevisionTable(scope)} WHERE company_id=? FOR SHARE`).bind(actor.companyId),
        db.prepare('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE').bind(release.release.id),
        assertion(db, key, 'feature-release', `EXISTS(SELECT 1 FROM company_release_acceptances r JOIN ${releaseRevisionTable(scope)} c ON c.company_id=r.company_id
          WHERE r.id=? AND r.company_id=? AND r.version=? AND r.revoked_at IS NULL AND r.configuration_revision=c.revision
          AND r.scope_version=? AND r.software_version=?)`, [release.release.id, actor.companyId, release.release.version, scope, artifact.softwareVersion]),
        assertion(db, key, 'feature-release-reviewers', acceptedReleasePredicate('?', scope), [actor.companyId]),
    ] : [];
}
