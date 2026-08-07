//! Compile-time target boundary for client session persistence.
//!
//! The shared UI crate deliberately does not select browser storage, a file, or an in-memory
//! persistence fallback. Each target application must replace the adapter for its target with an
//! implementation backed by an OS-protected credential facility before session restoration is
//! enabled. Until then, the explicit `Unsupported` result is fail-closed.

#[cfg(any(test, target_os = "android", target_os = "ios", target_os = "macos"))]
use crate::StoredSessionBundle;
use crate::{
    Locale, SafeUiText, SecureSessionBundleStore, SecureSessionError, SecureSessionFuture,
    SecureSessionStore, SessionId, StoredAccountSummary, StoredSession,
};

/// Client target selected at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientTarget {
    /// WebAssembly/browser client.
    Web,
    /// macOS, Windows, or Linux desktop client.
    Desktop,
    /// Android client.
    Android,
    /// iOS client.
    Ios,
    /// A target outside the approved client matrix.
    Unsupported,
}

impl ClientTarget {
    /// Stable display label for diagnostics and non-secret UI status.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Web => "Web",
            Self::Desktop => "Desktop",
            Self::Android => "Android",
            Self::Ios => "iOS",
            Self::Unsupported => "Unsupported target",
        }
    }

    /// Locale-aware target label without changing the stable diagnostic API.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Desktop, Locale::Th) => "เดสก์ท็อป",
            (Self::Unsupported, Locale::Th) => "เป้าหมายที่ไม่รองรับ",
            (Self::Web, Locale::Th) => "เว็บ",
            (Self::Android, Locale::Th) => "Android",
            (Self::Ios, Locale::Th) => "iOS",
            (_, Locale::En) => self.label(),
        }
    }
}

/// Whether this build has an OS-protected session implementation wired into its target adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureSessionSupport {
    /// A target adapter is present and may be used for restoration.
    Supported,
    /// No protected adapter is present; callers must not persist or restore credentials.
    Unsupported,
}

impl SecureSessionSupport {
    /// Stable display label for diagnostics and non-secret UI status.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Supported => "secure storage configured",
            Self::Unsupported => "secure storage unavailable in this build",
        }
    }

    /// Locale-aware protected-session status.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Supported, Locale::Th) => "กำหนดค่าการจัดเก็บที่ปลอดภัยแล้ว",
            (Self::Unsupported, Locale::Th) => "รุ่นนี้ไม่มีการจัดเก็บที่ปลอดภัย",
            (_, Locale::En) => self.label(),
        }
    }
}

// These modules are the target-owned replacement points. Apple targets are wired to the system
// Keychain below and Android is wired to Android Keystore plus encrypted SharedPreferences. The
// unsupported behavior remains explicit for web and Windows/Linux desktop until their target
// applications provide an OS-protected implementation.
#[cfg(target_arch = "wasm32")]
mod current_target {
    use super::ClientTarget;

    pub const TARGET: ClientTarget = ClientTarget::Web;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "android"))]
mod current_target {
    use super::ClientTarget;

    pub const TARGET: ClientTarget = ClientTarget::Android;
}

#[cfg(all(not(target_arch = "wasm32"), target_os = "ios"))]
mod current_target {
    use super::ClientTarget;

    pub const TARGET: ClientTarget = ClientTarget::Ios;
}

#[cfg(all(
    not(target_arch = "wasm32"),
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
mod current_target {
    use super::ClientTarget;

    pub const TARGET: ClientTarget = ClientTarget::Desktop;
}

#[cfg(all(
    not(target_arch = "wasm32"),
    not(target_os = "android"),
    not(target_os = "ios"),
    not(any(target_os = "macos", target_os = "windows", target_os = "linux"))
))]
mod current_target {
    use super::ClientTarget;

    pub const TARGET: ClientTarget = ClientTarget::Unsupported;
}

/// Describe the current build without exposing or inspecting session material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSessionDescriptor {
    /// Target selected by the compiler.
    target: ClientTarget,
    /// Protected persistence capability compiled into this client.
    secure_session: SecureSessionSupport,
}

impl TargetSessionDescriptor {
    /// Describe a web client whose refresh credential is held only by the
    /// server in an `HttpOnly`, same-site cookie. The console owns the HTTP
    /// exchange; the shared target adapter still never uses browser storage.
    pub const fn browser_http_only_cookie() -> Self {
        Self {
            target: ClientTarget::Web,
            secure_session: SecureSessionSupport::Supported,
        }
    }

    /// Return the compiler-selected client target.
    pub const fn target(self) -> ClientTarget {
        self.target
    }

    /// Return the protected persistence capability compiled into this client.
    pub const fn secure_session(self) -> SecureSessionSupport {
        self.secure_session
    }

