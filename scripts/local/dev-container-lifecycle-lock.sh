#!/bin/sh
# Atomic host lock shared by both lifecycle entrypoints; no automatic stale recovery.
acquire_lifecycle_lock() {
  lock_parent="$HOME/.colima/erpx-opensource-dev/lifecycle-locks"
  (umask 077; mkdir -p "$lock_parent")
  [ -d "$lock_parent" ] && [ ! -L "$lock_parent" ] || { echo LIFECYCLE_LOCK_DIRECTORY_INVALID >&2; exit 1; }
  lifecycle_lock="$lock_parent/$ERPX_DEV_WORKSPACE_KEY.lock"
  (umask 077; mkdir "$lifecycle_lock") 2>/dev/null || { echo WORKSPACE_LIFECYCLE_LOCK_HELD_OR_STALE >&2; exit 1; }
}
release_lifecycle_lock() { rmdir "$lifecycle_lock" || echo WORKSPACE_LIFECYCLE_LOCK_RELEASE_FAILED >&2; }
