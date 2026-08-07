use std::{
    cmp::Ordering,
    collections::BTreeSet,
    error::Error,
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) type CacheResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

pub(crate) const CACHE_BUDGET_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const RUNTIME_TARGET_NAMES: [&str; 4] = [
    "fast-lane-target",
    "xtask-target",
    "check-fast-target",
    "test-fast-target",
];

#[derive(Debug, Clone)]
pub(crate) struct CacheGroup {
    pub(crate) id: String,
    pub(crate) path: PathBuf,
    pub(crate) size: u64,
    pub(crate) last_used: SystemTime,
    pub(crate) pinned: bool,
}

pub(crate) fn status(root: &Path) -> CacheResult {
    let groups = inventory(root)?;
    let total = groups.iter().map(|group| group.size).sum::<u64>();
    println!(
        "Cargo build cache: {} across {} group(s); budget {}",
        human_bytes(total),
        groups.len(),
        human_bytes(CACHE_BUDGET_BYTES)
    );
    for group in groups {
        println!(
            "  {:>9}  {}  {}{}",
            human_bytes(group.size),
            display_time(group.last_used),
            group.id,
            if group.pinned { " [pinned]" } else { "" }
        );
    }
    Ok(())
}

pub(crate) fn prune(root: &Path, dry_run: bool, extra_pins: &[PathBuf]) -> CacheResult {
    if rustc_writer_active(root) {
        return Err("refusing Cargo-cache pruning while rustc is writing the workspace".into());
    }
    let mut groups = inventory(root)?;
    for group in &mut groups {
        if extra_pins
            .iter()
            .any(|pin| pin == &group.path || pin.starts_with(&group.path))
        {
            group.pinned = true;
        }
    }
    let before = groups.iter().map(|group| group.size).sum::<u64>();
    let mandatory_ids = groups
        .iter()
        .filter(|group| {
            (is_legacy_runtime_group(&group.id) || is_deferred_platform_group(&group.id))
                && !group.pinned
        })
        .map(|group| group.id.clone())
        .collect::<BTreeSet<_>>();
    let retained = groups
        .iter()
        .filter(|group| !mandatory_ids.contains(&group.id))
        .cloned()
        .collect::<Vec<_>>();
    let (budget_evictions, _) = select_evictions(retained, CACHE_BUDGET_BYTES);
    let budget_ids = budget_evictions
        .iter()
        .map(|group| group.id.clone())
        .collect::<BTreeSet<_>>();
    let evictions = groups
        .into_iter()
        .filter(|group| mandatory_ids.contains(&group.id) || budget_ids.contains(&group.id))
        .collect::<Vec<_>>();
    let removed = evictions.iter().map(|group| group.size).sum::<u64>();
    let after = before.saturating_sub(removed);
    for group in evictions {
        validate_group(root, &group.path)?;
        if dry_run {
            println!(
                "would remove {} ({})",
                group.path.display(),
                human_bytes(group.size)
            );
        } else {
            match fs::remove_dir_all(&group.path) {
                Ok(()) => println!(
                    "removed {} ({})",
                    group.path.display(),
                    human_bytes(group.size)
                ),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            remove_marker(root, &group.id)?;
        }
    }
    let budget_met = after <= CACHE_BUDGET_BYTES;
    println!(
        "Cargo build cache: {} -> {}{}",
        human_bytes(before),
        human_bytes(after),
        if dry_run { " (dry run)" } else { "" }
    );
    if !budget_met {
        eprintln!(
            "warning: Cargo build cache remains above {} because active cache groups are pinned",
            human_bytes(CACHE_BUDGET_BYTES)
        );
    }
    Ok(())
}

pub(crate) fn prune_incremental(root: &Path, dry_run: bool) -> CacheResult {
    if recorded_web_process_active(root) || rustc_writer_active(root) {
        return Err(
            "refusing incremental-cache pruning while Dioxus or rustc is writing the workspace"
                .into(),
        );
    }
    let mut directories = Vec::new();
    for group in inventory(root)? {
        validate_group(root, &group.path)?;
        collect_incremental_directories(&group.path, &mut directories)?;
    }
    directories.sort();
    directories.dedup();
    let mut total = 0_u64;
    for directory in directories {
        validate_incremental_directory(root, &directory)?;
        let (size, _) = scan_directory(&directory)?;
        total = total.saturating_add(size);
        if dry_run {
            println!(
                "would remove {} ({})",
                directory.display(),
                human_bytes(size)
            );
        } else {
            match fs::remove_dir_all(&directory) {
                Ok(()) => println!("removed {} ({})", directory.display(), human_bytes(size)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    println!(
        "incremental Cargo cache selected: {}{}",
        human_bytes(total),
        if dry_run { " (dry run)" } else { "" }
    );
    Ok(())
}

pub(crate) fn auto_prune(root: &Path, extra_pins: &[PathBuf]) {
    match inventory(root) {
        Ok(groups) if groups.iter().map(|group| group.size).sum::<u64>() <= CACHE_BUDGET_BYTES => {}
        Ok(_) => {
            if let Err(error) = prune(root, false, extra_pins) {
                eprintln!("warning: automatic Cargo cache pruning failed: {error}");
            }
        }
        Err(error) => eprintln!("warning: automatic Cargo cache inventory failed: {error}"),
    }
}

pub(crate) fn mark_used(root: &Path, paths: &[PathBuf]) -> CacheResult {
    for path in paths {
        let Some(id) = group_id(root, path) else {
            continue;
        };
        let marker = marker_path(root, &id);
        if let Some(parent) = marker.parent() {
            fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(marker)?;
    }
    Ok(())
}

pub(crate) fn mark_modified_since(root: &Path, since: SystemTime) -> CacheResult {
    let paths = inventory(root)?
        .into_iter()
        .filter(|group| group.last_used >= since)
        .map(|group| group.path)
        .collect::<Vec<_>>();
    mark_used(root, &paths)
}

pub(crate) fn inventory(root: &Path) -> CacheResult<Vec<CacheGroup>> {
    validate_cache_roots(root)?;
    let mut paths = Vec::new();
    let workspace_target = root.join("target");
    if workspace_target.is_dir() {
        for entry in fs::read_dir(&workspace_target)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                paths.push(entry.path());
            } else if metadata.file_type().is_symlink() {
                return Err(format!(
                    "refusing symlinked Cargo cache group {}",
                    entry.path().display()
                )
                .into());
            }
        }
    }
    let runtime = runtime_dir(root);
    for name in RUNTIME_TARGET_NAMES {
        let path = runtime.join(name);
        if path.exists() {
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(
                    format!("refusing symlinked Cargo cache group {}", path.display()).into(),
                );
            }
            if metadata.is_dir() {
                paths.push(path);
            }
        }
    }

    let web_active = recorded_web_process_active(root);
    let native_runtime_active = recorded_native_runtime_active(root);
    let current_exe = std::env::current_exe().ok();
    let mut groups = Vec::new();
    for path in paths {
        validate_group(root, &path)?;
        let id = group_id(root, &path).ok_or("Cargo cache group is outside the allowlist")?;
        let (size, filesystem_time) = scan_directory(&path)?;
        let marker_time = fs::metadata(marker_path(root, &id))
            .and_then(|metadata| metadata.modified())
            .ok();
        let last_used = marker_time
            .into_iter()
            .chain(filesystem_time)
            .max()
            .unwrap_or(UNIX_EPOCH);
        let web_group = is_live_web_cache_group(&workspace_target, &path);
        let native_group = path == workspace_target.join("debug");
        let pinned = (web_active && web_group)
            || (native_runtime_active && native_group)
            || current_exe
                .as_ref()
                .is_some_and(|executable| executable.starts_with(&path));
        groups.push(CacheGroup {
            id,
            path,
            size,
            last_used,
            pinned,
        });
    }
    groups.sort_by(|left, right| right.last_used.cmp(&left.last_used));
    Ok(groups)
}

fn compare_lru(left: &CacheGroup, right: &CacheGroup) -> Ordering {
    left.last_used
        .cmp(&right.last_used)
        .then_with(|| left.id.cmp(&right.id))
}

fn is_live_web_cache_group(workspace_target: &Path, path: &Path) -> bool {
    // `dx serve` writes these shared development groups. Target-triple groups
    // belong to the completed `dx build` gate; an active rustc still protects
    // them while that build is in progress, but the detached server does not.
    path.parent() == Some(workspace_target)
        && path.file_name().is_some_and(|name| {
            ["dx", "wasm-dev", "server-dev"]
                .iter()
                .any(|candidate| name == *candidate)
        })
}

fn is_legacy_runtime_group(id: &str) -> bool {
    matches!(
        id,
        "runtime--fast-lane-target" | "runtime--check-fast-target" | "runtime--test-fast-target"
    )
}

fn is_deferred_platform_group(id: &str) -> bool {
    matches!(
        id,
        "workspace--aarch64-apple-ios" | "workspace--aarch64-linux-android"
    )
}

fn select_evictions(mut groups: Vec<CacheGroup>, budget: u64) -> (Vec<CacheGroup>, u64) {
    let total = groups.iter().map(|group| group.size).sum::<u64>();
    if total <= budget {
        return (Vec::new(), total);
    }
    let pinned_size = groups
        .iter()
        .filter(|group| group.pinned)
        .map(|group| group.size)
        .sum::<u64>();
    let mut remaining = budget.saturating_sub(pinned_size);
    groups.sort_by(|left, right| compare_lru(right, left));
    let mut evictions = Vec::new();
    for group in groups {
        if group.pinned {
            continue;
        }
        if group.size <= remaining {
            remaining -= group.size;
        } else {
            evictions.push(group);
        }
    }
    let removed = evictions.iter().map(|group| group.size).sum::<u64>();
    let after = total.saturating_sub(removed);
    (evictions, after)
}

fn scan_directory(path: &Path) -> CacheResult<(u64, Option<SystemTime>)> {
    let mut pending = vec![path.to_path_buf()];
    let mut size = 0_u64;
    let mut latest = None;
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            latest = metadata.modified().ok().into_iter().chain(latest).max();
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                size = size.saturating_add(metadata.len());
            }
        }
    }
    Ok((size, latest))
}