    /// Return a display-safe status string for an authentication surface.
    pub const fn status_label(self) -> &'static str {
        self.secure_session.label()
    }

    /// Return locale-aware protected-session status copy.
    pub const fn status_label_for(self, locale: Locale) -> &'static str {
        self.secure_session.label_for(locale)
    }

    /// Return bounded, target-neutral copy for secure-storage status surfaces.
    pub fn status_copy(self) -> SafeUiText {
        let message = match self.secure_session {
            SecureSessionSupport::Supported => "Secure session storage is configured.",
            SecureSessionSupport::Unsupported => {
                "Secure session storage is unavailable; no local fallback is available."
            }
        };
        SafeUiText::new(message)
    }

    /// Local persistence fallback is never permitted by the shared UI contract.
    pub const fn local_fallback_allowed(self) -> bool {
        false
    }
}

const fn compiled_secure_session_support() -> SecureSessionSupport {
    match current_target::TARGET {
        ClientTarget::Android | ClientTarget::Ios => SecureSessionSupport::Supported,
        ClientTarget::Desktop if cfg!(target_os = "macos") => SecureSessionSupport::Supported,
        ClientTarget::Web | ClientTarget::Desktop | ClientTarget::Unsupported => {
            SecureSessionSupport::Unsupported
        }
    }
}

/// The fail-closed adapter supplied until a target application wires an OS-protected store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TargetSecureSessionAdapter;

#[cfg(any(target_os = "android", test))]
const MAX_ANDROID_CIPHERTEXT_BYTES: usize = StoredSessionBundle::MAX_SECURE_STORE_BYTES + 256;
#[cfg(any(target_os = "android", test))]
const MAX_ANDROID_ENCODED_BYTES: usize = MAX_ANDROID_CIPHERTEXT_BYTES.div_ceil(3) * 4;
#[cfg(any(target_os = "android", test))]
const ANDROID_BASE64_ENCODE_SIGNATURE: &str = "([BI)Ljava/lang/String;";

/// Android's default Base64 decoder accepts characters outside the encoded alphabet. A secure
/// session value comes from app-local preferences, so reject anything that is not the canonical
/// unwrapped representation before handing it to the platform decoder. This also keeps the
/// encoded value bounded before JNI allocates a Java string.
#[cfg(any(target_os = "android", test))]
fn is_valid_android_base64(encoded: &str) -> bool {
    let bytes = encoded.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_ANDROID_ENCODED_BYTES || !bytes.len().is_multiple_of(4)
    {
        return false;
    }

    let padding = bytes.iter().rev().take_while(|byte| **byte == b'=').count();
    if padding > 2 {
        return false;
    }

    let content_len = bytes.len() - padding;
    if bytes[..content_len].iter().any(|byte| {
        !matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/'
        )
    }) {
        return false;
    }

    // Enforce canonical zero bits in the final Base64 quantum. The Android decoder otherwise
    // accepts multiple textual encodings for the same ciphertext.
    match padding {
        0 => true,
        1 => base64_sextet(bytes[content_len - 1]).is_some_and(|value| value & 0b11 == 0),
        2 => base64_sextet(bytes[content_len - 1]).is_some_and(|value| value & 0b1111 == 0),
        _ => false,
    }
}

#[cfg(any(target_os = "android", test))]
fn base64_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
mod apple_keychain {
    use super::{
        SecureSessionError, SessionId, StoredAccountSummary, StoredSession, StoredSessionBundle,
    };
    use security_framework::passwords::{
        PasswordOptions, delete_generic_password_options, generic_password,
        set_generic_password_options,
    };
    use security_framework_sys::base::errSecItemNotFound;

    const SERVICE: &str = "com.legacy.erpx.session";
    const ACCOUNT: &str = "current";

