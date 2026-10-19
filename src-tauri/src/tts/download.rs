//! Resumable, cancellable model downloader. **Only ever started by an explicit user action**
//! (the setup wizard / Settings button); nothing here runs on startup or during synthesis.
//!
//! Partial files live in `<models>/.partial/*.part`. Pause keeps them (resume continues with an
//! HTTP `Range` request), cancel deletes them, failure keeps them for `Retry`.

use super::models::{self, PackFile};
use crate::events::{EVT_MODELS_PROGRESS, Emit};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

const STOP_NONE: u8 = 0;
const STOP_PAUSE: u8 = 1;
const STOP_CANCEL: u8 = 2;
const EMIT_EVERY: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DlState {
    Idle,
    Running,
    Paused,
    Failed,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileProgress {
    pub name: String,
    pub received: u64,
    pub total: u64,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub state: DlState,
    pub error: Option<String>,
    pub files: Vec<FileProgress>,
    pub received: u64,
    pub total: u64,
    pub speed_bps: u64,
}

impl DownloadProgress {
    fn idle() -> Self {
        Self {
            state: DlState::Idle,
            error: None,
            files: vec![],
            received: 0,
            total: 0,
            speed_bps: 0,
        }
    }

    fn recompute(&mut self) {
        self.received = self.files.iter().map(|f| f.received).sum();
        self.total = self.files.iter().map(|f| f.total).sum();
    }
}

#[derive(Debug)]
pub enum DlError {
    /// Paused or cancelled by the user.
    Stopped,
    Failed(String),
}

fn fail(msg: impl std::fmt::Display) -> DlError {
    DlError::Failed(msg.to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(h.finalize().as_slice()))
}

/// Download one file to `final_path`, resuming from `part_path` if it exists.
/// `progress(received, total)` is called as bytes arrive (`total` 0 = unknown).
#[allow(clippy::too_many_arguments)]
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
    part_path: &Path,
    final_path: &Path,
    token: &CancellationToken,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<(), DlError> {
    if let Some(dir) = part_path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(fail)?;
    }
    let mut have = tokio::fs::metadata(part_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if expected_size.is_some_and(|s| have > s) {
        let _ = tokio::fs::remove_file(part_path).await;
        have = 0;
    }

    if expected_size != Some(have) || have == 0 {
        let mut req = client.get(url);
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = tokio::select! {
            _ = token.cancelled() => return Err(DlError::Stopped),
            r = req.send() => r.map_err(|e| fail(format!("network error: {e}")))?,
        };
        let status = resp.status();
        let append = match status.as_u16() {
            206 => true,
            200 => false,
            416 => {
                // Our partial file doesn't match the server's: start clean on retry.
                let _ = tokio::fs::remove_file(part_path).await;
                return Err(fail(
                    "server rejected the resume request; press Retry to restart this file",
                ));
            }
            code => return Err(fail(format!("server returned HTTP {code}"))),
        };
        if !append {
            have = 0;
        }
        let total = expected_size
            .or_else(|| resp.content_length().map(|l| l + have))
            .unwrap_or(0);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(part_path)
            .await
            .map_err(fail)?;
        let mut stream = resp.bytes_stream();
        progress(have, total);
        loop {
            let chunk = tokio::select! {
                _ = token.cancelled() => {
                    let _ = file.flush().await;
                    return Err(DlError::Stopped);
                }
                c = stream.next() => c,
            };
            match chunk {
                None => break,
                Some(Err(e)) => {
                    let _ = file.flush().await;
                    return Err(fail(format!("download interrupted: {e}")));
                }
                Some(Ok(bytes)) => {
                    file.write_all(&bytes).await.map_err(fail)?;
                    have += bytes.len() as u64;
                    progress(have, total.max(have));
                }
            }
        }
        file.flush().await.map_err(fail)?;
    }

    // Verify before the file becomes visible to the rest of the app.
    let invalid = |why: String, part: &Path| {
        let _ = std::fs::remove_file(part);
        fail(why)
    };
    let len = tokio::fs::metadata(part_path).await.map_err(fail)?.len();
    if expected_size.is_some_and(|s| s != len) {
        return Err(invalid(
            format!("size mismatch (got {len} bytes); the partial file was discarded"),
            part_path,
        ));
    }
    if let Some(want) = expected_sha256 {
        let p = part_path.to_path_buf();
        let got = tokio::task::spawn_blocking(move || sha256_file(&p))
            .await
            .map_err(fail)?
            .map_err(fail)?;
        if !got.eq_ignore_ascii_case(want) {
            return Err(invalid(
                "checksum mismatch; the downloaded file was discarded".into(),
                part_path,
            ));
        }
    }
    if final_path.extension().is_some_and(|e| e == "json") {
        let text = tokio::fs::read(part_path).await.map_err(fail)?;
        if serde_json::from_slice::<serde_json::Value>(&text).is_err() {
            return Err(invalid(
                "downloaded config is not valid JSON".into(),
                part_path,
            ));
        }
    }
    if let Some(dir) = final_path.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(fail)?;
    }
    let _ = tokio::fs::remove_file(final_path).await;
    tokio::fs::rename(part_path, final_path)
        .await
        .map_err(fail)?;
    Ok(())
}

