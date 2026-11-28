WITH
lead_scope AS MATERIALIZED (
    SELECT l.id,l.company_id,l.branch_id,l.department_id,l.account_id,l.contact_id,l.owner_id,l.created_at,
           opportunity.id AS opportunity_id,opportunity.stage,opportunity.expected_minor,opportunity.currency,
           opportunity.sales_id,opportunity.reservation_id,opportunity.win_source,opportunity.won_at
    FROM crm_leads l
    JOIN crm_accounts account ON account.id=l.account_id AND account.company_id=l.company_id
    JOIN crm_contacts contact ON contact.id=l.contact_id AND contact.account_id=account.id AND contact.company_id=l.company_id
    LEFT JOIN finance_parties customer ON customer.id=account.finance_party_id
        AND customer.company_id=l.company_id AND customer.kind='customer'
    LEFT JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id
    LEFT JOIN commerce_orders quote ON quote.id=opportunity.quote_id AND quote.kind='quote'
        AND quote.company_id=l.company_id AND quote.branch_id=l.branch_id
        AND quote.department_id IS NOT DISTINCT FROM l.department_id
        AND quote.party_id=customer.id AND quote.currency=opportunity.currency
    LEFT JOIN commerce_orders sale ON sale.id=opportunity.sales_id AND sale.kind='sales'
        AND sale.company_id=l.company_id AND sale.branch_id=l.branch_id
        AND sale.department_id IS NOT DISTINCT FROM l.department_id
        AND sale.party_id=customer.id AND sale.currency=opportunity.currency AND sale.source_id=quote.id
    WHERE l.company_id=$1 AND ($2::text IS NULL OR l.branch_id=$2)
        AND ($3::text IS NULL OR l.department_id=$3)
        AND (account.finance_party_id IS NULL OR customer.id IS NOT NULL)
        AND (opportunity.id IS NULL OR (
             customer.id=opportunity.customer_id AND account.reviewed_by IS NOT NULL
             AND (opportunity.quote_id IS NULL OR quote.id IS NOT NULL)
             AND (opportunity.sales_id IS NULL OR sale.id IS NOT NULL)
             AND (opportunity.reservation_id IS NULL OR EXISTS (
                SELECT 1 FROM pms_records reservation
                JOIN pms_properties property ON property.id=reservation.property_id
                 AND property.company_id=l.company_id AND property.branch_id=l.branch_id
                JOIN crm_guest_links linked_guest ON linked_guest.contact_id=l.contact_id
                 AND linked_guest.company_id=l.company_id AND linked_guest.property_id=property.id
                 AND linked_guest.guest_id=reservation.data_json::jsonb->>'guestId'
                WHERE reservation.id=opportunity.reservation_id AND reservation.kind='reservation'
             ))
        ))
),
hotel_sources AS NOT MATERIALIZED (
    SELECT reservation.id,property.id AS property_id,property.branch_id,property.company_id,
        reservation.data_json::jsonb->>'state' AS state, guest.id AS guest_id
    FROM pms_properties property
    JOIN pms_records reservation ON reservation.property_id=property.id AND reservation.kind='reservation'
    JOIN pms_records guest ON guest.id=reservation.data_json::jsonb->>'guestId'
        AND guest.property_id=property.id AND guest.kind='guest'
    WHERE property.company_id=$1 AND ($2::text IS NULL OR property.branch_id=$2)
),
hotel_consent AS NOT MATERIALIZED (
    SELECT sharing.contact_id,sharing.company_id,sharing.property_id,sharing.guest_id
    FROM crm_guest_links sharing
    JOIN crm_guest_sharing_events reviewed ON reviewed.link_id=sharing.id
        AND reviewed.company_id=sharing.company_id AND reviewed.version=sharing.version
        AND reviewed.state='granted' AND reviewed.evidence=sharing.sharing_evidence
        AND reviewed.actor_id=sharing.sharing_by
    WHERE sharing.company_id=$1 AND sharing.sharing_state='granted'
),
report_leads AS MATERIALIZED (
    SELECT l.*,orders.subtotal_minor AS sales_booked_minor,orders.state AS sales_state,
        orders.currency AS booked_currency,reservation.state AS reservation_state,
        CASE l.win_source WHEN 'pms' THEN reservation.id IS NOT NULL WHEN 'sales' THEN orders.id IS NOT NULL ELSE true END AS source_visible,
        (
            SELECT min(coalesce((stay.data_json::jsonb->>'checkedInAt')::bigint,stay.updated_at))
            FROM pms_records stay
            WHERE stay.property_id=reservation.property_id AND stay.kind='stay'
                AND stay.data_json::jsonb->>'reservationId'=reservation.id
                AND (stay.data_json::jsonb->>'guestId' IS NULL OR stay.data_json::jsonb->>'guestId'=reservation.guest_id)
                AND stay.data_json::jsonb->>'state' IN ('in_house','checked_out')
        ) AS checked_in_at
    FROM lead_scope l
    LEFT JOIN commerce_orders orders ON $12::boolean AND orders.id=l.sales_id
        AND orders.company_id=l.company_id AND orders.kind='sales'
    LEFT JOIN hotel_sources reservation ON $13::boolean AND reservation.id=l.reservation_id
        AND reservation.company_id=l.company_id AND reservation.branch_id=l.branch_id
        AND EXISTS (
            SELECT 1 FROM hotel_consent shared WHERE shared.company_id=l.company_id
            AND shared.contact_id=l.contact_id AND shared.property_id=reservation.property_id
            AND shared.guest_id=reservation.guest_id
        )
    WHERE ($4::text IS NULL OR l.branch_id=$4)
        AND ($5::text IS NULL OR reservation.property_id=$5)
        AND ($7::text IS NULL OR l.owner_id=$7)
        AND ($6::text IS NULL OR ($14::boolean AND EXISTS (
            SELECT 1 FROM marketing_touches touch
            JOIN marketing_campaigns campaign ON campaign.id=touch.campaign_id AND campaign.company_id=touch.company_id
            WHERE touch.lead_id=l.id AND touch.company_id=l.company_id AND touch.campaign_id=$6
            AND (
                touch.source_kind='staff' AND touch.source_id=l.id
                OR touch.source_kind='form' AND EXISTS (
                    SELECT 1 FROM marketing_lead_candidates candidate
                    JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id
                        AND form.kind='form' AND form.version>=candidate.form_version
                    JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version
                        AND captured.action='activate' AND captured.status='active'
                    JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1
                        AND submission.event_type='form_submit'
                    WHERE candidate.id=touch.source_id AND candidate.company_id=touch.company_id
                        AND candidate.crm_lead_id=touch.lead_id AND length(trim(candidate.consent_evidence))>=3
                )
                OR touch.source_kind='pms' AND $13::boolean AND EXISTS (
                    SELECT 1 FROM hotel_sources hotel
                    JOIN hotel_consent shared ON shared.company_id=hotel.company_id
                        AND shared.property_id=hotel.property_id AND shared.guest_id=hotel.guest_id
                    WHERE hotel.id=touch.source_id AND hotel.company_id=l.company_id
                        AND hotel.branch_id=l.branch_id AND shared.contact_id=l.contact_id
                )
            )
        )))
),
posting_sources AS MATERIALIZED (
    SELECT l.id AS lead_id,l.company_id,invoice.id AS source_id,invoice.journal_id,'invoice' AS source_kind
    FROM report_leads l
    JOIN commerce_orders orders ON orders.id=l.sales_id AND orders.company_id=l.company_id AND orders.kind='sales'
    JOIN commerce_fulfillments fulfillment ON fulfillment.order_id=orders.id
    JOIN finance_invoices invoice ON invoice.id=fulfillment.invoice_id AND invoice.company_id=orders.company_id
        AND invoice.party_id=orders.party_id AND invoice.kind='receivable'
    WHERE $15::boolean AND invoice.status IN ('posted','partially_paid','paid') AND invoice.journal_id IS NOT NULL
    UNION
    SELECT l.id,l.company_id,returned.id,returned.credit_journal_id,'credit_note'
    FROM report_leads l
    JOIN commerce_orders orders ON orders.id=l.sales_id AND orders.company_id=l.company_id AND orders.kind='sales'
    JOIN commerce_returns returned ON returned.order_id=orders.id AND returned.company_id=orders.company_id
    JOIN commerce_fulfillments fulfillment ON fulfillment.id=returned.fulfillment_id
        AND fulfillment.order_id=orders.id AND fulfillment.invoice_id=returned.invoice_id
    JOIN finance_invoices invoice ON invoice.id=returned.invoice_id AND invoice.company_id=orders.company_id
        AND invoice.party_id=orders.party_id AND invoice.kind='receivable'
    WHERE $15::boolean AND returned.kind='customer_return' AND returned.status='posted' AND returned.credit_journal_id IS NOT NULL
    UNION
    SELECT l.id,l.company_id,entry.id,posting.journal_id,'pms'
    FROM report_leads l
    JOIN hotel_sources reservation ON reservation.id=l.reservation_id
        AND reservation.company_id=l.company_id AND reservation.branch_id=l.branch_id
    JOIN pms_records account ON account.property_id=reservation.property_id AND account.kind='guest_account'
        AND account.data_json::jsonb->>'reservationId'=reservation.id
        AND (account.data_json::jsonb->>'guestId' IS NULL OR account.data_json::jsonb->>'guestId'=reservation.guest_id)
    JOIN pms_records entry ON entry.property_id=account.property_id AND entry.kind='entry'
        AND entry.data_json::jsonb->>'guestAccountId'=account.id
    JOIN pms_postings posting ON posting.record_id=entry.id AND posting.property_id=entry.property_id
    WHERE $16::boolean AND posting.journal_id IS NOT NULL
),
primary_sources AS MATERIALIZED (
    SELECT DISTINCT source.lead_id,journal.id,journal.chart_id,journal.company_id,journal.currency
    FROM posting_sources source
    LEFT JOIN finance_book_postings alias ON alias.secondary_journal_id=source.journal_id AND alias.company_id=source.company_id
    JOIN finance_journals journal ON journal.id=coalesce(alias.primary_journal_id,source.journal_id)
        AND journal.company_id=source.company_id AND journal.source_type=source.source_kind AND journal.source_id=source.source_id
    JOIN finance_charts chart ON chart.id=journal.chart_id AND chart.company_id=journal.company_id
    WHERE journal.status='posted' AND journal.currency='THB' AND journal.entry_date BETWEEN $8 AND $9
        AND NOT EXISTS (SELECT 1 FROM finance_book_postings secondary WHERE secondary.secondary_journal_id=journal.id)
        AND NOT EXISTS (
            SELECT 1 FROM finance_book_postings book
            LEFT JOIN finance_journals secondary ON secondary.id=book.secondary_journal_id
            LEFT JOIN finance_book_rules rule ON rule.id=book.rule_id
            WHERE book.primary_journal_id=journal.id
            AND (
                book.company_id<>journal.company_id OR secondary.company_id IS DISTINCT FROM journal.company_id
                OR secondary.status IS DISTINCT FROM 'posted' OR rule.company_id IS DISTINCT FROM journal.company_id
                OR rule.status IS DISTINCT FROM 'approved' OR rule.reviewed_by IS NULL OR rule.reviewed_by=rule.prepared_by
                OR rule.primary_chart_id<>journal.chart_id OR rule.secondary_chart_id<>secondary.chart_id
                OR secondary.currency<>journal.currency OR secondary.source_type<>'book_projection' OR secondary.source_id<>journal.id
                OR (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=secondary.id)
                    <> (SELECT count(*) FROM finance_journal_lines line WHERE line.journal_id=journal.id)
                OR EXISTS (
                    SELECT 1 FROM finance_journal_lines original
                    LEFT JOIN finance_book_account_mappings mapping ON mapping.rule_id=rule.id AND mapping.primary_account_id=original.account_id
                    LEFT JOIN finance_journal_lines projected ON projected.journal_id=secondary.id AND projected.ordinal=original.ordinal
                    LEFT JOIN finance_accounts account ON account.id=projected.account_id AND account.company_id=journal.company_id AND account.chart_id=secondary.chart_id
                    WHERE original.journal_id=journal.id AND (
                        mapping.secondary_account_id IS NULL OR projected.id IS NULL
                        OR projected.account_id<>mapping.secondary_account_id OR account.id IS NULL
                        OR (projected.debit_minor,projected.credit_minor,projected.base_debit_minor,projected.base_credit_minor)
                            IS DISTINCT FROM (original.debit_minor,original.credit_minor,original.base_debit_minor,original.base_credit_minor)
                    )
                )
            )
        )
),
journal_revenue AS (
    SELECT source.lead_id,source.id,
        coalesce(sum(line.credit_minor-line.debit_minor) FILTER (WHERE account.category='revenue'),0) AS amount
    FROM primary_sources source
    JOIN finance_journal_lines line ON line.journal_id=source.id
    LEFT JOIN finance_accounts account ON account.id=line.account_id
        AND account.company_id=source.company_id AND account.chart_id=source.chart_id
    GROUP BY source.lead_id,source.id
    HAVING count(*)>=2 AND sum(line.debit_minor-line.credit_minor)=0 AND count(account.id)=count(*)
),
revenue AS (
    SELECT lead_id,sum(amount)::bigint AS recognized_minor FROM journal_revenue GROUP BY lead_id
)
SELECT
    jsonb_build_object(
        'leads', ARRAY(
            SELECT to_jsonb(lead) || jsonb_build_object('recognized_minor',revenue.recognized_minor)
            FROM report_leads lead LEFT JOIN revenue ON revenue.lead_id=lead.id
            ORDER BY lead.created_at,lead.id
        ),
        'events', ARRAY(
            SELECT jsonb_build_object('lead_id',event.lead_id,'opportunity_id',event.opportunity_id,
                'action',event.action,'created_at',event.created_at,
                'loss_reason',event.snapshot_json::jsonb->>'lossReason')
            FROM crm_case_events event JOIN report_leads lead ON lead.id=event.lead_id
            WHERE event.created_at<=$10
            ORDER BY event.created_at,event.id
        ),
        'overdue',ARRAY(
            SELECT activity.lead_id FROM crm_lead_activities activity
            JOIN report_leads lead ON lead.id=activity.lead_id AND lead.company_id=activity.company_id
            WHERE activity.status='open' AND activity.due_date<$11
              AND ($7::text IS NULL OR activity.owner_id=$7)
            ORDER BY activity.due_date,activity.id
        )
    ) AS sources,
    jsonb_build_object(
        'branches', ARRAY(
            SELECT jsonb_build_object('id',id,'name',display_name) FROM organization_nodes
            WHERE company_id=$1 AND kind IN ('branch','headquarters') AND ($2::text IS NULL OR id=$2)
            ORDER BY display_name,id
        ),
        'properties', ARRAY(
            SELECT jsonb_build_object('id',id,'name',name) FROM pms_properties
            WHERE $13::boolean AND company_id=$1 AND ($2::text IS NULL OR branch_id=$2)
            ORDER BY name,id
        ),
        'campaigns', ARRAY(
            SELECT jsonb_build_object('id',id,'name',name) FROM marketing_campaigns
            WHERE $14::boolean AND company_id=$1 ORDER BY name,id
        ),
        'owners', ARRAY(
            SELECT jsonb_build_object('id',id,'name',name)
            FROM (
                SELECT DISTINCT lead.owner_id AS id,coalesce(person.legal_name,lead.owner_id) AS name
                FROM lead_scope lead
                LEFT JOIN hr_people person ON person.user_id=lead.owner_id AND person.company_id=lead.company_id
                    AND person.merged_into_id IS NULL
            ) owner_options
            ORDER BY name,id
        )
    ) AS options