    fn keychain_options() -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(SERVICE, ACCOUNT);
        // Session credentials are device-local identity material. Explicitly selecting the
        // non-synchronizable keychain prevents an iCloud copy and keeps reads, writes, and
        // deletes scoped to the same local item.
        options.set_access_synchronized(Some(false));
        #[cfg(target_os = "ios")]
        options.use_protected_keychain();
        options
    }

    fn map_error(error: security_framework::base::Error) -> SecureSessionError {
        if error.code() == errSecItemNotFound {
            SecureSessionError::Unavailable
        } else {
            SecureSessionError::Failed
        }
    }

    fn load_bundle() -> Result<Option<StoredSessionBundle>, SecureSessionError> {
        match generic_password(keychain_options()) {
            Ok(bytes) => StoredSessionBundle::decode_secure_store(&bytes).map(Some),
            Err(error) if error.code() == errSecItemNotFound => Ok(None),
            Err(error) => Err(map_error(error)),
        }
    }

    fn save_bundle(bundle: &StoredSessionBundle) -> Result<(), SecureSessionError> {
        let bytes = bundle.encode_secure_store()?;
        set_generic_password_options(&bytes, keychain_options()).map_err(map_error)
    }

    fn delete_bundle() -> Result<(), SecureSessionError> {
        match delete_generic_password_options(keychain_options()) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == errSecItemNotFound => Ok(()),
            Err(error) => Err(map_error(error)),
        }
    }

    pub fn load() -> Result<Option<StoredSession>, SecureSessionError> {
        Ok(load_bundle()?.map(|bundle| bundle.active().clone()))
    }

    pub fn save(session: &StoredSession) -> Result<(), SecureSessionError> {
        let mut bundle =
            load_bundle()?.unwrap_or_else(|| StoredSessionBundle::new(session.clone()));
        bundle.upsert(session.clone());
        save_bundle(&bundle)
    }

    pub fn clear() -> Result<(), SecureSessionError> {
        let Some(bundle) = load_bundle()? else {
            return delete_bundle();
        };
        let active_id = bundle.active().session_id().clone();
        remove_account(&active_id).map(|_| ())
    }

    pub(super) fn accounts() -> Result<Vec<StoredAccountSummary>, SecureSessionError> {
        Ok(load_bundle()?
            .map(|bundle| bundle.account_summaries())
            .unwrap_or_default())
    }

    pub(super) fn replacement_candidates(
        session: &StoredSession,
    ) -> Result<Vec<StoredSession>, SecureSessionError> {
        let Some(mut bundle) = load_bundle()? else {
            return Ok(Vec::new());
        };
        Ok(bundle.upsert(session.clone()))
    }

    pub(super) fn load_account(
        session_id: &SessionId,
    ) -> Result<Option<StoredSession>, SecureSessionError> {
        Ok(load_bundle()?.and_then(|bundle| bundle.session(session_id).cloned()))
    }

    pub(super) fn remove_account(session_id: &SessionId) -> Result<bool, SecureSessionError> {
        let Some(mut bundle) = load_bundle()? else {
            return Ok(false);
        };
        let removed = bundle.remove(session_id).is_some();
        if !removed {
            return Ok(false);
        }
        if bundle.is_empty() {
            delete_bundle()?;
        } else {
            save_bundle(&bundle)?;
        }
        Ok(true)
    }
}

#[cfg(target_os = "android")]
mod android_keystore {
    use std::panic::AssertUnwindSafe;

    use erpx_android_bridge::with_attached as bridge_with_attached;
    use jni::{JNIEnv, objects::JByteArray, objects::JObject, objects::JValue};

    use super::{
        SecureSessionError, SessionId, StoredAccountSummary, StoredSession, StoredSessionBundle,
    };

    const PREFERENCES_NAME: &str = "com.legacy.erpx.secure_session";
    const PREFERENCES_KEY: &str = "encrypted_session_bundle_v2";
    const LEGACY_PREFERENCES_KEY: &str = "encrypted_session_v1";
    const KEY_ALIAS: &str = "com.legacy.erpx.session.aes_gcm.v1";
    const BASE64_NO_WRAP: i32 = 2;
    const CIPHER_VERSION: u8 = 1;
    const MIN_GCM_IV_BYTES: usize = 12;
    const MAX_GCM_IV_BYTES: usize = 32;
    const MAX_ENCRYPTED_BYTES: usize = super::MAX_ANDROID_CIPHERTEXT_BYTES;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum StoreError {
        Failed,
        Corrupt,
    }

    impl From<jni::errors::Error> for StoreError {
        fn from(_: jni::errors::Error) -> Self {
            Self::Failed
        }
    }

    impl From<StoreError> for SecureSessionError {
        fn from(error: StoreError) -> Self {
            match error {
                StoreError::Failed => Self::Failed,
                StoreError::Corrupt => Self::Corrupt,
            }
        }
    }

    fn with_attached<F, T>(closure: F) -> Result<T, SecureSessionError>
    where
        F: for<'local> FnOnce(&mut JNIEnv<'local>, JObject<'local>) -> Result<T, StoreError>,
    {
        std::panic::catch_unwind(AssertUnwindSafe(|| bridge_with_attached(closure)))
            .map_err(|_| SecureSessionError::Unavailable)?
            .map_err(Into::into)
    }

