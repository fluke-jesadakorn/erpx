use erpx_server_identity_access::public::directory::framing::*;

fn nested(depth: usize) -> Vec<u8> {
    let mut result = vec![0x04, 1, 0x80];
    for _ in 0..depth {
        assert!(result.len() < 128);
        let mut next = vec![0x30, result.len() as u8];
        next.extend(result);
        result = next;
    }
    result
}

#[test]
fn rejects_declared_bomb_before_body_and_bounds_header() {
    assert_eq!(
        frame_length(&[0x30, 0x84, 0x7f, 0xff, 0xff, 0xff]),
        Err(BerFailure::TooLarge)
    );
    assert_eq!(frame_length(&[0x30, 0xff]), Err(BerFailure::TooLarge));
    assert_eq!(frame_length(&[0x30, 0x80]), Err(BerFailure::Indefinite));
    assert_eq!(frame_length(&[0x31, 0]), Err(BerFailure::Malformed));
    assert_eq!(frame_length(&[0x30, 0x82, 0]), Ok(None));
    assert_eq!(
        frame_length(&[0x30, 0x82, 0, 3]),
        Ok(Some(BerFrameLength {
            header: 4,
            total: 7
        }))
    );
}

#[test]
fn rejects_nested_indefinite_depth_overflow_and_truncated_constructed() {
    assert_eq!(
        validate_complete_frame(&[0x30, 4, 0x30, 0x80, 0, 0]),
        Err(BerFailure::Indefinite)
    );
    assert_eq!(
        validate_complete_frame(&[0x30, 3, 0x04, 2, 0]),
        Err(BerFailure::Truncated)
    );
    let body = [0x04, 0].repeat(MAX_BER_ELEMENTS);
    let mut elements = vec![0x30, 0x82, (body.len() >> 8) as u8, body.len() as u8];
    elements.extend(body);
    assert_eq!(
        validate_complete_frame(&elements),
        Err(BerFailure::TooManyElements)
    );
    assert_eq!(validate_complete_frame(&nested(32)), Ok(()));
    assert_eq!(
        validate_complete_frame(&nested(33)),
        Err(BerFailure::TooDeep)
    );
}

#[test]
fn opaque_octets_are_not_ber_and_extra_frames_are_not_silently_consumed() {
    assert_eq!(
        validate_complete_frame(&[0x30, 4, 0x04, 2, 0x30, 0x80]),
        Ok(())
    );
    assert_eq!(
        validate_complete_frame(&[0x30, 0, 0x30, 0]),
        Err(BerFailure::Malformed)
    );
    assert_eq!(
        validate_complete_frame(&[0x30, 3, 0x04]),
        Err(BerFailure::Truncated)
    );
}

#[test]
fn wire_budget_reserves_before_payload_retention_and_never_wraps() {
    let mut budget = WireBudget::new();
    for _ in 0..64 {
        assert_eq!(budget.reserve(MAX_BER_FRAME_BYTES), Ok(()));
    }
    assert_eq!(budget.reserve(1), Err(BerFailure::Budget));
    assert_eq!(
        WireBudget::new().reserve(usize::MAX),
        Err(BerFailure::TooLarge)
    );
    let mut frames = WireBudget::new();
    for _ in 0..MAX_WIRE_FRAMES {
        assert_eq!(frames.reserve(0), Ok(()));
    }
    assert_eq!(frames.reserve(0), Err(BerFailure::Budget));
}
