use crate::domain::preview::{
    PREVIEW_EDGE, PREVIEW_INPUT_LIMIT, PREVIEW_OUTPUT_LIMIT, PREVIEW_PIXEL_LIMIT, PreviewArtifact,
    PreviewFailure as Failure, PreviewInput, PreviewMedia, PreviewResult, PreviewSourceMedia,
};
use erpx_server_runtime::public::owned_process::{
    OwnedProcessLimits, apply_owned_process_resource_limits,
};
use libvips::{
    VipsApp, VipsImage,
    ops::{self, Access, IccTransformOptions, Intent, Interpretation, ResizeOptions},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewToolConfiguration {
    heif_convert: PathBuf,
    pdftoppm: PathBuf,
}
impl PreviewToolConfiguration {
    pub fn new(heif_convert: &Path, pdftoppm: &Path) -> Result<Self, Failure> {
        Ok(Self {
            heif_convert: tool_path(heif_convert)?,
            pdftoppm: tool_path(pdftoppm)?,
        })
    }
    pub fn heif_convert(&self) -> &Path {
        &self.heif_convert
    }
    pub fn pdftoppm(&self) -> &Path {
        &self.pdftoppm
    }
    pub(crate) fn validate(&self) -> Result<(), Failure> {
        if tool_path(&self.heif_convert)? != self.heif_convert
            || tool_path(&self.pdftoppm)? != self.pdftoppm
        {
            return Err(Failure::ToolConfiguration);
        }
        Ok(())
    }
}
fn tool_path(path: &Path) -> Result<PathBuf, Failure> {
    if !path.is_absolute() {
        return Err(Failure::ToolConfiguration);
    }
    let p = path
        .canonicalize()
        .map_err(|_| Failure::ToolConfiguration)?;
    let m = fs::metadata(&p).map_err(|_| Failure::ToolConfiguration)?;
    if !m.is_file() {
        return Err(Failure::ToolConfiguration);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if m.permissions().mode() & 0o111 == 0 {
            return Err(Failure::ToolConfiguration);
        }
    }
    Ok(p)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreviewChildRequest {
    pub schema: u8,
    pub media: PreviewSourceMedia,
    pub checksum_sha256: String,
    pub tools: PreviewToolConfiguration,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum PreviewChildResponse {
    Rendered {
        source_checksum_sha256: String,
        checksum_sha256: String,
        media: PreviewMedia,
        width: u32,
        height: u32,
        libvips: String,
    },
    NoPreview {
        source_checksum_sha256: String,
    },
    Failed {
        failure: Failure,
    },
}
pub(crate) fn process_limits() -> Result<OwnedProcessLimits, Failure> {
    OwnedProcessLimits::new(
        Duration::from_secs(40),
        1024 * 1024,
        1024 * 1024 * 1024,
        128 * 1024 * 1024,
    )
    .map_err(|_| Failure::ResourceLimit)
}
/// Dedicated inbound Rust CLI adapter only. The parent supplies its private
/// fixed-name directory; the request never accepts arbitrary source paths/URLs.
pub fn run_preview_child(input: &Path, request: &Path) -> Result<(), Failure> {
    let execute = || {
        apply_owned_process_resource_limits(process_limits()?)
            .map_err(|_| Failure::ResourceLimit)?;
        // This runs only in the dedicated child. Native-created output/cache
        // files inherit private permissions without changing the parent umask.
        nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o077));
        let cwd = std::env::current_dir().map_err(|_| Failure::WorkerProtocol)?;
        if input != cwd.join("input") || request != cwd.join("request.json") {
            return Err(Failure::WorkerProtocol);
        }
        child(&cwd, input, request)
    };
    let response = match execute() {
        Ok(r) => r,
        Err(failure) => PreviewChildResponse::Failed { failure },
    };
    let bytes = serde_json::to_vec(&response).map_err(|_| Failure::WorkerProtocol)?;
    if bytes.len() > 4096 {
        return Err(Failure::WorkerProtocol);
    }
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|_| Failure::WorkerProtocol)
}
fn child(cwd: &Path, input: &Path, request: &Path) -> Result<PreviewChildResponse, Failure> {
    let request: PreviewChildRequest = serde_json::from_slice(&read_regular(request, 64 * 1024)?)
        .map_err(|_| Failure::WorkerProtocol)?;
    if request.schema != 1 {
        return Err(Failure::WorkerProtocol);
    }
    request.tools.validate()?;
    let bytes = read_regular(input, PREVIEW_INPUT_LIMIT)?;
    let source = PreviewInput::new(bytes, request.media, &request.checksum_sha256)?;
    if !source.media().needs_preview() {
        return Ok(PreviewChildResponse::NoPreview {
            source_checksum_sha256: source.checksum_sha256().into(),
        });
    }
    let app = VipsApp::new("erpx-files-preview", false).map_err(|_| Failure::WorkerUnavailable)?;
    if app
        .version_string()
        .map_err(|_| Failure::WorkerUnavailable)?
        != "8.18.7"
    {
        return Err(Failure::ToolConfiguration);
    }
    app.concurrency_set(1);
    app.cache_set_max(0);
    app.cache_set_max_mem(64 * 1024 * 1024);
    app.cache_set_max_files(8);
    let media = if source.media() == PreviewSourceMedia::Pdf {
        native_tool(
            &request.tools.pdftoppm,
            &[
                "-f",
                "1",
                "-l",
                "1",
                "-singlefile",
                "-scale-to",
                "2400",
                "-jpeg",
                "input",
                "preview",
            ],
            cwd,
        )?;
        PreviewMedia::Jpeg
    } else {
        let decoded = if matches!(
            source.media(),
            PreviewSourceMedia::Heic | PreviewSourceMedia::Heif
        ) {
            let metadata = load_image("input", source.media())?;
            require_pixels(&metadata)?;
            drop(metadata);
            native_tool(&request.tools.heif_convert, &["input", "decoded.png"], cwd)?;
            let size =
                fs::symlink_metadata(cwd.join("decoded.png")).map_err(|_| Failure::DecodedSize)?;
            if !size.is_file() || size.len() == 0 || size.len() > 100 * 1024 * 1024 {
                return Err(Failure::DecodedSize);
            }
            "decoded.png"
        } else {
            "input"
        };
        let decoded_media = if decoded == "decoded.png" {
            PreviewSourceMedia::Png
        } else {
            source.media()
        };
        render_image(&app, decoded, decoded_media)?;
        PreviewMedia::Webp
    };
    let name = match media {
        PreviewMedia::Jpeg => "preview.jpg",
        PreviewMedia::Webp => "preview.webp",
    };
    let artifact = PreviewArtifact::from_rendered(
        read_regular(&cwd.join(name), PREVIEW_OUTPUT_LIMIT)?,
        media,
    )?;
    let result = PreviewResult::Rendered(artifact);
    result.require_source_kind(source.media())?;
    let PreviewResult::Rendered(artifact) = result else {
        return Err(Failure::WorkerProtocol);
    };
    Ok(PreviewChildResponse::Rendered {
        source_checksum_sha256: source.checksum_sha256().into(),
        checksum_sha256: artifact.checksum_sha256().into(),
        media,
        width: artifact.width(),
        height: artifact.height(),
        libvips: app
            .version_string()
            .map_err(|_| Failure::WorkerUnavailable)?
            .into(),
    })
}
fn load_image(file: &str, media: PreviewSourceMedia) -> Result<VipsImage, Failure> {
    // Fixed filenames only. Loader options are not concatenated with request data.
    let _ = media;
    VipsImage::new_from_file_access(
        &format!("{file}[fail_on=warning]"),
        Access::Sequential,
        false,
    )
    .map_err(|_| Failure::DecoderRejected)
}
fn require_pixels(image: &VipsImage) -> Result<(), Failure> {
    let (w, h) = (image.get_width(), image.get_height());
    if w < 1 || h < 1 || w as u64 * h as u64 > PREVIEW_PIXEL_LIMIT {
        return Err(Failure::PixelLimit);
    }
    Ok(())
}
/// Preserve B0's loader scaling, ICC-before-resize, alpha premultiplication,
/// EXIF orientation, final sRGB and WebP encoder settings. No custom codec.
fn render_image(app: &VipsApp, file: &str, media: PreviewSourceMedia) -> Result<(), Failure> {
    let mut image = load_image(file, media)?;
    require_pixels(&image)?;
    let shrink = shrink_factor(&image);
    if media == PreviewSourceMedia::Jpeg {
        let mut factor = if shrink >= 8. {
            8
        } else if shrink >= 4. {
            4
        } else if shrink >= 2. {
            2
        } else {
            1
        };
        if factor > 1 && shrink as i32 == factor {
            factor /= 2;
        }
        if factor > 1 {
            // The generated libvips binding passes the loader's output-only
            // flags property as an input vararg, which can crash on Linux.
            // The filename is a fixed private name and factor is computed above.
            image = VipsImage::new_from_file_access(
                &format!("{file}[shrink={factor},fail_on=warning]"),
                Access::Sequential,
                false,
            )
            .map_err(|_| Failure::DecoderRejected)?;
        }
    } else if media == PreviewSourceMedia::Webp && shrink > 1. {
        image = VipsImage::new_from_file_access(
            &format!("{file}[scale={},fail_on=warning]", 1. / shrink),
            Access::Sequential,
            false,
        )
        .map_err(|_| Failure::DecoderRejected)?;
    }
    let interpretation = image
        .get_interpretation()
        .map_err(|_| Failure::DecoderRejected)?;
    let profile = if matches!(interpretation, Interpretation::Rgb16) {
        "p3"
    } else {
        "srgb"
    };
    if image.get_as_string("icc-profile-data").is_ok()
        && !matches!(
            interpretation,
            Interpretation::Labs | Interpretation::Grey16
        )
    {
        match ops::icc_transform_with_opts(
            &image,
            profile,
            &IccTransformOptions {
                embedded: true,
                intent: Intent::Perceptual,
                depth: if matches!(interpretation, Interpretation::Rgb16) {
                    16
                } else {
                    8
                },
                ..Default::default()
            },
        ) {
            Ok(transformed) => image = transformed,
            Err(_) => app.error_clear(),
        }
        // B0 explicitly tolerates a bad embedded ICC transform and proceeds
        // with its colourspace fallback; retained independent oracle covers it.
    } else if matches!(interpretation, Interpretation::Cmyk) {
        image = ops::icc_transform_with_opts(
            &image,
            profile,
            &IccTransformOptions {
                input_profile: Some("cmyk".into()),
                intent: Intent::Perceptual,
                ..Default::default()
            },
        )
        .map_err(|_| Failure::DecoderRejected)?;
    }
    let shrink = shrink_factor(&image);
    let format = image.get_format().map_err(|_| Failure::DecoderRejected)?;
    let alpha = image.image_hasalpha() && shrink != 1.;
    if alpha {
        image = ops::cast(
            &ops::premultiply(&image).map_err(|_| Failure::DecoderRejected)?,
            format,
        )
        .map_err(|_| Failure::DecoderRejected)?;
    }
    if shrink != 1. {
        image = ops::resize_with_opts(
            &image,
            1. / shrink,
            &ResizeOptions {
                vscale: 1. / shrink,
                ..Default::default()
            },
        )
        .map_err(|_| Failure::DecoderRejected)?;
    }
    image = ops::autorot(&image).map_err(|_| Failure::DecoderRejected)?;
    if alpha {
        image = ops::cast(
            &ops::unpremultiply(&image).map_err(|_| Failure::DecoderRejected)?,
            format,
        )
        .map_err(|_| Failure::DecoderRejected)?;
    }
    image = ops::colourspace(&image, Interpretation::Srgb).map_err(|_| Failure::DecoderRejected)?;
    // Generated WebpsaveOptions always sets nullable profile, which forces ICC
    // retention in this binding. The vetted high-level writer accepts these
    // fixed saver options without setting that argument at all.
    image
        .image_write_to_file("preview.webp[Q=82,effort=4,keep=none]")
        .map_err(|_| Failure::DecoderRejected)
}
fn shrink_factor(image: &VipsImage) -> f64 {
    (image.get_width() as f64 / PREVIEW_EDGE as f64)
        .max(image.get_height() as f64 / PREVIEW_EDGE as f64)
        .max(1.)
}
pub(crate) fn read_regular(path: &Path, limit: usize) -> Result<Vec<u8>, Failure> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| Failure::WorkerProtocol)?;
    let metadata = file.metadata().map_err(|_| Failure::WorkerProtocol)?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err(Failure::OutputSize);
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::WorkerProtocol)?;
    if bytes.len() > limit {
        return Err(Failure::OutputSize);
    }
    Ok(bytes)
}
fn native_tool(binary: &Path, args: &[&str], directory: &Path) -> Result<(), Failure> {
    let mut child = Command::new(binary)
        .args(args)
        .current_dir(directory)
        .env_clear()
        .env("LANG", "C")
        .env("TZ", "UTC")
        .env("TMPDIR", directory)
        .env("OMP_NUM_THREADS", "1")
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| Failure::WorkerUnavailable)?;
    let counter = Arc::new(AtomicUsize::new(0));
    let exceeded = Arc::new(AtomicBool::new(false));
    let read =
        |stream: Box<dyn Read + Send>, counter: Arc<AtomicUsize>, exceeded: Arc<AtomicBool>| {
            thread::spawn(move || {
                let mut stream = stream;
                let mut buffer = [0; 8192];
                loop {
                    let count = match stream.read(&mut buffer) {
                        Ok(0) => return,
                        Ok(n) => n,
                        Err(_) => {
                            exceeded.store(true, Ordering::Release);
                            return;
                        }
                    };
                    if counter
                        .fetch_add(count, Ordering::Relaxed)
                        .saturating_add(count)
                        > 1024 * 1024
                    {
                        exceeded.store(true, Ordering::Release);
                        return;
                    }
                }
            })
        };
    let out = child.stdout.take().ok_or(Failure::WorkerProtocol)?;
    let err = child.stderr.take().ok_or(Failure::WorkerProtocol)?;
    let out = read(Box::new(out), counter.clone(), exceeded.clone());
    let err = read(Box::new(err), counter, exceeded.clone());
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut failure = None;
    let status = loop {
        if exceeded.load(Ordering::Acquire) {
            failure = Some(Failure::WorkerOutput);
            let _ = child.kill();
        }
        if Instant::now() >= deadline {
            failure = Some(Failure::Deadline);
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::DecoderRejected);
            }
        }
    };
    let _ = out.join();
    let _ = err.join();
    if let Some(f) = failure {
        return Err(f);
    }
    if !status.success() {
        return Err(Failure::DecoderRejected);
    }
    Ok(())
}
