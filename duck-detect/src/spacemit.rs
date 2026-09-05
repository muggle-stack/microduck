//! SpaceMIT EP 2.0.6's C ABI (spacemit_ort_env_c_api.h), loaded only on explicit request.
//! No link-time vendor dependency: ARM builds and CPU-only machines stay unchanged.

use std::ffi::{CString, c_char};

use anyhow::{Context, Result};
use ort::{AsPointer, session::builder::SessionBuilder};

use crate::onnx::Options;

type EnvInit = unsafe extern "system" fn(
    *mut ort::sys::OrtSessionOptions,
    *const *const c_char,
    *const *const c_char,
    usize,
) -> ort::sys::OrtStatusPtr;

pub(crate) struct Library {
    ep: libloading::Library,
    // EP finalizers must run while the native ORT C++ symbols are still available.
    _ort_global: libloading::Library,
}

fn provider_options(options: &Options) -> Vec<(&'static str, String)> {
    let mut entries = vec![("SPACEMIT_EP_INTRA_THREAD_NUM", options.threads.to_string())];
    if !options.spacemit_affinity.is_empty() {
        entries.push((
            "SPACEMIT_EP_INTRA_THREAD_AFFINITY",
            options.spacemit_affinity.clone(),
        ));
    }
    if !options.spacemit_allow_fp16_epilogue {
        entries.push(("SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE", "1".into()));
    }
    entries
}

impl Library {
    #[cfg(not(target_os = "linux"))]
    pub(crate) fn open() -> Result<Self> {
        anyhow::bail!("SpaceMIT EP requires Linux and matching native ORT/EP libraries")
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn open() -> Result<Self> {
        use libloading::os::unix::{Library as UnixLibrary, RTLD_GLOBAL, RTLD_NOW};
        use std::path::PathBuf;

        // Match ort's load-dynamic path resolution, including executable-relative libraries.
        let ort_path = std::env::var("ORT_DYLIB_PATH")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "libonnxruntime.so".into());
        let mut ort_path = PathBuf::from(ort_path);
        if !ort_path.is_absolute() {
            let executable = std::env::current_exe()?;
            if let Some(parent) = executable.parent() {
                let relative = parent.join(&ort_path);
                if relative.exists() {
                    ort_path = relative;
                }
            }
        }
        // Load/version-check through ort, but do not replace its environment or policy settings.
        // If another caller already initialized ort from a different library, reject the mix
        // before promoting symbols into the process-wide lookup scope.
        let _ = ort::init_from(&ort_path).context("loading native ORT for SpaceMIT EP")?;
        // SAFETY: a trusted ORT library with the documented OrtGetApiBase C ABI. Compare API
        // identity before any vendor object can be passed between different ORT builds.
        unsafe {
            let runtime = libloading::Library::new(&ort_path)?;
            let get_base = runtime
                .get::<unsafe extern "C" fn() -> *const ort::sys::OrtApiBase>(b"OrtGetApiBase\0")?;
            let base = get_base();
            anyhow::ensure!(!base.is_null(), "native ORT returned no API base");
            let api = ((*base).GetApi)(ort::sys::ORT_API_VERSION);
            anyhow::ensure!(
                std::ptr::eq(api, ort::api()),
                "SpaceMIT EP and Rust must use the same ORT library; check ORT_DYLIB_PATH"
            );
        }
        // SpaceMIT 2.0.6 has undefined ORT C++ symbols and no DT_NEEDED on libonnxruntime.
        // ort's default RTLD_LOCAL is insufficient even if its C API is already initialized.
        // SAFETY: reopen the verified same DLL with RTLD_GLOBAL (also promotes an existing
        // RTLD_LOCAL mapping); retain this handle until after the session and EP drop.
        let ort_global: libloading::Library =
            unsafe { UnixLibrary::open(Some(&ort_path), RTLD_NOW | RTLD_GLOBAL) }
                .with_context(|| {
                    format!("exposing native ORT symbols from {}", ort_path.display())
                })?
                .into();
        let path = std::env::var_os("SPACEMIT_EP_DYLIB_PATH")
            .unwrap_or_else(|| "libspacemit_ep.so".into());
        // SAFETY: the operator supplies a trusted vendor library, just as ORT_DYLIB_PATH does.
        // The Model owns this handle until after its session has dropped.
        let library = unsafe { libloading::Library::new(&path) }
            .with_context(|| format!("cannot load SpaceMIT EP {path:?}; no CPU fallback"))?;
        Ok(Self {
            ep: library,
            _ort_global: ort_global,
        })
    }

    pub(crate) fn register(&self, builder: &mut SessionBuilder, options: &Options) -> Result<()> {
        let entries = provider_options(options);
        let keys = entries
            .iter()
            .map(|(key, _)| CString::new(*key))
            .collect::<Result<Vec<_>, _>>()?;
        let values = entries
            .iter()
            .map(|(_, value)| CString::new(value.as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let key_ptrs: Vec<_> = keys.iter().map(|key| key.as_ptr()).collect();
        let value_ptrs: Vec<_> = values.iter().map(|value| value.as_ptr()).collect();
        // SAFETY: the symbol's ABI is copied from the installed vendor C header. Session options
        // belong to the same native ORT as the EP; arrays and C strings live through this call.
        // status_to_result copies the message and releases OrtStatus, including on failure.
        unsafe {
            let init = self
                .ep
                .get::<EnvInit>(b"OrtSessionOptionsSpaceMITEnvInit\0")
                .context("SpaceMIT EP is missing OrtSessionOptionsSpaceMITEnvInit")?;
            ort::error::status_to_result(init(
                builder.ptr_mut(),
                key_ptrs.as_ptr(),
                value_ptrs.as_ptr(),
                entries.len(),
            ))
            .context("SpaceMIT EP registration failed; no CPU fallback")?;
        }
        tracing::info!(threads = options.threads, affinity = %options.spacemit_affinity,
            allow_fp16_epilogue = options.spacemit_allow_fp16_epilogue,
            "SpaceMIT EP registered (use ORT profiling to verify node execution)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fp16_epilogue_requires_explicit_opt_in() {
        let mut options = Options::default();
        assert!(
            provider_options(&options)
                .contains(&("SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE", "1".into()))
        );
        options.spacemit_allow_fp16_epilogue = true;
        assert!(
            !provider_options(&options)
                .iter()
                .any(|(key, _)| *key == "SPACEMIT_EP_DISABLE_FLOAT16_EPILOGUE")
        );
    }
}