fn validate_cache_roots(root: &Path) -> CacheResult {
    let canonical_root = root.canonicalize()?;
    for path in [root.join("target"), runtime_dir(root)] {
        if !path.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(format!("refusing symlinked cache root {}", path.display()).into());
        }
        if !path.canonicalize()?.starts_with(&canonical_root) {
            return Err(format!("cache root escapes workspace: {}", path.display()).into());
        }
    }
    Ok(())
}

fn validate_group(root: &Path, path: &Path) -> CacheResult {
    let Some(expected_id) = group_id(root, path) else {
        return Err(format!(
            "Cargo cache group is outside the allowlist: {}",
            path.display()
        )
        .into());
    };
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(format!("unsafe Cargo cache group {expected_id}: {}", path.display()).into());
    }
    let canonical_root = root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(format!("Cargo cache group escapes workspace: {}", path.display()).into());
    }
    Ok(())
}

fn collect_incremental_directories(path: &Path, found: &mut Vec<PathBuf>) -> CacheResult {
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "refusing symlink inside Cargo cache: {}",
                    entry.path().display()
                )
                .into());
            }
            if !metadata.is_dir() {
                continue;
            }
            if entry.file_name() == "incremental" {
                found.push(entry.path());
            } else {
                pending.push(entry.path());
            }
        }
    }
    Ok(())
}

