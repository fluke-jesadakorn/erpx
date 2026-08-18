//! Non-secret platform preference stores.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::platform::{PreferenceKey, PreferenceStore};

const MAX_VALUE_BYTES: usize = 128;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn key_name(key: PreferenceKey) -> &'static str {
    match key {
        PreferenceKey::Locale => "locale",
        PreferenceKey::Theme => "theme",
        PreferenceKey::NavigationDisclosure => "navigation-disclosure",
    }
}

fn valid_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VALUE_BYTES
        && !value.chars().any(char::is_control)
        && !value.contains(['\n', '\r', '='])
}

/// Small file-backed store for non-secret macOS preferences.
///
/// Writes use a same-directory temporary file, `sync_all`, and rename. The
/// in-memory view changes only after persistence succeeds.
#[derive(Debug, Clone)]
pub struct MacOsPreferenceStore {
    path: PathBuf,
    values: Rc<RefCell<BTreeMap<PreferenceKey, String>>>,
}

impl MacOsPreferenceStore {
    pub fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let values = if path.exists() {
            parse_preferences(&fs::read_to_string(&path)?)
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            path,
            values: Rc::new(RefCell::new(values)),
        })
    }

    pub fn default_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Application Support/ERPX/preferences-v1"))
    }

    pub fn try_store(&self, key: PreferenceKey, value: &str) -> io::Result<()> {
        if !valid_value(value) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid preference value",
            ));
        }
        let mut next = self.values.borrow().clone();
        next.insert(key, value.to_owned());
        write_atomic(&self.path, &next)?;
        *self.values.borrow_mut() = next;
        Ok(())
    }
}

impl PreferenceStore for MacOsPreferenceStore {
    fn load(&self, key: PreferenceKey) -> Option<String> {
        self.values.borrow().get(&key).cloned()
    }

    fn store(&self, key: PreferenceKey, value: &str) {
        let _ = self.try_store(key, value);
    }
}

fn parse_preferences(contents: &str) -> BTreeMap<PreferenceKey, String> {
    contents
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = match key {
                "locale" => PreferenceKey::Locale,
                "theme" => PreferenceKey::Theme,
                "navigation-disclosure" => PreferenceKey::NavigationDisclosure,
                _ => return None,
            };
            valid_value(value).then(|| (key, value.to_owned()))
        })
        .collect()
}

fn write_atomic(path: &Path, values: &BTreeMap<PreferenceKey, String>) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "preference path has no parent")
    })?;
    fs::create_dir_all(parent)?;
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        for (key, value) in values {
            writeln!(file, "{}={value}", key_name(*key))?;
        }
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default)]
pub struct WebPreferenceStore;

#[cfg(target_arch = "wasm32")]
impl PreferenceStore for WebPreferenceStore {
    fn load(&self, key: PreferenceKey) -> Option<String> {
        web_sys::window()?
            .local_storage()
            .ok()??
            .get_item(key_name(key))
            .ok()?
            .filter(|value| valid_value(value))
    }

    fn store(&self, key: PreferenceKey, value: &str) {
        if !valid_value(value) {
            return;
        }
        if let Some(storage) =
            web_sys::window().and_then(|window| window.local_storage().ok().flatten())
        {
            let _ = storage.set_item(key_name(key), value);
        }
    }
}

/// Update the Web document language without evaluating JavaScript.
#[cfg(target_arch = "wasm32")]
pub fn set_web_document_language(language: &str) -> bool {
    if !valid_value(language) {
        return false;
    }
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .is_some_and(|root| root.set_attribute("lang", language).is_ok())
}

#[cfg(target_arch = "wasm32")]
pub fn apply_web_locale(language: &str) -> bool {
    use wasm_bindgen::JsCast;

    if !set_web_document_language(language) {
        return false;
    }
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return false;
    };
    if let Some(root) = document.document_element() {
        let _ = root.set_attribute("data-locale", language);
    }
    document
        .dyn_into::<web_sys::HtmlDocument>()
        .ok()
        .is_some_and(|document| {
            document
                .set_cookie(&format!(
                    "erpx_locale={language}; Path=/; Max-Age=31536000; SameSite=Strict"
                ))
                .is_ok()
        })
}

#[cfg(target_arch = "wasm32")]
pub fn web_location_path() -> Option<String> {
    let location = web_sys::window()?.location();
    let hash = location.hash().ok()?;
    if let Some(path) = hash.strip_prefix("#/") {
        return Some(format!("/{path}"));
    }
    Some(format!(
        "{}{}",
        location.pathname().ok()?,
        location.search().ok()?
    ))
}

#[cfg(target_arch = "wasm32")]
pub fn web_origin() -> Option<String> {
    web_sys::window()?
        .location()
        .origin()
        .ok()
        .filter(|origin| !origin.is_empty() && origin != "null")
}

#[cfg(target_arch = "wasm32")]
pub fn replace_web_location(path: &str) -> bool {
    crate::platform::validate_deep_link(path).is_ok()
        && web_sys::window().is_some_and(|window| window.location().replace(path).is_ok())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "erpx-{label}-{}-{nonce}/preferences",
            std::process::id()
        ))
    }

    #[test]
    fn preferences_round_trip_from_atomic_file() {
        let path = test_path("preferences");
        let store = MacOsPreferenceStore::open(&path).expect("open store");
        store
            .try_store(PreferenceKey::Locale, "th-TH")
            .expect("store locale");
        store
            .try_store(PreferenceKey::Theme, "dark")
            .expect("store theme");
        let reopened = MacOsPreferenceStore::open(&path).expect("reopen store");
        assert_eq!(
            reopened.load(PreferenceKey::Locale).as_deref(),
            Some("th-TH")
        );
        assert_eq!(reopened.load(PreferenceKey::Theme).as_deref(), Some("dark"));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn invalid_values_never_replace_persisted_preferences() {
        let path = test_path("invalid-preference");
        let store = MacOsPreferenceStore::open(&path).expect("open store");
        store
            .try_store(PreferenceKey::Locale, "en-US")
            .expect("store locale");
        assert!(
            store
                .try_store(PreferenceKey::Locale, "th-TH\nsecret=value")
                .is_err()
        );
        assert_eq!(store.load(PreferenceKey::Locale).as_deref(), Some("en-US"));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
