import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';import {assertion} from "@erpx/server-workflow/public/command-assertions";export function leaveReviewEvidenceFacts(database:SqlDatabase){return {guard11(key:string,value:"leave-evidence-canonical-count",id:string,company_id:string,length:number){return assertion(database,
          key,
          value,
          `(
            SELECT count(*)
            FROM attachment_sets set_row
            JOIN attachment_links link ON link.set_id=set_row.id
            JOIN attachments source ON source.id=link.attachment_id
            WHERE set_row.target_type='leave_request' AND set_row.target_id=?
              AND set_row.company_id=? AND source.company_id=set_row.company_id
              AND set_row.state IN ('draft','bound')
              AND link.state IN ('draft','bound')
              AND source.source_state='available'
          )=?`,
          [id, company_id, length],
        );},
guard12(key:string,value:string,id:string,company_id:string,linkId:string,sourceId:string,purposeKey:string,checksumSha256:string,mediaType:string,bytes:number){return assertion(database,
            key,
            value,
            `EXISTS(
              SELECT 1
              FROM attachment_sets set_row
              JOIN attachment_links link ON link.set_id=set_row.id
              JOIN attachments source ON source.id=link.attachment_id
              WHERE set_row.target_type='leave_request' AND set_row.target_id=?
                AND set_row.company_id=? AND source.company_id=set_row.company_id
                AND set_row.state IN ('draft','bound')
                AND link.state IN ('draft','bound')
                AND source.source_state='available'
                AND link.id=? AND source.id=? AND set_row.purpose_key=?
                AND source.checksum_sha256=?
                AND COALESCE(source.detected_media_type,source.media_type)=?
                AND source.bytes=?
            )`,
            [
              id,
              company_id,
              linkId,
              sourceId,
              purposeKey,
              checksumSha256,
              mediaType,
              bytes,
            ],
          );},
guard13(key:string,value:"leave-evidence-legacy-count",id:string,company_id:string,length:number){return assertion(database,
                key,
                value,
                `(
                  SELECT count(*)
                  FROM attachments source
                  WHERE source.owner_type='leave_request' AND source.owner_id=?
                    AND source.company_id=? AND source.revoked_at IS NULL
                    AND source.source_state='available'
                    AND NOT EXISTS(
                      SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
                    )
                )=?`,
                [id, company_id, length],
              );},
guard14(key:string,value:string,sourceId:string,id:string,company_id:string,purposeKey:string,checksumSha256:string,mediaType:string,bytes:number){return assertion(database,
                  key,
                  value,
                  `EXISTS(
                    SELECT 1
                    FROM attachments source
                    WHERE source.id=? AND source.owner_type='leave_request'
                      AND source.owner_id=? AND source.company_id=?
                      AND source.revoked_at IS NULL AND source.source_state='available'
                      AND CASE
                        WHEN source.purpose IN ('medical_certificate','leave_supporting_photo')
                          THEN source.purpose
                        ELSE 'leave_supporting_photo'
                      END=?
                      AND source.checksum_sha256=?
                      AND COALESCE(source.detected_media_type,source.media_type)=?
                      AND source.bytes=?
                      AND NOT EXISTS(
                        SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
                      )
                  )`,
                  [
                    sourceId,
                    id,
                    company_id,
                    purposeKey,
                    checksumSha256,
                    mediaType,
                    bytes,
                  ],
                );},
guard15(key:string,value:"leave-evidence-no-pending",id:string,company_id:string){return assertion(database,
                key,
                value,
                `NOT EXISTS(
                  SELECT 1
                  FROM attachment_sets set_row
                  JOIN attachment_links link ON link.set_id=set_row.id
                  LEFT JOIN attachments source ON source.id=link.attachment_id
                    AND source.company_id=set_row.company_id
                  LEFT JOIN upload_intents intent ON intent.id=link.upload_intent_id
                    AND intent.company_id=set_row.company_id
                  WHERE set_row.target_type='leave_request' AND set_row.target_id=?
                    AND set_row.company_id=? AND set_row.state='draft'
                    AND link.state='draft'
                    AND (
                      (link.attachment_id IS NULL AND intent.id IS NOT NULL)
                      OR (source.id IS NOT NULL AND source.source_state<>'available')
                    )
                )`,
                [id, company_id],
              );},
guard16(key:string,value:"leave-evidence-required-count",value2:(string | number)[],legacyIncluded:boolean){return assertion(database,
                      key,
                      value,
                      `(
                        SELECT count(DISTINCT evidence.source_id)
                        FROM (
                          SELECT source.id source_id
                          FROM attachment_sets set_row
                          JOIN attachment_links link ON link.set_id=set_row.id
                          JOIN attachments source ON source.id=link.attachment_id
                            AND source.company_id=set_row.company_id
                          WHERE set_row.target_type='leave_request' AND set_row.target_id=?
                            AND set_row.company_id=? AND set_row.purpose_key=?
                            AND set_row.state IN ('draft','bound')
                            AND link.state IN ('draft','bound')
                            AND source.source_state='available'
                          ${legacyIncluded ? `UNION ALL
                          SELECT source.id source_id
                          FROM attachments source
                          WHERE source.owner_type='leave_request' AND source.owner_id=?
                            AND source.company_id=? AND source.purpose=?
                            AND source.revoked_at IS NULL AND source.source_state='available'
                            AND NOT EXISTS(
                              SELECT 1 FROM attachment_links link WHERE link.attachment_id=source.id
                            )` : ""}
                        ) evidence
                      )>=?`,
                      value2,
                    );}};}