fn validate_incremental_directory(root: &Path, path: &Path) -> CacheResult {
    if path.file_name().and_then(|name| name.to_str()) != Some("incremental") {
        return Err(format!("refusing non-incremental cache path: {}", path.display()).into());
    }
    let canonical_root = root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(canonical_root.join("target"))
        && !canonical_path.starts_with(runtime_dir(root))
    {
        return Err(format!("incremental cache escapes workspace: {}", path.display()).into());
    }
    Ok(())
}

fn group_id(root: &Path, path: &Path) -> Option<String> {
    let workspace_target = root.join("target");
    if path.parent() == Some(workspace_target.as_path()) {
        return path
            .file_name()
            .map(|name| format!("workspace--{}", name.to_string_lossy()));
    }
    let runtime = runtime_dir(root);
    if path.parent() == Some(runtime.as_path()) {
        let name = path.file_name()?.to_str()?;
        if RUNTIME_TARGET_NAMES.contains(&name) {
            return Some(format!("runtime--{name}"));
        }
    }
    None
}

fn marker_path(root: &Path, id: &str) -> PathBuf {
    runtime_dir(root)
        .join("cache-access")
        .join(format!("{id}.stamp"))
}

fn remove_marker(root: &Path, id: &str) -> CacheResult {
    match fs::remove_file(marker_path(root, id)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn runtime_dir(root: &Path) -> PathBuf {
    root.join(".runtime/erpx")
}

fn recorded_web_process_active(root: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(runtime_dir(root).join("web.pid")) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        return false;
    };
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| String::from_utf8_lossy(&output.stdout).contains("dx serve"))
}

