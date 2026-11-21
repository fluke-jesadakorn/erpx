//! Brand identity decisions: approved category locks and independent review.
use crate::domain::identity::{BrandCategory, BrandCategoryLock, BrandIdentity};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::Value;

pub fn category_locked(category: BrandCategory) -> BusinessError {
    match category {
        BrandCategory::Identity => BusinessError::new(
            FailureKind::Conflict,
            "BRAND_CATEGORY_LOCKED",
            "The approved identity category is locked. An owner must review and unlock it before a change.",
        ),
        BrandCategory::Visual => BusinessError::new(
            FailureKind::Conflict,
            "BRAND_CATEGORY_LOCKED",
            "The approved visual category is locked. An owner must review and unlock it before a change.",
        ),
    }
}

fn approved_snapshot(lock: &BrandCategoryLock) -> Option<Value> {
    serde_json::from_str::<Value>(&lock.approved_snapshot_json).ok()
}

/// A locked category accepts only its independently approved snapshot. An
/// unreadable snapshot fails closed instead of silently permitting a change.
pub fn assert_brand_category_locks(
    locks: &[BrandCategoryLock],
    identity: &BrandIdentity,
) -> Result<(), BusinessError> {
    for lock in locks {
        if !lock.locked {
            continue;
        }
        let category = lock
            .category()
            .ok_or_else(|| category_locked(BrandCategory::Identity))?;
        if approved_snapshot(lock).as_ref() != Some(&identity.category(category)) {
            return Err(category_locked(category));
        }
    }
    Ok(())
}

pub fn independent_publication() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BRAND_INDEPENDENT_PUBLICATION",
        "Another authorized person must review and publish this draft.",
    )
}

/// The publishing actor may be neither the draft creator nor its last updater.
pub fn assert_brand_independent_publisher(
    created_by_is_actor: bool,
    updated_by_is_actor: bool,
) -> Result<(), BusinessError> {
    if created_by_is_actor || updated_by_is_actor {
        return Err(independent_publication());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::identity::BrandIdentity;

    fn identity() -> BrandIdentity {
        BrandIdentity::parse(&serde_json::json!({
            "identity": {"shortName": "Acme", "tagline": "Approved"},
            "visual": {"accentColor": "#224488", "logoPublicationId": null}
        }))
        .unwrap()
    }

    fn lock(category: BrandCategory, locked: bool, snapshot: Value) -> BrandCategoryLock {
        BrandCategoryLock {
            company_id: "company".into(),
            category: category.as_str().into(),
            locked,
            approved_snapshot_json: snapshot.to_string(),
            version: 1,
            updated_by: "owner".into(),
            reason: "Reviewed".into(),
            updated_at: 1,
        }
    }

    #[test]
    fn locked_categories_reject_changed_values_and_allow_approved_values() {
        let current = identity();
        let locks = [
            lock(BrandCategory::Identity, true, serde_json::json!(current.identity)),
            lock(BrandCategory::Visual, true, serde_json::json!(current.visual)),
        ];
        assert!(assert_brand_category_locks(&locks, &current).is_ok());
        let mut changed = current.clone();
        changed.visual.accent_color = "#778899".into();
        let error = assert_brand_category_locks(&locks, &changed).unwrap_err();
        assert_eq!(error.code, "BRAND_CATEGORY_LOCKED");
        assert!(error.message.contains("visual"));
    }

    #[test]
    fn unlocked_categories_accept_reviewed_changes() {
        let mut changed = identity();
        changed.visual.accent_color = "#778899".into();
        let locks = [lock(
            BrandCategory::Visual,
            false,
            serde_json::json!(identity().visual),
        )];
        assert!(assert_brand_category_locks(&locks, &changed).is_ok());
    }

    #[test]
    fn unreadable_approved_snapshot_fails_closed() {
        let mut lock = lock(BrandCategory::Visual, true, serde_json::json!(null));
        lock.approved_snapshot_json = "{".into();
        let error = assert_brand_category_locks(&[lock], &identity()).unwrap_err();
        assert_eq!(error.code, "BRAND_CATEGORY_LOCKED");
    }

    #[test]
    fn publication_requires_an_independent_person() {
        assert!(assert_brand_independent_publisher(false, false).is_ok());
        assert_eq!(
            assert_brand_independent_publisher(true, false)
                .unwrap_err()
                .code,
            "BRAND_INDEPENDENT_PUBLICATION"
        );
        assert_eq!(
            assert_brand_independent_publisher(false, true)
                .unwrap_err()
                .code,
            "BRAND_INDEPENDENT_PUBLICATION"
        );
    }
}
