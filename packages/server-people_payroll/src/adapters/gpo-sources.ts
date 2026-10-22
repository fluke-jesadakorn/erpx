import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "@erpx/server-identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** All facts use the supplied transaction; Company and placement scope remain explicit. */
export async function gpoActiveHolders(db:SqlDatabase,companyId:string){return (await db.prepare(`SELECT e.id engagement_id,e.person_id,e.position_id,e.version,p.legal_name name,
    COALESCE((SELECT jsonb_agg(jsonb_build_object('connection',i.connection_id,'id',i.external_id) ORDER BY i.connection_id,i.external_id)
      ${identityaccessFinalSources.gpo_sources_iam_members_m_from} ${identityaccessFinalSources.gpo_sources_iam_identities_i_join} ON i.id=m.identity_id AND i.company_id=m.company_id AND i.person_id=m.person_id
      WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active' AND i.enabled=true),'[]'::jsonb) accounts
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    WHERE e.company_id=? AND e.status='active' AND e.start_date<=CURRENT_DATE::text AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      AND p.status='active' AND p.merged_into_id IS NULL ORDER BY e.id`).bind(companyId)
    .all<{ engagement_id: string; person_id: string; position_id: string; version: number; name: string; accounts: { connection: string; id: string }[] | string }>()).results;}
export async function gpoPositionHolders(db:SqlDatabase,connectionId:string,companyId:string,positionId:string){return (await db.prepare(`SELECT e.id engagement_id,e.person_id,e.version,p.legal_name name,i.external_id account_id
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    LEFT JOIN LATERAL (SELECT DISTINCT i.external_id ${identityaccessFinalSources.gpo_sources_iam_members_m_from} ${identityaccessFinalSources.gpo_sources_iam_identities_i_join} ON i.id=m.identity_id
      WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active'
        AND i.company_id=e.company_id AND i.person_id=e.person_id AND i.connection_id=? AND i.enabled=true) i ON true
    WHERE e.company_id=? AND e.position_id=? AND e.status='active' AND e.start_date<=CURRENT_DATE::text
      AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text)
      AND p.status='active' AND p.merged_into_id IS NULL
    ORDER BY p.legal_name,e.id,i.external_id`).bind(connectionId,companyId,positionId).all<{engagement_id:string;person_id:string;version:number;name:string;account_id:string|null}>()).results;}
export async function gpoUnmapHolders(db:SqlDatabase,connection_id:string,companyId:string,positionId:string){return (await db.prepare(`SELECT e.person_id,p.legal_name,
      ARRAY(SELECT DISTINCT i.external_id ${identityaccessFinalSources.gpo_sources_iam_members_m_from} ${identityaccessFinalSources.gpo_sources_iam_identities_i_join} ON i.id=m.identity_id
        WHERE m.company_id=e.company_id AND m.person_id=e.person_id AND m.status='active'
          AND i.company_id=e.company_id AND i.person_id=e.person_id AND i.connection_id=? AND i.enabled=true ORDER BY i.external_id) account_ids
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id
    WHERE e.company_id=? AND e.position_id=? AND e.status='active' AND e.start_date<=CURRENT_DATE::text
      AND (e.end_date IS NULL OR e.end_date>=CURRENT_DATE::text) AND p.status='active' AND p.merged_into_id IS NULL ORDER BY p.legal_name,e.id`)
    .bind(connection_id,companyId,positionId).all<{person_id:string;legal_name:string;account_ids:string[]}>()).results;}