    fn string<'local>(
        env: &mut JNIEnv<'local>,
        value: &str,
    ) -> Result<JObject<'local>, StoreError> {
        Ok(env.new_string(value)?.into())
    }

    fn object_call<'local>(
        env: &mut JNIEnv<'local>,
        receiver: &JObject<'local>,
        name: &str,
        signature: &str,
        args: &[JValue<'local, '_>],
    ) -> Result<JObject<'local>, StoreError> {
        Ok(env.call_method(receiver, name, signature, args)?.l()?)
    }

    fn static_call<'local>(
        env: &mut JNIEnv<'local>,
        class: &str,
        name: &str,
        signature: &str,
        args: &[JValue<'local, '_>],
    ) -> Result<JObject<'local>, StoreError> {
        Ok(env.call_static_method(class, name, signature, args)?.l()?)
    }

    fn preferences<'local>(
        env: &mut JNIEnv<'local>,
        context: &JObject<'local>,
    ) -> Result<JObject<'local>, StoreError> {
        let name = string(env, PREFERENCES_NAME)?;
        Ok(object_call(
            env,
            context,
            "getSharedPreferences",
            "(Ljava/lang/String;I)Landroid/content/SharedPreferences;",
            &[JValue::Object(&name), JValue::Int(0)],
        )?)
    }

    fn key_store<'local>(env: &mut JNIEnv<'local>) -> Result<JObject<'local>, StoreError> {
        let provider = string(env, "AndroidKeyStore")?;
        let store = static_call(
            env,
            "java/security/KeyStore",
            "getInstance",
            "(Ljava/lang/String;)Ljava/security/KeyStore;",
            &[JValue::Object(&provider)],
        )?;
        let null = JObject::null();
        env.call_method(
            &store,
            "load",
            "(Ljava/security/KeyStore$LoadStoreParameter;)V",
            &[JValue::Object(&null)],
        )?;
        Ok(store)
    }

    fn ensure_key<'local>(
        env: &mut JNIEnv<'local>,
        store: &JObject<'local>,
    ) -> Result<JObject<'local>, StoreError> {
        let alias = string(env, KEY_ALIAS)?;
        let present = env
            .call_method(
                store,
                "containsAlias",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&alias)],
            )?
            .z()?;
        if !present {
            let purpose_encrypt = env
                .get_static_field(
                    "android/security/keystore/KeyProperties",
                    "PURPOSE_ENCRYPT",
                    "I",
                )?
                .i()?;
            let purpose_decrypt = env
                .get_static_field(
                    "android/security/keystore/KeyProperties",
                    "PURPOSE_DECRYPT",
                    "I",
                )?
                .i()?;
            let builder = env.new_object(
                "android/security/keystore/KeyGenParameterSpec$Builder",
                "(Ljava/lang/String;I)V",
                &[
                    JValue::Object(&alias),
                    JValue::Int(purpose_encrypt | purpose_decrypt),
                ],
            )?;
            let gcm = string(env, "GCM")?;
            let modes = env.new_object_array(1, "java/lang/String", JObject::null())?;
            env.set_object_array_element(&modes, 0, &gcm)?;
            env.call_method(
                &builder,
                "setBlockModes",
                "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
                &[JValue::Object(&modes)],
            )?;
            let no_padding = string(env, "NoPadding")?;
            let paddings = env.new_object_array(1, "java/lang/String", JObject::null())?;
            env.set_object_array_element(&paddings, 0, &no_padding)?;
            env.call_method(
                &builder,
                "setEncryptionPaddings",
                "([Ljava/lang/String;)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
                &[JValue::Object(&paddings)],
            )?;
            env.call_method(
                &builder,
                "setRandomizedEncryptionRequired",
                "(Z)Landroid/security/keystore/KeyGenParameterSpec$Builder;",
                &[JValue::Bool(1)],
            )?;
            let spec = object_call(
                env,
                &builder,
                "build",
                "()Landroid/security/keystore/KeyGenParameterSpec;",
                &[],
            )?;
            let algorithm = string(env, "AES")?;
            let provider = string(env, "AndroidKeyStore")?;
            let generator = static_call(
                env,
                "javax/crypto/KeyGenerator",
                "getInstance",
                "(Ljava/lang/String;Ljava/lang/String;)Ljavax/crypto/KeyGenerator;",
                &[JValue::Object(&algorithm), JValue::Object(&provider)],
            )?;
            env.call_method(
                &generator,
                "init",
                "(Ljava/security/spec/AlgorithmParameterSpec;)V",
                &[JValue::Object(&spec)],
            )?;
            env.call_method(&generator, "generateKey", "()Ljavax/crypto/SecretKey;", &[])?;
        }
        let null = JObject::null();
        let key = object_call(
            env,
            store,
            "getKey",
            "(Ljava/lang/String;[C)Ljava/security/Key;",
            &[JValue::Object(&alias), JValue::Object(&null)],
        )?;
        if key.is_null() {
            return Err(StoreError::Failed);
        }
        Ok(key)
    }

    fn cipher<'local>(env: &mut JNIEnv<'local>) -> Result<JObject<'local>, StoreError> {
        let transformation = string(env, "AES/GCM/NoPadding")?;
        static_call(
            env,
            "javax/crypto/Cipher",
            "getInstance",
            "(Ljava/lang/String;)Ljavax/crypto/Cipher;",
            &[JValue::Object(&transformation)],
        )
    }

    fn bytes<'local>(
        env: &mut JNIEnv<'local>,
        value: JObject<'local>,
    ) -> Result<Vec<u8>, StoreError> {
        if value.is_null() {
            return Err(StoreError::Failed);
        }
        let value: JByteArray<'local> = value.into();
        Ok(env.convert_byte_array(&value)?)
    }

    fn base64_encode<'local>(
        env: &mut JNIEnv<'local>,
        payload: &[u8],
    ) -> Result<String, StoreError> {
        let bytes = env.byte_array_from_slice(payload)?;
        let bytes_object: JObject<'local> = bytes.into();
        let encoded = static_call(
            env,
            "android/util/Base64",
            "encodeToString",
            super::ANDROID_BASE64_ENCODE_SIGNATURE,
            &[JValue::Object(&bytes_object), JValue::Int(BASE64_NO_WRAP)],
        )?;
        Ok(env
            .get_string(&encoded.into())?
            .to_string_lossy()
            .into_owned())
    }

    fn base64_decode<'local>(
        env: &mut JNIEnv<'local>,
        encoded: &str,
    ) -> Result<Vec<u8>, StoreError> {
        if !super::is_valid_android_base64(encoded) {
            return Err(StoreError::Corrupt);
        }
        let encoded = string(env, encoded)?;
        let decoded = static_call(
            env,
            "android/util/Base64",
            "decode",
            "(Ljava/lang/String;I)[B",
            &[JValue::Object(&encoded), JValue::Int(BASE64_NO_WRAP)],
        )?;
        bytes(env, decoded)
    }

    fn encrypt<'local>(
        env: &mut JNIEnv<'local>,
        key: &JObject<'local>,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, StoreError> {
        let cipher = cipher(env)?;
        env.call_method(
            &cipher,
            "init",
            "(ILjava/security/Key;)V",
            &[JValue::Int(1), JValue::Object(key)],
        )?;
        let iv_object = object_call(env, &cipher, "getIV", "()[B", &[])?;
        let iv = bytes(env, iv_object)?;
        if !(MIN_GCM_IV_BYTES..=MAX_GCM_IV_BYTES).contains(&iv.len()) {
            return Err(StoreError::Failed);
        }
        let plaintext = env.byte_array_from_slice(plaintext)?;
        let plaintext_object: JObject<'local> = plaintext.into();
        let ciphertext_object = object_call(
            env,
            &cipher,
            "doFinal",
            "([B)[B",
            &[JValue::Object(&plaintext_object)],
        )?;
        let ciphertext = bytes(env, ciphertext_object)?;
        let mut payload = Vec::with_capacity(2 + iv.len() + ciphertext.len());
        payload.push(CIPHER_VERSION);
        payload.push(iv.len() as u8);
        payload.extend_from_slice(&iv);
        payload.extend_from_slice(&ciphertext);
        if payload.len() > MAX_ENCRYPTED_BYTES {
            return Err(StoreError::Failed);
        }
        Ok(payload)
    }

    fn decrypt<'local>(
        env: &mut JNIEnv<'local>,
        key: &JObject<'local>,
        payload: &[u8],
    ) -> Result<Vec<u8>, StoreError> {
        if payload.len() > MAX_ENCRYPTED_BYTES || payload.len() < 2 {
            return Err(StoreError::Corrupt);
        }
        if payload[0] != CIPHER_VERSION {
            return Err(StoreError::Corrupt);
        }
        let iv_len = payload[1] as usize;
        if !(MIN_GCM_IV_BYTES..=MAX_GCM_IV_BYTES).contains(&iv_len) || payload.len() <= 2 + iv_len {
            return Err(StoreError::Corrupt);
        }
        let iv = &payload[2..2 + iv_len];
        let ciphertext = &payload[2 + iv_len..];
        let cipher = cipher(env)?;
        let iv = env.byte_array_from_slice(iv)?;
        let iv_object: JObject<'local> = iv.into();
        let parameters = env.new_object(
            "javax/crypto/spec/GCMParameterSpec",
            "(I[B)V",
            &[JValue::Int(128), JValue::Object(&iv_object)],
        )?;
        env.call_method(
            &cipher,
            "init",
            "(ILjava/security/Key;Ljava/security/spec/AlgorithmParameterSpec;)V",
            &[
                JValue::Int(2),
                JValue::Object(key),
                JValue::Object(&parameters),
            ],
        )?;
        let ciphertext = env.byte_array_from_slice(ciphertext)?;
        let ciphertext_object: JObject<'local> = ciphertext.into();
        let plaintext_object = object_call(
            env,
            &cipher,
            "doFinal",
            "([B)[B",
            &[JValue::Object(&ciphertext_object)],
        )
        .map_err(|_| StoreError::Corrupt)?;
        bytes(env, plaintext_object).map_err(|_| StoreError::Corrupt)
    }

    fn load_bundle() -> Result<Option<StoredSessionBundle>, SecureSessionError> {
        with_attached(|env, context| {
            let preferences = preferences(env, &context)?;
            let key = string(env, PREFERENCES_KEY)?;
            let mut value = object_call(
                env,
                &preferences,
                "getString",
                "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&key), JValue::Object(&JObject::null())],
            )?;
            if value.is_null() {
                let legacy_key = string(env, LEGACY_PREFERENCES_KEY)?;
                value = object_call(
                    env,
                    &preferences,
                    "getString",
                    "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(&legacy_key),
                        JValue::Object(&JObject::null()),
                    ],
                )?;
            }
            if value.is_null() {
                return Ok(None);
            }
            let encoded = env
                .get_string(&value.into())?
                .to_string_lossy()
                .into_owned();
            let payload = base64_decode(env, &encoded)?;
            let store = key_store(env)?;
            let key = ensure_key(env, &store)?;
            let plaintext = decrypt(env, &key, &payload)?;
            StoredSessionBundle::decode_secure_store(&plaintext)
                .map(Some)
                .map_err(|_| StoreError::Corrupt)
        })
    }

    fn save_bundle(bundle: &StoredSessionBundle) -> Result<(), SecureSessionError> {
        let plaintext = bundle.encode_secure_store()?;
        with_attached(|env, context| {
            let store = key_store(env)?;
            let key = ensure_key(env, &store)?;
            let payload = encrypt(env, &key, &plaintext)?;
            let encoded = base64_encode(env, &payload)?;
            let preferences = preferences(env, &context)?;
            let name = string(env, PREFERENCES_KEY)?;
            let value = string(env, &encoded)?;
            let editor = object_call(
                env,
                &preferences,
                "edit",
                "()Landroid/content/SharedPreferences$Editor;",
                &[],
            )?;
            let editor = object_call(
                env,
                &editor,
                "putString",
                "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&name), JValue::Object(&value)],
            )?;
            let legacy_name = string(env, LEGACY_PREFERENCES_KEY)?;
            let editor = object_call(
                env,
                &editor,
                "remove",
                "(Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&legacy_name)],
            )?;
            let committed = env.call_method(&editor, "commit", "()Z", &[])?.z()?;
            if committed {
                Ok(())
            } else {
                Err(StoreError::Failed)
            }
        })
    }

    pub fn load() -> Result<Option<StoredSession>, SecureSessionError> {
        Ok(load_bundle()?.map(|bundle| bundle.active().clone()))
    }

    pub fn save(session: &StoredSession) -> Result<(), SecureSessionError> {
        let mut bundle =
            load_bundle()?.unwrap_or_else(|| StoredSessionBundle::new(session.clone()));
        bundle.upsert(session.clone());
        save_bundle(&bundle)
    }

    fn delete_bundle() -> Result<(), SecureSessionError> {
        with_attached(|env, context| {
            let preferences = preferences(env, &context)?;
            let key = string(env, PREFERENCES_KEY)?;
            let editor = object_call(
                env,
                &preferences,
                "edit",
                "()Landroid/content/SharedPreferences$Editor;",
                &[],
            )?;
            let editor = object_call(
                env,
                &editor,
                "remove",
                "(Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&key)],
            )?;
            let legacy_key = string(env, LEGACY_PREFERENCES_KEY)?;
            let editor = object_call(
                env,
                &editor,
                "remove",
                "(Ljava/lang/String;)Landroid/content/SharedPreferences$Editor;",
                &[JValue::Object(&legacy_key)],
            )?;
            let committed = env.call_method(&editor, "commit", "()Z", &[])?.z()?;
            if committed {
                Ok(())
            } else {
                Err(StoreError::Failed)
            }
        })
    }

    pub fn clear() -> Result<(), SecureSessionError> {
        let Some(bundle) = load_bundle()? else {
            return delete_bundle();
        };
        let active_id = bundle.active().session_id().clone();
        remove_account(&active_id).map(|_| ())
    }

    pub(super) fn accounts() -> Result<Vec<StoredAccountSummary>, SecureSessionError> {
        Ok(load_bundle()?
            .map(|bundle| bundle.account_summaries())
            .unwrap_or_default())
    }

    pub(super) fn replacement_candidates(
        session: &StoredSession,
    ) -> Result<Vec<StoredSession>, SecureSessionError> {
        let Some(mut bundle) = load_bundle()? else {
            return Ok(Vec::new());
        };
        Ok(bundle.upsert(session.clone()))
    }

    pub(super) fn load_account(
        session_id: &SessionId,
    ) -> Result<Option<StoredSession>, SecureSessionError> {
        Ok(load_bundle()?.and_then(|bundle| bundle.session(session_id).cloned()))
    }

    pub(super) fn remove_account(session_id: &SessionId) -> Result<bool, SecureSessionError> {
        let Some(mut bundle) = load_bundle()? else {
            return Ok(false);
        };
        let removed = bundle.remove(session_id).is_some();
        if !removed {
            return Ok(false);
        }
        if bundle.is_empty() {
            delete_bundle()?;
        } else {
            save_bundle(&bundle)?;
        }
        Ok(true)
    }
}

