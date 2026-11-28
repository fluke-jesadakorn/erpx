-- A finite scheduled producer cutover; preserve stored notices and read state.
CREATE FUNCTION erpx_native_field_reservation_notice() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF current_setting('erpx.notification_runtime',true) IS DISTINCT FROM 'native-rust'
 AND ((NEW.record_type='field_service_order' AND NEW.id LIKE 'inventory-reservation-%')
   OR (TG_OP='UPDATE' AND OLD.record_type='field_service_order' AND OLD.id LIKE 'inventory-reservation-%'))
 AND (TG_OP='INSERT' OR (to_jsonb(NEW)-'read_at') IS DISTINCT FROM (to_jsonb(OLD)-'read_at')) THEN
  RAISE EXCEPTION 'Field reservation alerts require the native runtime'
   USING ERRCODE='23514',CONSTRAINT='native_field_reservation_notice_required';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER native_field_reservation_notice_required BEFORE INSERT OR UPDATE ON staff_notifications
 FOR EACH ROW EXECUTE FUNCTION erpx_native_field_reservation_notice();
