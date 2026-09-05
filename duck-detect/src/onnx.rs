//! ONNX detector using the board's native ORT, on CPU or an explicitly selected SpaceMIT EP.
//! The default remains CPU/two threads; neither backend changes the RGB/YOLO contract.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::tensor::TensorElementType;
use ort::value::{Tensor, ValueType};

use crate::spacemit::Library;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Provider {
    #[default]
    Cpu,
    Spacemit,
}

/// Options are per detector, not a global ORT environment setting: policies stay on CPU.
#[derive(Debug, Clone)]
pub struct Options {
    pub provider: Provider,
    /// ORT intra-op threads and, when selected, the EP's separate intra-op setting.
    pub threads: usize,
    /// EP worker CPU IDs, e.g. `0;1;2;3`. Empty leaves affinity to the provider.
    pub spacemit_affinity: String,
    /// Opt-in precision change. EP 2.0.6's INT8 path requires this; see the K1 report.
    pub spacemit_allow_fp16_epilogue: bool,
    /// Profiling is off in the daemon. The offline benchmark can enable it separately.
    pub profile: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            provider: Provider::Cpu,
            // Leave CPU time for robotd's control loop and GStreamer.
            threads: 2,
            spacemit_affinity: String::new(),
            spacemit_allow_fp16_epilogue: false,
            profile: None,
        }
    }
}

impl Options {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=256).contains(&self.threads),
            "ONNX threads must be in 1..=256"
        );
        if self.provider == Provider::Spacemit && !self.spacemit_affinity.is_empty() {
            let cores: Vec<_> = self.spacemit_affinity.split(';').collect();
            ensure!(
                cores.len() == self.threads && cores.iter().all(|s| s.parse::<u32>().is_ok()),
                "SpaceMIT affinity must contain one CPU ID per thread, separated by ';'"
            );
        }
        Ok(())
    }
}

pub struct Model {
    // Field drop order matters: the session must release all EP objects before dlclose.
    session: Session,
    _ep_library: Option<Library>,
    /// `[height, width, channels]`, as the graph declares it.
    pub input: (usize, usize, usize),
    input_len: usize,
    output_len: usize,
}

fn input_shape(shape: &[i64]) -> Result<(usize, usize, usize)> {
    match shape {
        [1, 3, h, w] if *h > 0 && *w > 0 => Ok((*h as usize, *w as usize, 3)),
        other => bail!("expected a static [1, 3, H, W] input, got {other:?}"),
    }
}

fn output_len(shape: &[i64]) -> Result<usize> {
    match shape {
        [1, 5, n] if *n > 0 => (*n as usize).checked_mul(5).context("output size overflow"),
        other => bail!("expected a one-class [1, 5, N] output, got {other:?}"),
    }
}

fn float_shape(dtype: &ValueType) -> Result<&[i64]> {
    match dtype {
        ValueType::Tensor {
            ty: TensorElementType::Float32,
            shape,
            ..
        } => Ok(shape),
        other => bail!("expected an f32 tensor, got {other:?}"),
    }
}

impl Model {
    pub fn open(path: &Path) -> Result<Self> {
        Self::open_with_options(path, &Options::default())
    }