impl TargetSecureSessionAdapter {
    /// Construct the compile-time target adapter.
    pub const fn new() -> Self {
        Self
    }

    /// Report the target and storage capability without touching storage.
    pub const fn descriptor() -> TargetSessionDescriptor {
        TargetSessionDescriptor {
            target: current_target::TARGET,
            secure_session: compiled_secure_session_support(),
        }
    }
}

impl SecureSessionStore for TargetSecureSessionAdapter {
    fn load(&self) -> SecureSessionFuture<'_, Option<StoredSession>> {
        #[cfg(target_os = "android")]
        {
            Box::pin(async { android_keystore::load() })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            Box::pin(async { apple_keychain::load() })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }

    fn save(&self, session: &StoredSession) -> SecureSessionFuture<'_, ()> {
        #[cfg(target_os = "android")]
        {
            let session = session.clone();
            Box::pin(async move { android_keystore::save(&session) })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            let session = session.clone();
            Box::pin(async move { apple_keychain::save(&session) })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            let _ = session;
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }

    fn clear(&self) -> SecureSessionFuture<'_, ()> {
        #[cfg(target_os = "android")]
        {
            Box::pin(async { android_keystore::clear() })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            Box::pin(async { apple_keychain::clear() })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }
}

impl SecureSessionBundleStore for TargetSecureSessionAdapter {
    fn accounts(&self) -> SecureSessionFuture<'_, Vec<StoredAccountSummary>> {
        #[cfg(target_os = "android")]
        {
            Box::pin(async { android_keystore::accounts() })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            Box::pin(async { apple_keychain::accounts() })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }

