//! ffmpeg encode of captured BGRA. Software fact: capture Wait every frame.
//! Not a ring proof — do not use this path for `make bench` / `make ring`.

use anyhow::{bail, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

pub struct Mp4Writer {
    child: Child,
    stdin: ChildStdin,
    path: PathBuf,
}

impl Mp4Writer {
    pub fn spawn(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        let size = format!("{width}x{height}");
        let fps_s = fps.to_string();
        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "bgra",
                "-s",
                &size,
                "-r",
                &fps_s,
                "-i",
                "-",
                "-an",
                "-c:v",
                "h264_nvenc",
                "-preset",
                "p5",
                "-cq",
                "15",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn ffmpeg (h264_nvenc)")?;
        let stdin = child.stdin.take().context("ffmpeg stdin")?;
        Ok(Self {
            child,
            stdin,
            path: path.to_path_buf(),
        })
    }

    pub fn write_bgra(&mut self, bgra: &[u8]) -> Result<()> {
        self.stdin.write_all(bgra).context("ffmpeg write frame")?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<PathBuf> {
        self.stdin.flush().ok();
        drop(self.stdin);
        let out = self.child.wait_with_output().context("ffmpeg wait")?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            bail!("ffmpeg failed ({}): {err}", out.status);
        }
        Ok(self.path)
    }
}