fn recorded_native_runtime_active(root: &Path) -> bool {
    let runtime = runtime_dir(root);
    let Ok(entries) = fs::read_dir(runtime) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("pid")
        {
            return false;
        }
        let Ok(contents) = fs::read_to_string(entry.path()) else {
            return false;
        };
        let Ok(pid) = contents.trim().parse::<u32>() else {
            return false;
        };
        process_command(pid).is_some_and(|command| {
            command.contains("target/debug/erpx-")
                || command.contains("erpx-server")
                || command.contains("erpx-worker")
                || command.contains("erpx-ucs-lab")
        })
    })
}

fn rustc_writer_active(root: &Path) -> bool {
    let Ok(output) = Command::new("ps").args(["-axo", "command="]).output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let root = root.to_string_lossy();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|command| command.contains("rustc") && command.contains(root.as_ref()))
}

fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn display_time(time: SystemTime) -> String {
    if time == UNIX_EPOCH {
        return "unknown".to_owned();
    }
    let Ok(age) = SystemTime::now().duration_since(time) else {
        return "future".to_owned();
    };
    let seconds = age.as_secs();
    if seconds < 60 {
        format!("{seconds}s ago")
    } else if seconds < 60 * 60 {
        format!("{}m ago", seconds / 60)
    } else if seconds < 24 * 60 * 60 {
        format!("{}h ago", seconds / (60 * 60))
    } else {
        format!("{}d ago", seconds / (24 * 60 * 60))
    }
}