    fn replacement_candidates(
        &self,
        session: &StoredSession,
    ) -> SecureSessionFuture<'_, Vec<StoredSession>> {
        let session = session.clone();
        #[cfg(target_os = "android")]
        {
            Box::pin(async move { android_keystore::replacement_candidates(&session) })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            Box::pin(async move { apple_keychain::replacement_candidates(&session) })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            let _ = session;
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }

    fn load_account(
        &self,
        session_id: &SessionId,
    ) -> SecureSessionFuture<'_, Option<StoredSession>> {
        let session_id = session_id.clone();
        #[cfg(target_os = "android")]
        {
            Box::pin(async move { android_keystore::load_account(&session_id) })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            Box::pin(async move { apple_keychain::load_account(&session_id) })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            let _ = session_id;
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }

    fn remove_account(&self, session_id: &SessionId) -> SecureSessionFuture<'_, bool> {
        let session_id = session_id.clone();
        #[cfg(target_os = "android")]
        {
            Box::pin(async move { android_keystore::remove_account(&session_id) })
        }
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        {
            Box::pin(async move { apple_keychain::remove_account(&session_id) })
        }
        #[cfg(not(any(target_os = "ios", target_os = "macos", target_os = "android")))]
        {
            let _ = session_id;
            Box::pin(async { Err(SecureSessionError::Unsupported) })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ANDROID_BASE64_ENCODE_SIGNATURE, ClientTarget, MAX_ANDROID_ENCODED_BYTES,
        SecureSessionSupport, TargetSecureSessionAdapter, is_valid_android_base64,
    };
    use crate::{
        SafeUiText, SecureSessionError, SecureSessionStore, StoredSession, redact_deep_link,
    };
    use std::future::Future;

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = std::task::Waker::noop();
        let mut context = std::task::Context::from_waker(waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(value) => return value,
                std::task::Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn current_target_is_selected_from_an_explicit_target_module() {
        let descriptor = TargetSecureSessionAdapter::descriptor();
        assert!(matches!(
            descriptor.target(),
            ClientTarget::Web
                | ClientTarget::Desktop
                | ClientTarget::Android
                | ClientTarget::Ios
                | ClientTarget::Unsupported
        ));
        assert_eq!(
            descriptor.secure_session(),
            super::compiled_secure_session_support()
        );
        assert!(!descriptor.status_label().contains("plaintext"));
        assert!(!descriptor.local_fallback_allowed());
        if cfg!(any(
            target_os = "ios",
            target_os = "macos",
            target_os = "android"
        )) {
            assert!(
                descriptor
                    .status_copy()
                    .as_str()
                    .contains("Secure session storage is configured")
            );
        } else {
            assert!(
                descriptor
                    .status_copy()
                    .as_str()
                    .contains("no local fallback")
            );
        }
    }

    #[test]
    fn target_descriptor_cannot_advertise_storage_for_an_unsupported_build() {
        let descriptor = TargetSecureSessionAdapter::descriptor();
        let target_supports_storage = matches!(
            descriptor.target(),
            ClientTarget::Android | ClientTarget::Ios
        ) || (matches!(descriptor.target(), ClientTarget::Desktop)
            && cfg!(target_os = "macos"));

        assert_eq!(
            descriptor.secure_session(),
            if target_supports_storage {
                SecureSessionSupport::Supported
            } else {
                SecureSessionSupport::Unsupported
            },
            "target capability must be derived from the compiled target"
        );
        if !target_supports_storage {
            assert_eq!(
                block_on(TargetSecureSessionAdapter::new().load()),
                Err(SecureSessionError::Unsupported)
            );
        }
    }

    #[test]
    fn unsupported_adapter_fails_closed_for_load_save_and_clear() {
        let adapter = TargetSecureSessionAdapter::new();
        let session = StoredSession::new(
            "principal-1",
            "session-1",
            "access-secret",
            "refresh-secret",
            "access-expiry",
            "refresh-expiry",
        )
        .expect("valid test session");
        if cfg!(any(target_os = "ios", target_os = "macos")) {
            // Device/desktop Keychain access is intentionally not exercised by unit tests.
            // Target checks type-check the real adapter; certification runs on the target.
            let _ = session;
        } else {
            assert_eq!(
                block_on(adapter.load()),
                Err(SecureSessionError::Unsupported)
            );
            assert_eq!(
                block_on(adapter.save(&session)),
                Err(SecureSessionError::Unsupported)
            );
            assert_eq!(
                block_on(adapter.clear()),
                Err(SecureSessionError::Unsupported)
            );
        }
    }

    #[test]
    fn secure_session_wire_format_is_bounded_and_revalidated() {
        let session = StoredSession::new(
            "principal-1",
            "session-1",
            "access-secret",
            "refresh-secret",
            "access-expiry",
            "refresh-expiry",
        )
        .expect("valid test session");
        let encoded = session.encode_secure_store().expect("wire encoding");
        assert_eq!(
            StoredSession::decode_secure_store(&encoded).expect("wire decoding"),
            session
        );
        let mut corrupt = encoded.clone();
        corrupt[0] ^= 1;
        assert_eq!(
            StoredSession::decode_secure_store(&corrupt),
            Err(SecureSessionError::Corrupt)
        );
        corrupt = encoded;
        corrupt.push(0);
        assert_eq!(
            StoredSession::decode_secure_store(&corrupt),
            Err(SecureSessionError::Corrupt)
        );
    }

    #[test]
    fn target_boundary_does_not_accept_credential_bearing_return_paths() {
        assert_eq!(
            redact_deep_link("/#/platform/customers?access_token=secret"),
            "/"
        );
        assert_eq!(
            redact_deep_link("/platform/customers#refresh_token=secret"),
            "/"
        );
    }

    #[test]
    fn target_storage_copy_is_bounded_and_redaction_safe() {
        let descriptor = TargetSecureSessionAdapter::descriptor();
        let copy = descriptor.status_copy();
        assert!(copy.as_str().len() <= SafeUiText::MAX_BYTES);
        assert!(!format!("{copy:?}").contains("secret"));
        assert!(!copy.as_str().contains("localStorage"));
        assert!(!descriptor.local_fallback_allowed());
    }

    #[test]
    fn android_secure_storage_rejects_noncanonical_or_unbounded_base64() {
        assert_eq!(
            ANDROID_BASE64_ENCODE_SIGNATURE, "([BI)Ljava/lang/String;",
            "Android Base64 must use the platform's two-argument encodeToString API"
        );
        assert!(is_valid_android_base64("AQID"));
        assert!(is_valid_android_base64("AQI="));
        assert!(is_valid_android_base64("AQ=="));

        for invalid in [
            "AQI",    // missing padding/quantum
            "AQI=\n", // whitespace accepted by permissive platform decoders
            "AQ?D",   // outside the standard alphabet
            "A===",   // more than two padding bytes
            "AB==",   // non-zero discarded bits in a padded quantum
            "AAB=",   // non-zero discarded bits in a padded quantum
        ] {
            assert!(
                !is_valid_android_base64(invalid),
                "expected invalid Android secure-storage payload: {invalid:?}"
            );
        }

        assert!(!is_valid_android_base64(
            &"A".repeat(MAX_ANDROID_ENCODED_BYTES + 4)
        ));
    }
}
