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

pub(crate) struct Library(libloading::Library);

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
    pub(crate) fn open() -> Result<Self> {
        let path = std::env::var_os("SPACEMIT_EP_DYLIB_PATH")
            .unwrap_or_else(|| "libspacemit_ep.so".into());
        // SAFETY: the operator supplies a trusted vendor library, just as ORT_DYLIB_PATH does.
        // The Model owns this handle until after its session has dropped.
        let library = unsafe { libloading::Library::new(&path) }
            .with_context(|| format!("cannot load SpaceMIT EP {path:?}; no CPU fallback"))?;
        Ok(Self(library))
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
                .0
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