struct Inner {
    progress: DownloadProgress,
    token: Option<CancellationToken>,
    packs: Vec<String>,
    dir: PathBuf,
    generation: u64,
}

pub struct Downloader {
    inner: Arc<Mutex<Inner>>,
    stop: Arc<AtomicU8>,
    emit: Emit,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Downloader {
    pub fn new(emit: Emit) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                progress: DownloadProgress::idle(),
                token: None,
                packs: vec![],
                dir: PathBuf::new(),
                generation: 0,
            })),
            stop: Arc::new(AtomicU8::new(STOP_NONE)),
            emit,
        }
    }

    pub fn progress(&self) -> DownloadProgress {
        lock(&self.inner).progress.clone()
    }

    /// Begin downloading the chosen packs into `dir`. `on_done` runs after a fully successful run.
    pub fn start(
        &self,
        rt: &tokio::runtime::Handle,
        pack_ids: Vec<String>,
        dir: PathBuf,
        on_done: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        let mut files: Vec<&'static PackFile> = Vec::new();
        for id in &pack_ids {
            let p = models::pack(id).ok_or_else(|| format!("unknown model pack '{id}'"))?;
            if models::pack_installed(p, &dir) {
                continue;
            }
            // Archive packs (marker) are all-or-nothing; voice packs resume per file.
            files.extend(
                p.files
                    .iter()
                    .filter(|f| p.marker.is_some() || !models::pack_installed_file(f, &dir)),
            );
        }
        if pack_ids.is_empty() {
            return Err("select at least one voice pack".into());
        }
        let (token, generation) = {
            let mut g = lock(&self.inner);
            if g.progress.state == DlState::Running {
                return Err("a download is already running".into());
            }
            let token = CancellationToken::new();
            g.token = Some(token.clone());
            g.generation += 1;
            g.packs = pack_ids;
            g.dir = dir.clone();
            g.progress = DownloadProgress {
                state: DlState::Running,
                error: None,
                files: files
                    .iter()
                    .map(|f| FileProgress {
                        name: file_name(f.rel),
                        received: 0,
                        total: f.size.unwrap_or(0),
                        done: false,
                    })
                    .collect(),
                received: 0,
                total: 0,
                speed_bps: 0,
            };
            g.progress.recompute();
            (token, g.generation)
        };
        self.stop.store(STOP_NONE, Ordering::SeqCst);
        self.publish();
        let (inner, stop, emit) = (self.inner.clone(), self.stop.clone(), self.emit.clone());
        rt.spawn(run(
            inner, stop, emit, generation, token, files, dir, on_done,
        ));
        Ok(())
    }

    /// Continue a paused or failed download with the same packs.
    pub fn resume(
        &self,
        rt: &tokio::runtime::Handle,
        on_done: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), String> {
        let (packs, dir) = {
            let g = lock(&self.inner);
            if !matches!(g.progress.state, DlState::Paused | DlState::Failed) {
                return Err("nothing to resume".into());
            }
            (g.packs.clone(), g.dir.clone())
        };
        self.start(rt, packs, dir, on_done)
    }

    /// Stop but keep partial files.
    pub fn pause(&self) {
        self.stop.store(STOP_PAUSE, Ordering::SeqCst);
        if let Some(t) = &lock(&self.inner).token {
            t.cancel();
        }
    }

    /// Stop and delete partial files.
    pub fn cancel(&self) {
        self.stop.store(STOP_CANCEL, Ordering::SeqCst);
        let (running, dir) = {
            let mut g = lock(&self.inner);
            if let Some(t) = &g.token {
                t.cancel();
            }
            let running = g.progress.state == DlState::Running;
            if !running {
                g.progress = DownloadProgress {
                    state: DlState::Cancelled,
                    ..DownloadProgress::idle()
                };
            }
            (running, g.dir.clone())
        };
        if !running {
            let _ = std::fs::remove_dir_all(dir.join(".partial"));
            self.publish();
        }
    }

    fn publish(&self) {
        (self.emit)(
            EVT_MODELS_PROGRESS,
            serde_json::to_value(self.progress()).unwrap_or_default(),
        );
    }
}

