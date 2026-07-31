//! Narrow, target-owned JNI entry point for ERPX Android adapters.
//!
//! The raw Android context handles are supplied by the Dioxus/NDK runtime. This crate is the
//! only ERPX boundary that turns those handles into a scoped JNI environment; callers still own
//! all domain-specific platform behavior and must not use this bridge as a storage fallback.

#[cfg(target_os = "android")]
pub fn with_attached<F, T, E>(closure: F) -> Result<T, E>
where
    F: for<'local> FnOnce(&mut jni::JNIEnv<'local>, jni::objects::JObject<'local>) -> Result<T, E>,
    E: From<jni::errors::Error>,
{
    // `ndk_context::android_context()` panics when the host has not initialized the Android
    // runtime yet (for example during an early lifecycle callback). Convert that state into the
    // caller's ordinary JNI error so secure-session adapters fail closed instead of unwinding.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let context = ndk_context::android_context();
        // SAFETY: Dioxus initializes ndk-context with the process JavaVM and a live Android
        // Context before an Android client starts. Both handles remain valid for this scoped JNI
        // call.
        let vm = unsafe { jni::JavaVM::from_raw(context.vm().cast())? };
        // SAFETY: ndk-context supplies a live local-compatible Context handle for the Android
        // runtime; it is consumed only during the attached JNI scope below.
        let context = unsafe { jni::objects::JObject::from_raw(context.context().cast()) };
        let mut env = vm.attach_current_thread()?;
        closure(&mut env, context)
    }))
    .unwrap_or_else(|_| Err(E::from(jni::errors::Error::NullPtr("android_context"))))
}

#[cfg(not(target_os = "android"))]
pub fn with_attached<F, T, E>(_closure: F) -> Result<T, E>
where
    F: FnOnce((), ()) -> Result<T, E>,
{
    unreachable!("Android bridge is only available on Android targets")
}
