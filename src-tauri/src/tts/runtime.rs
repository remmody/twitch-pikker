//! The programs the voices run on: the Piper executable, and Python + PyTorch for Silero.
//! Locating them is always local. Installing them happens only on an explicit user request
//! (the setup wizard): Piper arrives through the normal model downloader, Python (Windows,
//! via winget) and PyTorch (into a private venv) through [`RuntimeInstaller`].

use super::download::DlState;
use super::models::{PIPER_EXE_REL, find_executable};
use crate::events::{EVT_RUNTIME_PROGRESS, Emit};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const TORCH_CHECK_TIMEOUT: Duration = Duration::from_secs(60);

fn is_store_stub(p: &Path) -> bool {
    // Windows ships a "python.exe" alias that just opens the Microsoft Store.
    p.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

/// A real system Python (not the Store alias).
pub fn resolve_system_python(configured: &str) -> Option<PathBuf> {
    if let Some(p) = find_executable(configured).filter(|p| !is_store_stub(p)) {
        return Some(p);
    }
    #[cfg(windows)]
    {
        let base =
            std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Programs/Python"))?;
        let mut dirs: Vec<_> = std::fs::read_dir(base)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("Python3"))
            .map(|e| e.path())
            .collect();
        dirs.sort();
        dirs.into_iter()
            .rev()
            .map(|d| d.join("python.exe"))
            .find(|p| p.is_file())
    }
    #[cfg(not(windows))]
    None
}

pub fn venv_dir(models_dir: &Path) -> PathBuf {
    models_dir.join("runtime/venv")
}

pub fn venv_python(models_dir: &Path) -> PathBuf {
    let v = venv_dir(models_dir);
    if cfg!(windows) {
        v.join("Scripts/python.exe")
    } else {
        v.join("bin/python")
    }
}

/// Python used to run Silero: our private venv if it exists, otherwise the system Python.
pub fn resolve_silero_python(configured: &str, models_dir: &Path) -> Option<PathBuf> {
    let v = venv_python(models_dir);
    if v.is_file() {
        Some(v)
    } else {
        resolve_system_python(configured)
    }
}

/// Piper: an explicit path / PATH entry, otherwise the copy installed by the wizard.
pub fn resolve_piper(configured: &str, models_dir: &Path) -> Option<PathBuf> {
    find_executable(configured).or_else(|| {
        let p = models_dir.join(PIPER_EXE_REL);
        p.is_file().then_some(p)
    })
}

fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(not(windows))]
    let _ = cmd;
}