    pub fn open_with_options(path: &Path, options: &Options) -> Result<Self> {
        options.validate()?;
        // Declared before the builder so error unwinding drops session options before the DLL.
        let ep_library = match options.provider {
            Provider::Cpu => None,
            Provider::Spacemit => Some(Library::open()?),
        };
        let mut builder = Session::builder()
            .context("ort session builder")?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(options.threads)?
            .with_inter_threads(1)?;
        if let Some(library) = &ep_library {
            library.register(&mut builder, options)?;
            // A requested EP must not become a successful-looking CPU fallback. Operators
            // should use duck-bench --profile-prefix to verify actual EP node execution.
            builder = builder.with_config_entry("session.disable_cpu_ep_fallback", "1")?;
        }
        if let Some(prefix) = &options.profile {
            builder = builder.with_profiling(prefix)?;
        }
        let session = builder.commit_from_file(path).with_context(|| {
            format!("cannot load {} with {:?}", path.display(), options.provider)
        })?;
        ensure!(
            session.inputs().len() == 1 && session.outputs().len() == 1,
            "detector requires exactly one input and one output"
        );
        ensure!(
            session.inputs()[0].name() == "images",
            "detector input must be named images"
        );
        let input = input_shape(float_shape(session.inputs()[0].dtype())?)?;
        let input_len = input
            .0
            .checked_mul(input.1)
            .and_then(|n| n.checked_mul(input.2))
            .context("input size overflow")?;
        let output_len = output_len(float_shape(session.outputs()[0].dtype())?)?;
        Ok(Self {
            session,
            _ep_library: ep_library,
            input,
            input_len,
            output_len,
        })
    }

    pub fn end_profiling(&mut self) -> Result<String> {
        self.session.end_profiling().context("ending ORT profile")
    }

    /// Letterboxed HWC RGB bytes in, the same `[1,5,N]` head as the RKNN path out.
    pub fn infer(&mut self, frame: &[u8], out: &mut Vec<f32>) -> Result<()> {
        let (height, width, channels) = self.input;
        ensure!(
            frame.len() == self.input_len,
            "frame is {} bytes, the model wants {}",
            frame.len(),
            self.input_len
        );

        // Keep the upstream preprocessing arithmetic: NCHW f32, RGB, divided by 255.
        let mut planar = vec![0.0f32; frame.len()];
        for y in 0..height {
            for x in 0..width {
                for c in 0..channels {
                    planar[c * height * width + y * width + x] =
                        frame[(y * width + x) * channels + c] as f32 / 255.0;
                }
            }
        }
        let tensor = Tensor::from_array((
            [1_usize, channels, height, width],
            planar.into_boxed_slice(),
        ))
        .context("building the input tensor")?;
        let outputs = self
            .session
            .run(ort::inputs!["images" => tensor])
            .context("inference failed")?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .context("the output is not f32")?;
        ensure!(
            output_len(shape)? == self.output_len && data.len() == self.output_len,
            "detector output shape changed at runtime"
        );
        ensure!(
            data.iter().all(|x| x.is_finite()),
            "non-finite detector output"
        );
        out.clear();
        out.extend_from_slice(data);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_configuration_stays_cpu_two_threads() {
        let options = Options::default();
        assert_eq!(options.provider, Provider::Cpu);
        assert_eq!(options.threads, 2);
        assert!(!options.spacemit_allow_fp16_epilogue);
        assert!(options.profile.is_none());
    }

    #[test]
    fn reject_invalid_threads_and_ep_affinity() {
        let mut options = Options {
            threads: 0,
            ..Options::default()
        };
        assert!(options.validate().is_err());
        options.threads = 2;
        options.provider = Provider::Spacemit;
        options.spacemit_affinity = "0;1".into();
        assert!(options.validate().is_ok());
        for affinity in ["0", "0;-1", "0;x", "0;1;"] {
            options.spacemit_affinity = affinity.into();
            assert!(options.validate().is_err());
        }
    }

    #[test]
    fn refuse_dynamic_wrong_batch_or_coco_contracts() {
        assert_eq!(input_shape(&[1, 3, 320, 320]).unwrap(), (320, 320, 3));
        for shape in [
            [2, 3, 320, 320],
            [1, 3, -1, -1],
            [1, 3, 0, 320],
            [1, 320, 320, 3],
        ] {
            assert!(input_shape(&shape).is_err());
        }
        assert_eq!(output_len(&[1, 5, 2100]).unwrap(), 10500);
        for shape in [
            [1, 84, 2100],
            [1, 2100, 5],
            [1, 5, -1],
            [1, 5, 0],
            [2, 5, 2100],
        ] {
            assert!(output_len(&shape).is_err());
        }
    }
}