fn file_name(rel: &str) -> String {
    Path::new(rel)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(rel)
        .to_owned()
}

fn with<R>(
    inner: &Mutex<Inner>,
    generation: u64,
    f: impl FnOnce(&mut DownloadProgress) -> R,
) -> Option<R> {
    let mut g = lock(inner);
    (g.generation == generation).then(|| f(&mut g.progress))
}

#[allow(clippy::too_many_arguments)]
async fn run(
    inner: Arc<Mutex<Inner>>,
    stop: Arc<AtomicU8>,
    emit: Emit,
    generation: u64,
    token: CancellationToken,
    files: Vec<&'static PackFile>,
    dir: PathBuf,
    on_done: Arc<dyn Fn() + Send + Sync>,
) {
    let publish = |inner: &Mutex<Inner>| {
        let snap = lock(inner).progress.clone();
        emit(
            EVT_MODELS_PROGRESS,
            serde_json::to_value(snap).unwrap_or_default(),
        );
    };
    let client = match reqwest::Client::builder()
        .user_agent(concat!("twitch-pikker/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            with(&inner, generation, |p| {
                (p.state, p.error) = (DlState::Failed, Some(e.to_string()))
            });
            publish(&inner);
            return;
        }
    };

    let part_dir = dir.join(".partial");
    let mut result = Ok(());
    for (i, f) in files.iter().enumerate() {
        let part = part_dir.join(format!("{}.part", file_name(f.rel)));
        let dest = dir.join(f.rel);
        let (mut last_emit, mut last_bytes, mut last_t) = (Instant::now(), 0u64, Instant::now());
        let inner2 = inner.clone();
        let emit2 = emit.clone();
        let mut on_progress = move |received: u64, total: u64| {
            let live = with(&inner2, generation, |p| {
                let file = &mut p.files[i];
                let delta = received.saturating_sub(file.received);
                file.received = received;
                if total > 0 {
                    file.total = total;
                }
                p.recompute();
                delta
            });
            if live.is_some() && last_emit.elapsed() >= EMIT_EVERY {
                let dt = last_t.elapsed().as_secs_f64().max(0.001);
                let total_now = lock(&inner2).progress.received;
                let inst = (total_now.saturating_sub(last_bytes)) as f64 / dt;
                let snap = with(&inner2, generation, |p| {
                    p.speed_bps = if p.speed_bps == 0 {
                        inst as u64
                    } else {
                        (p.speed_bps as f64 * 0.6 + inst * 0.4) as u64
                    };
                    p.clone()
                });
                (last_bytes, last_t, last_emit) = (total_now, Instant::now(), Instant::now());
                if let Some(s) = snap {
                    emit2(
                        EVT_MODELS_PROGRESS,
                        serde_json::to_value(s).unwrap_or_default(),
                    );
                }
            }
        };
        result = download_file(
            &client,
            f.url,
            f.size,
            f.sha256,
            &part,
            &dest,
            &token,
            &mut on_progress,
        )
        .await;
        if result.is_ok()
            && let Some(sub) = f.extract
        {
            let (archive, target) = (dest.clone(), dir.join(sub));
            result = match tokio::task::spawn_blocking(move || {
                let r = super::runtime::extract_archive(&archive, &target);
                let _ = std::fs::remove_file(&archive);
                r
            })
            .await
            {
                Ok(Ok(())) => Ok(()),
                Ok(Err(m)) => Err(DlError::Failed(m)),
                Err(e) => Err(fail(e)),
            };
        }
        if result.is_err() {
            break;
        }
        with(&inner, generation, |p| {
            p.files[i].done = true;
            p.files[i].received = p.files[i].total.max(p.files[i].received);
            p.recompute();
        });
        publish(&inner);
    }

    let reason = stop.load(Ordering::SeqCst);
    let ok = result.is_ok();
    with(&inner, generation, |p| match &result {
        Ok(()) => p.state = DlState::Done,
        Err(DlError::Stopped) if reason == STOP_CANCEL => p.state = DlState::Cancelled,
        Err(DlError::Stopped) => p.state = DlState::Paused,
        Err(DlError::Failed(m)) => (p.state, p.error) = (DlState::Failed, Some(m.clone())),
    });
    if matches!(result, Err(DlError::Stopped)) && reason == STOP_CANCEL {
        let _ = tokio::fs::remove_dir_all(&part_dir).await;
    }
    if ok {
        let _ = tokio::fs::remove_dir(&part_dir).await; // only if empty
        on_done();
    }
    if let Some(mut s) = with(&inner, generation, |p| {
        p.speed_bps = 0;
        p.clone()
    }) {
        if s.state == DlState::Cancelled {
            s.files.clear();
        }
        emit(
            EVT_MODELS_PROGRESS,
            serde_json::to_value(s).unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Minimal local HTTP server with Range support. Returns (url, observed Range headers).
    async fn serve(body: Vec<u8>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/f.bin", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let r2 = ranges.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let (body, r2) = (body.clone(), r2.clone());
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let range = req
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("range: bytes=")
                                .map(str::to_owned)
                        })
                        .and_then(|v| v.trim_end_matches('-').parse::<usize>().ok());
                    let resp = if let Some(start) = range {
                        lock(&r2).push(format!("{start}-"));
                        let part = &body[start..];
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {start}-{}/{}\r\nConnection: close\r\n\r\n",
                            part.len(), body.len() - 1, body.len()
                        )
                        .into_bytes()
                        .into_iter()
                        .chain(part.iter().copied())
                        .collect::<Vec<u8>>()
                    } else {
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes()
                        .into_iter()
                        .chain(body.iter().copied())
                        .collect()
                    };
                    let _ = s.write_all(&resp).await;
                });
            }
        });
        (url, ranges)
    }

    fn dirs3(name: &str) -> (PathBuf, PathBuf) {
        let d = std::env::temp_dir().join(format!("tcr-dl-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        (d.join("x.part"), d.join("out/x.bin"))
    }

    fn sample() -> Vec<u8> {
        (0..200_000u32).map(|i| (i % 251) as u8).collect()
    }

    #[tokio::test]
    async fn resumes_from_partial_file_and_verifies_checksum() {
        let body = sample();
        let sha = hex(Sha256::digest(&body).as_slice());
        let (url, ranges) = serve(body.clone()).await;
        let (part, dest) = dirs3("resume");
        std::fs::create_dir_all(part.parent().unwrap()).unwrap();
        std::fs::write(&part, &body[..50_000]).unwrap();
        let tok = CancellationToken::new();
        let mut last = 0;
        download_file(
            &reqwest::Client::new(),
            &url,
            Some(200_000),
            Some(&sha),
            &part,
            &dest,
            &tok,
            &mut |r, _| last = r,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert_eq!(lock(&ranges).as_slice(), ["50000-"]);
        assert_eq!(last, 200_000);
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn checksum_mismatch_discards_file() {
        let (url, _) = serve(sample()).await;
        let (part, dest) = dirs3("badsum");
        let tok = CancellationToken::new();
        let wrong = "0".repeat(64);
        let r = download_file(
            &reqwest::Client::new(),
            &url,
            Some(200_000),
            Some(&wrong),
            &part,
            &dest,
            &tok,
            &mut |_, _| {},
        )
        .await;
        assert!(matches!(r, Err(DlError::Failed(m)) if m.contains("checksum")));
        assert!(!dest.exists() && !part.exists());
    }

    #[tokio::test]
    async fn cancellation_stops_and_keeps_partial_for_pause() {
        let (url, _) = serve(sample()).await;
        let (part, dest) = dirs3("stop");
        let tok = CancellationToken::new();
        tok.cancel();
        let r = download_file(
            &reqwest::Client::new(),
            &url,
            Some(200_000),
            None,
            &part,
            &dest,
            &tok,
            &mut |_, _| {},
        )
        .await;
        assert!(matches!(r, Err(DlError::Stopped)));
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn unreachable_server_is_a_clean_error() {
        let (part, dest) = dirs3("net");
        let tok = CancellationToken::new();
        let r = download_file(
            &reqwest::Client::new(),
            "http://127.0.0.1:9/x",
            None,
            None,
            &part,
            &dest,
            &tok,
            &mut |_, _| {},
        )
        .await;
        assert!(matches!(r, Err(DlError::Failed(_))));
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod integration {
    use super::*;

    #[tokio::test]
    #[ignore = "downloads the Piper program (~25 MB) from GitHub"]
    async fn installs_piper_runtime() {
        let dir = std::env::temp_dir().join(format!("tcr-int-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let emit: Emit = Arc::new(|_, _| {});
        let dl = Downloader::new(emit);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Mutex::new(Some(tx));
        let done: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(t) = tx.lock().unwrap().take() {
                let _ = t.send(());
            }
        });
        dl.start(
            &tokio::runtime::Handle::current(),
            vec!["piper_runtime".into()],
            dir.clone(),
            done,
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(300), rx)
            .await
            .unwrap()
            .unwrap();
        assert!(dir.join(models::PIPER_EXE_REL).is_file());
        assert!(!dir.join("runtime").join("piper_windows_amd64.zip").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