/// Can this Python import PyTorch and NumPy?
pub async fn check_torch(python: &Path) -> bool {
    let mut cmd = Command::new(python);
    cmd.args(["-c", "import torch, numpy"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    hide_window(&mut cmd);
    match tokio::time::timeout(TORCH_CHECK_TIMEOUT, cmd.status()).await {
        Ok(Ok(st)) => st.success(),
        _ => false,
    }
}

/// Extract a `.zip` or `.tar.gz` into `dest`. Both extractors reject paths that escape `dest`.
pub fn extract_archive(archive: &Path, dest: &Path) -> Result<(), String> {
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    if name.ends_with(".zip") {
        zip::ZipArchive::new(file)
            .and_then(|mut z| z.extract(dest))
            .map_err(|e| format!("cannot extract {name}: {e}"))
    } else if name.ends_with(".tar.gz") {
        tar::Archive::new(flate2::read::GzDecoder::new(file))
            .unpack(dest)
            .map_err(|e| format!("cannot extract {name}: {e}"))
    } else {
        Err(format!("unsupported archive type: {name}"))
    }
}

// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeProgress {
    pub state: DlState,
    /// "python" | "pytorch"
    pub step: Option<&'static str>,
    /// Last line of installer output.
    pub line: String,
    pub error: Option<String>,
}

impl RuntimeProgress {
    fn idle() -> Self {
        Self {
            state: DlState::Idle,
            step: None,
            line: String::new(),
            error: None,
        }
    }
}

struct Inner {
    progress: RuntimeProgress,
    token: Option<CancellationToken>,
    generation: u64,
}

pub struct RuntimeInstaller {
    inner: Arc<Mutex<Inner>>,
    emit: Emit,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

enum StepError {
    Cancelled,
    Failed(String),
}

impl RuntimeInstaller {
    pub fn new(emit: Emit) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                progress: RuntimeProgress::idle(),
                token: None,
                generation: 0,
            })),
            emit,
        }
    }

    pub fn progress(&self) -> RuntimeProgress {
        lock(&self.inner).progress.clone()
    }

    pub fn cancel(&self) {
        if let Some(t) = &lock(&self.inner).token {
            t.cancel();
        }
    }

    /// Install Python (Windows only) and/or PyTorch (into a private venv). `on_done` runs after success.
    pub fn start(
        &self,
        rt: &tokio::runtime::Handle,
        python: bool,
        pytorch: bool,
        configured_python: String,
        models_dir: PathBuf,
        on_done: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        if !python && !pytorch {
            return Err("nothing to install".into());
        }
        let (token, generation) = {
            let mut g = lock(&self.inner);
            if g.progress.state == DlState::Running {
                return Err("an install is already running".into());
            }
            let t = CancellationToken::new();
            g.token = Some(t.clone());
            g.generation += 1;
            g.progress = RuntimeProgress {
                state: DlState::Running,
                ..RuntimeProgress::idle()
            };
            (t, g.generation)
        };
        publish(&self.inner, &self.emit);
        let (inner, emit) = (self.inner.clone(), self.emit.clone());
        rt.spawn(async move {
            let upd = |step: Option<&'static str>, line: String| {
                if let Some(snap) = with(&inner, generation, |p| {
                    if step.is_some() {
                        p.step = step;
                    }
                    p.line = line;
                    p.clone()
                }) {
                    crate::events::emit(&emit, EVT_RUNTIME_PROGRESS, &snap);
                }
            };
            let result = run(
                python,
                pytorch,
                &configured_python,
                &models_dir,
                &token,
                &upd,
            )
            .await;
            let ok = result.is_ok();
            with(&inner, generation, |p| match &result {
                Ok(()) => p.state = DlState::Done,
                Err(StepError::Cancelled) => p.state = DlState::Cancelled,
                Err(StepError::Failed(m)) => {
                    (p.state, p.error) = (DlState::Failed, Some(m.clone()))
                }
            });
            if ok {
                on_done();
            }
            publish(&inner, &emit);
        });
        Ok(())
    }
}

fn with<R>(
    inner: &Mutex<Inner>,
    generation: u64,
    f: impl FnOnce(&mut RuntimeProgress) -> R,
) -> Option<R> {
    let mut g = lock(inner);
    (g.generation == generation).then(|| f(&mut g.progress))
}

fn publish(inner: &Mutex<Inner>, emit: &Emit) {
    crate::events::emit(emit, EVT_RUNTIME_PROGRESS, &lock(inner).progress.clone());
}

