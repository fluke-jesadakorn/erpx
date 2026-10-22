/** Purchased-product visibility for disposable Learn workspace fixtures only. */
import { createHash } from 'node:crypto';
import type { PostgresDatabase } from '@erpx/server-runtime/adapters/runtime/postgres';
import type { EngineeringMoneyPeopleBrowser } from '../../finance/builders/engineering-money-people-browser';

export async function seedLearningWorkspacePackages(input: { run: string; db: PostgresDatabase; academy: EngineeringMoneyPeopleBrowser }) {
  const url = new URL(String(input.db.pool.options.connectionString));
  if (!/^[a-f0-9]{12}$/.test(input.run) || input.academy.run !== input.run || url.hostname !== '127.0.0.1' || url.port !== '5432' || url.username !== 'erpx_test_admin' || url.pathname !== `/erpx_browser_${input.run}_h001_db`) throw new Error('DISPOSABLE_LEARNING_PACKAGE_FIXTURE_REQUIRED');
  const now = Date.now();
  for (const company of Object.values(input.academy.companies)) {
    const image = { companyId: company.companyId, packageId: 'learn', packageVersion: 1, state: 'active', startsAt: now - 1_000, endsAt: null, version: 1,
      reason: 'Fictional Learn development-smoke Company package visibility.', evidence: 'Disposable test fixture only; no Commercial purchase or installed grant.' };
    const eventId = `fixture:learning-workspace:${input.run}:${company.companyId}`;
    await input.db.transaction(async tx => {
      const course = await tx.prepare('SELECT id FROM learning_courses WHERE company_id=? AND id=?').bind(company.companyId, company.learning.courseId).first();
      if (!course) throw new Error('OWNED_LEARNING_COMPANY_COURSE_REQUIRED');
      await tx.prepare('INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)')
        .bind(image.companyId, image.packageId, image.packageVersion, image.state, image.startsAt, image.endsAt, image.version, image.reason, image.evidence, now).run();
      await tx.prepare('INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES(?,?,?,?,?,NULL,?::jsonb,?)')
        .bind(eventId, image.companyId, image.packageId, company.users['academy-author'].userId, createHash('sha256').update(JSON.stringify([eventId, image])).digest('hex'), JSON.stringify(image), now).run();
    });
  }
}