fn human_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", bytes as f64 / GIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::symlink,
        path::{Path, PathBuf},
        time::{Duration, SystemTime},
    };

    use super::{
        CACHE_BUDGET_BYTES, CacheGroup, compare_lru, group_id, is_deferred_platform_group,
        is_legacy_runtime_group, is_live_web_cache_group, prune, prune_incremental, scan_directory,
        select_evictions, validate_cache_roots,
    };

    fn temp_workspace(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "erpx-cache-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".runtime/erpx")).expect("runtime");
        root
    }

    fn group(id: &str, age: u64, pinned: bool) -> CacheGroup {
        CacheGroup {
            id: id.to_owned(),
            path: PathBuf::from(id),
            size: 1,
            last_used: SystemTime::UNIX_EPOCH + Duration::from_secs(age),
            pinned,
        }
    }

    #[test]
    fn directory_scan_counts_files_without_following_symlinks() {
        let root = temp_workspace("scan");
        let cache = root.join("target/debug");
        fs::create_dir_all(&cache).expect("cache");
        fs::write(cache.join("artifact"), [0_u8; 16]).expect("artifact");
        symlink(root.join("missing"), cache.join("ignored-link")).expect("symlink");
        let (size, modified) = scan_directory(&cache).expect("scan");
        assert_eq!(size, 16);
        assert!(modified.is_some());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn lru_order_is_oldest_first_with_stable_id_tiebreaker() {
        let mut groups = [
            group("new", 20, false),
            group("old-b", 10, false),
            group("old-a", 10, false),
        ];
        groups.sort_by(compare_lru);
        assert_eq!(groups.map(|item| item.id), ["old-a", "old-b", "new"]);
    }

    #[test]
    fn eviction_respects_budget_and_pinned_groups() {
        let mut pinned = group("pinned", 1, true);
        pinned.size = 7;
        let mut old = group("old", 2, false);
        old.size = 5;
        let mut new = group("new", 3, false);
        new.size = 5;
        let (evictions, after) = select_evictions(vec![new, pinned, old], 12);
        assert_eq!(evictions.len(), 1);
        assert_eq!(evictions[0].id, "old");
        assert_eq!(after, 12);
    }

    #[test]
    fn oversized_recent_group_does_not_force_smaller_groups_out() {
        let mut oversized = group("oversized-new", 30, false);
        oversized.size = 30;
        let mut older = group("older", 20, false);
        older.size = 10;
        let mut oldest = group("oldest", 10, false);
        oldest.size = 10;
        let (evictions, after) = select_evictions(vec![oldest, older, oversized], 25);
        assert_eq!(
            evictions
                .iter()
                .map(|group| group.id.as_str())
                .collect::<Vec<_>>(),
            vec!["oversized-new"]
        );
        assert_eq!(after, 20);
    }

    #[test]
    fn dry_run_does_not_remove_selected_cache_group() {
        let root = temp_workspace("dry-run");
        let old = root.join("target/old");
        let new = root.join("target/new");
        fs::create_dir_all(&old).expect("old cache");
        fs::create_dir_all(&new).expect("new cache");
        let old_file = fs::File::create(old.join("artifact")).expect("old artifact");
        old_file
            .set_len(CACHE_BUDGET_BYTES / 2 + 1)
            .expect("old sparse artifact");
        let new_file = fs::File::create(new.join("artifact")).expect("new artifact");
        new_file
            .set_len(CACHE_BUDGET_BYTES / 2 + 1)
            .expect("new sparse artifact");
        prune(&root, true, &[]).expect("dry run");
        assert!(old.exists());
        assert!(new.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn incremental_dry_run_is_bounded_and_non_destructive() {
        let root = temp_workspace("incremental-dry-run");
        let incremental = root.join("target/debug/incremental");
        fs::create_dir_all(&incremental).expect("incremental cache");
        fs::write(incremental.join("artifact"), [0_u8; 32]).expect("artifact");
        prune_incremental(&root, true).expect("incremental dry run");
        assert!(incremental.join("artifact").is_file());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn only_superseded_fast_lane_targets_are_legacy() {
        for id in [
            "runtime--fast-lane-target",
            "runtime--check-fast-target",
            "runtime--test-fast-target",
        ] {
            assert!(is_legacy_runtime_group(id));
        }
        assert!(!is_legacy_runtime_group("runtime--xtask-target"));
        assert!(!is_legacy_runtime_group("workspace--debug"));
    }

    #[test]
    fn deferred_cross_platform_target_groups_are_prunable() {
        for id in [
            "workspace--aarch64-apple-ios",
            "workspace--aarch64-linux-android",
        ] {
            assert!(is_deferred_platform_group(id));
        }
        assert!(!is_deferred_platform_group(
            "workspace--aarch64-apple-darwin"
        ));
        assert!(!is_deferred_platform_group("workspace--debug"));
        assert!(!is_deferred_platform_group(
            "workspace--wasm32-unknown-unknown"
        ));
    }

    #[test]
    fn vanished_directory_is_an_empty_scan() {
        let root = temp_workspace("vanished");
        let missing = root.join("target/missing");
        let (size, modified) = scan_directory(&missing).expect("missing directory scan");
        assert_eq!(size, 0);
        assert!(modified.is_none());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn group_ids_only_accept_allowlisted_roots() {
        let root = Path::new("/workspace");
        assert_eq!(
            group_id(root, &root.join("target/debug")).as_deref(),
            Some("workspace--debug")
        );
        assert_eq!(
            group_id(root, &root.join(".runtime/erpx/fast-lane-target")).as_deref(),
            Some("runtime--fast-lane-target")
        );
        assert!(group_id(root, &root.join(".runtime/n8n")).is_none());
    }

    #[test]
    fn active_web_pins_only_dioxus_writable_target_groups() {
        let target = Path::new("/workspace/target");
        assert!(is_live_web_cache_group(target, &target.join("dx")));
        assert!(is_live_web_cache_group(target, &target.join("wasm-dev")));
        assert!(is_live_web_cache_group(target, &target.join("server-dev")));
        assert!(!is_live_web_cache_group(
            target,
            &target.join("wasm32-unknown-unknown")
        ));
        assert!(!is_live_web_cache_group(
            target,
            &target.join("aarch64-apple-darwin")
        ));
        assert!(!is_live_web_cache_group(target, &target.join("debug")));
        assert!(!is_live_web_cache_group(
            target,
            &target.join("aarch64-apple-ios")
        ));
    }

    #[test]
    fn symlinked_target_root_is_rejected() {
        let root = temp_workspace("escape");
        let outside = temp_workspace("outside");
        symlink(outside.join(".runtime"), root.join("target")).expect("symlink target");
        let error = validate_cache_roots(&root).expect_err("must reject symlink");
        assert!(error.to_string().contains("symlinked cache root"));
        fs::remove_file(root.join("target")).expect("remove symlink");
        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(outside).expect("cleanup outside");
    }
}