async fn run(
    python: bool,
    pytorch: bool,
    configured: &str,
    models_dir: &Path,
    token: &CancellationToken,
    upd: &(dyn Fn(Option<&'static str>, String) + Sync),
) -> Result<(), StepError> {
    if python && resolve_system_python(configured).is_none() {
        upd(Some("python"), "Installing Python…".into());
        if !cfg!(windows) {
            return Err(StepError::Failed(
                "Automatic Python install is only available on Windows. Install Python 3.10+ with your package manager, then press Re-check.".into(),
            ));
        }
        let mut cmd = Command::new("winget");
        cmd.args([
            "install",
            "-e",
            "--id",
            "Python.Python.3.12",
            "--scope",
            "user",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
        ]);
        run_step(cmd, token, upd).await?;
        if resolve_system_python(configured).is_none() {
            return Err(StepError::Failed(
                "Python was installed but could not be located. Restart the app and try again."
                    .into(),
            ));
        }
    }
    if pytorch {
        let venv_py = venv_python(models_dir);
        if !venv_py.is_file() {
            let sys = resolve_system_python(configured).ok_or_else(|| {
                StepError::Failed("Python was not found. Install Python first.".into())
            })?;
            upd(
                Some("pytorch"),
                "Creating a private Python environment…".into(),
            );
            let mut cmd = Command::new(sys);
            cmd.arg("-m").arg("venv").arg(venv_dir(models_dir));
            run_step(cmd, token, upd).await?;
        }
        upd(
            Some("pytorch"),
            "Installing PyTorch (CPU) — this can take a few minutes…".into(),
        );
        let mut cmd = Command::new(&venv_py);
        cmd.args([
            "-m",
            "pip",
            "install",
            "--disable-pip-version-check",
            "--no-input",
            "numpy",
            "torch",
            "--index-url",
            "https://download.pytorch.org/whl/cpu",
            "--extra-index-url",
            "https://pypi.org/simple",
        ]);
        run_step(cmd, token, upd).await?;
        if !check_torch(&venv_py).await {
            return Err(StepError::Failed(
                "PyTorch installed but cannot be imported.".into(),
            ));
        }
    }
    Ok(())
}

/// Run a command, streaming its output lines to `upd`; kills it on cancel.
async fn run_step(
    mut cmd: Command,
    token: &CancellationToken,
    upd: &(dyn Fn(Option<&'static str>, String) + Sync),
) -> Result<(), StepError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    hide_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| StepError::Failed(format!("cannot start installer: {e}")))?;
    let (tx, mut rx) = mpsc::channel::<String>(64);
    fn pump<R: AsyncRead + Unpin + Send + 'static>(r: Option<R>, tx: mpsc::Sender<String>) {
        if let Some(r) = r {
            tokio::spawn(async move {
                let mut lines = BufReader::new(r).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    let l = l.trim().to_owned();
                    if !l.is_empty() && tx.send(l).await.is_err() {
                        break;
                    }
                }
            });
        }
    }
    pump(child.stdout.take(), tx.clone());
    pump(child.stderr.take(), tx);

    let mut last = String::new();
    loop {
        tokio::select! {
            _ = token.cancelled() => {
                let _ = child.kill().await;
                return Err(StepError::Cancelled);
            }
            line = rx.recv() => match line {
                Some(l) => { upd(None, l.clone()); last = l; }
                None => break,
            },
        }
    }
    let st = child
        .wait()
        .await
        .map_err(|e| StepError::Failed(e.to_string()))?;
    if st.success() {
        Ok(())
    } else {
        Err(StepError::Failed(format!(
            "installer failed ({st}): {last}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tcr-rt-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn store_alias_is_not_python() {
        assert!(is_store_stub(Path::new(
            "C:/Users/x/AppData/Local/Microsoft/WindowsApps/python.exe"
        )));
        assert!(!is_store_stub(Path::new("C:/Python312/python.exe")));
    }

    #[test]
    fn extracts_zip_and_targz() {
        let d = tmp("extract");
        let zpath = d.join("a.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zpath).unwrap());
            z.start_file("piper/piper.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(b"exe").unwrap();
            z.finish().unwrap();
        }
        extract_archive(&zpath, &d.join("out")).unwrap();
        assert_eq!(
            std::fs::read(d.join("out/piper/piper.exe")).unwrap(),
            b"exe"
        );

        let tpath = d.join("a.tar.gz");
        {
            let enc = flate2::write::GzEncoder::new(
                std::fs::File::create(&tpath).unwrap(),
                flate2::Compression::fast(),
            );
            let mut b = tar::Builder::new(enc);
            let data = b"bin";
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            b.append_data(&mut h, "piper/piper", &data[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        extract_archive(&tpath, &d.join("out2")).unwrap();
        assert_eq!(std::fs::read(d.join("out2/piper/piper")).unwrap(), b"bin");
        assert!(extract_archive(&d.join("x.rar"), &d.join("o")).is_err());
    }

    #[test]
    fn piper_is_found_in_models_dir() {
        let d = tmp("piper");
        assert!(resolve_piper("definitely-not-piper-xyz", &d).is_none());
        let exe = d.join(PIPER_EXE_REL);
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        assert_eq!(resolve_piper("definitely-not-piper-xyz", &d), Some(exe));
    }

    #[tokio::test]
    async fn run_step_reports_failure_and_output() {
        let token = CancellationToken::new();
        let seen = Mutex::new(Vec::new());
        let upd = |_: Option<&'static str>, l: String| lock(&seen).push(l);
        let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        cmd.args(if cfg!(windows) {
            ["/C", "echo hello & exit 3"]
        } else {
            ["-c", "echo hello; exit 3"]
        });
        let r = run_step(cmd, &token, &upd).await;
        assert!(matches!(r, Err(StepError::Failed(m)) if m.contains("hello")));
        assert_eq!(lock(&seen).first().map(String::as_str), Some("hello"));
    }
}
