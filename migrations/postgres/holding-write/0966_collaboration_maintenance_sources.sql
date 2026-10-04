-- Extend the approved shared source resolver without altering previously applied
-- native-source migrations or granting access through a relation/action card.
ALTER TABLE collaboration_record_relations DROP CONSTRAINT collaboration_record_relations_source_type_check;
ALTER TABLE collaboration_record_relations DROP CONSTRAINT collaboration_record_relations_target_type_check;
ALTER TABLE collaboration_action_cards DROP CONSTRAINT collaboration_action_cards_source_type_check;
ALTER TABLE collaboration_record_relations
 ADD CONSTRAINT collaboration_record_relations_source_type_check CHECK(source_type IN ('commerce_order','support_ticket','field_service_order','staff_task','pos_sale','pms_record','asset','maintenance_plan','maintenance_work')),
 ADD CONSTRAINT collaboration_record_relations_target_type_check CHECK(target_type IN ('commerce_order','support_ticket','field_service_order','staff_task','pos_sale','pms_record','asset','maintenance_plan','maintenance_work'));
ALTER TABLE collaboration_action_cards ADD CONSTRAINT collaboration_action_cards_source_type_check CHECK(source_type IN ('commerce_order','support_ticket','field_service_order','staff_task','pos_sale','pms_record','asset','maintenance_plan','maintenance_work'));
