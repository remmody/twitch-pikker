//! Local model discovery, validation and the (user-initiated) download manifest.

use super::voices::{Gender, TtsEngine, VoiceInfo};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub struct PackFile {
    pub url: &'static str,
    /// Path under the models folder.
    pub rel: &'static str,
    pub size: Option<u64>,
    pub sha256: Option<&'static str>,
    /// If set, the file is an archive extracted into this folder (under the models folder) and then deleted.
    pub extract: Option<&'static str>,
}

/// A downloadable set of files that makes one or more voices usable.
pub struct Pack {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub engine: TtsEngine,
    pub approx_mb: u32,
    pub default_selected: bool,
    /// If set, the pack counts as installed when this file (under the models folder) exists.
    pub marker: Option<&'static str>,
    pub files: &'static [PackFile],
}

// Piper URLs are pinned to a commit of rhasspy/piper-voices, and the SHA-256 values are the LFS
// hashes Hugging Face publishes for those blobs. Silero publishes no checksum: its hash below
// was recorded from a one-time download of the file (trust-on-first-use) and is pinned together
// with its size. If upstream replaces the file, the download fails verification instead of
// installing something unexpected.
macro_rules! piper_pack {
    ($id:literal, $label:literal, $voice:literal, $dir:literal, $sha:literal, $default:expr) => {
        Pack {
            id: $id,
            label: $label,
            description: concat!("Piper Russian voice (", $voice, ")"),
            engine: TtsEngine::Piper,
            approx_mb: 61,
            default_selected: $default,
            marker: None,
            files: &[
                PackFile {
                    url: concat!(
                        "https://huggingface.co/rhasspy/piper-voices/resolve/c10ece1aade47bb51c153c893d14e5bf8e5b7117/ru/ru_RU/",
                        $dir, "/medium/ru_RU-", $dir, "-medium.onnx"
                    ),
                    rel: concat!("piper/ru_RU-", $dir, "-medium.onnx"),
                    size: Some(63_201_294),
                    sha256: Some($sha),
                    extract: None,
                },
                PackFile {
                    url: concat!(
                        "https://huggingface.co/rhasspy/piper-voices/resolve/c10ece1aade47bb51c153c893d14e5bf8e5b7117/ru/ru_RU/",
                        $dir, "/medium/ru_RU-", $dir, "-medium.onnx.json"
                    ),
                    rel: concat!("piper/ru_RU-", $dir, "-medium.onnx.json"),
                    size: None,
                    sha256: None,
                    extract: None,
                },
            ],
        }
    };
}

// Piper program (official release 2023.11.14-2). SHA-256 values were recorded from a one-time
// download of each archive (GitHub publishes none), and are pinned together with the size.

macro_rules! piper_runtime_file {
    ($archive:literal, $size:expr, $sha:literal) => {
        &[PackFile {
            url: concat!(
                "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/",
                $archive
            ),
            rel: concat!("runtime/", $archive),
            size: Some($size),
            sha256: Some($sha),
            extract: Some("runtime"),
        }]
    };
}

#[cfg(all(windows, target_arch = "x86_64"))]
const PIPER_RUNTIME_FILES: &[PackFile] = piper_runtime_file!(
    "piper_windows_amd64.zip",
    22_477_236,
    "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea"
);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const PIPER_RUNTIME_FILES: &[PackFile] = piper_runtime_file!(
    "piper_linux_x86_64.tar.gz",
    26_460_462,
    "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992"
);
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const PIPER_RUNTIME_FILES: &[PackFile] = piper_runtime_file!(
    "piper_linux_aarch64.tar.gz",
    26_004_717,
    "fea0fd2d87c54dbc7078d0f878289f404bd4d6eea6e7444a77835d1537ab88eb"
);
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const PIPER_RUNTIME_FILES: &[PackFile] = piper_runtime_file!(
    "piper_macos_x64.tar.gz",
    19_146_927,
    "ced85c0a3df13945b1e623b878a48fdc2854d5c485b4b67f62857cf551deaf8b"
);
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const PIPER_RUNTIME_FILES: &[PackFile] = piper_runtime_file!(
    "piper_macos_aarch64.tar.gz",
    19_146_957,
    "6b1eb03b3735946cb35216e063e7eebcc33a6bbf5dd96ec0217959bf1cdcb0cc"
);
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
    all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ),
)))]
const PIPER_RUNTIME_FILES: &[PackFile] = &[];

/// Piper program location after extraction, relative to the models folder.
pub const PIPER_EXE_REL: &str = if cfg!(windows) {
    "runtime/piper/piper.exe"
} else {
    "runtime/piper/piper"
};

pub const PACKS: &[Pack] = &[
    Pack {
        id: "piper_runtime",
        label: "Piper program",
        description: "The offline speech engine that runs the Piper voices.",
        engine: TtsEngine::Piper,
        approx_mb: 25,
        default_selected: true,
        marker: Some(PIPER_EXE_REL),
        files: PIPER_RUNTIME_FILES,
    },
    Pack {
        id: "silero_v4_ru",
        label: "Silero v4 Russian (5 voices)",
        description: "Kseniya, Xenia, Baya (female); Aidar, Eugene (male). Needs Python + PyTorch installed.",
        engine: TtsEngine::Silero,
        approx_mb: 38,
        default_selected: true,
        marker: None,
        files: &[PackFile {
            url: "https://models.silero.ai/models/tts/ru/v4_ru.pt",
            rel: "silero/v4_ru.pt",
            size: Some(40_107_184),
            sha256: Some("896ab96347d5bd781ab97959d4fd6885620e5aab52405d3445626eb7c1414b00"),
            extract: None,
        }],
    },
    piper_pack!(
        "piper_irina",
        "Piper — Irina (female)",
        "irina",
        "irina",
        "8ff38212d23da300bbe3705c645e6e5b9475f0bfde01558eb17813e22acaaaaa",
        true
    ),
    piper_pack!(
        "piper_denis",
        "Piper — Denis (male)",
        "denis",
        "denis",
        "15fab56e11a097858ee115545d0f697fc2a316c41a291a5362349fb870411b0a",
        true
    ),
    piper_pack!(
        "piper_dmitri",
        "Piper — Dmitri (male)",
        "dmitri",
        "dmitri",
        "f073356ebc4bd0f80c5af58df2953a5988bd5bdab1eb38635ce960b071fbefcb",
        false
    ),
    piper_pack!(
        "piper_ruslan",
        "Piper — Ruslan (male)",
        "ruslan",
        "ruslan",
        "72a5f88e0b20928064eb45d88e1daa21f8af62d18613580d32cbb4aed48dcf7f",
        false
    ),
];

pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

/// `dir/rel`, or (for hand-assembled folders) the bare file name directly under `dir`.
fn locate(dir: &Path, rel: &str) -> Option<PathBuf> {
    let exact = dir.join(rel);
    if exact.is_file() {
        return Some(exact);
    }
    let flat = dir.join(Path::new(rel).file_name()?);
    flat.is_file().then_some(flat)
}

pub fn pack_installed_file(f: &PackFile, dir: &Path) -> bool {
    locate(dir, f.rel).is_some()
}

pub fn pack_installed(p: &Pack, dir: &Path) -> bool {
    match p.marker {
        Some(m) => dir.join(m).is_file(),
        None => p.files.iter().all(|f| locate(dir, f.rel).is_some()),
    }
}

const SILERO_REL: &str = "silero/v4_ru.pt";

/// Model file a voice needs. May not exist; the engine reports `ModelMissing` then.
pub fn model_path(engine: TtsEngine, voice_id: &str, dir: &Path) -> PathBuf {
    let rel = match engine {
        TtsEngine::Silero => SILERO_REL.to_owned(),
        TtsEngine::Piper => format!("piper/{voice_id}.onnx"),
    };
    locate(dir, &rel).unwrap_or_else(|| dir.join(rel))
}

pub fn is_installed(engine: TtsEngine, voice_id: &str, dir: &Path) -> bool {
    let model = model_path(engine, voice_id, dir);
    match engine {
        TtsEngine::Silero => model.is_file(),
        TtsEngine::Piper => model.is_file() && sidecar(&model).is_file(),
    }
}

fn sidecar(onnx: &Path) -> PathBuf {
    let mut s = onnx.as_os_str().to_owned();
    s.push(".json");
    PathBuf::from(s)
}

/// Built-in voices plus any extra Piper `.onnx` models found in the folder.
pub fn list_voices(engine: TtsEngine, dir: &Path) -> Vec<VoiceInfo> {
    let mut out: Vec<VoiceInfo> = engine
        .known_voices()
        .iter()
        .map(|v| VoiceInfo {
            engine,
            id: v.id.to_owned(),
            name: v.name.to_owned(),
            language: "ru",
            gender: v.gender,
            installed: is_installed(engine, v.id, dir),
        })
        .collect();
    if engine == TtsEngine::Piper {
        for id in piper_models_in(dir) {
            if !out.iter().any(|v| v.id == id) {
                out.push(VoiceInfo {
                    engine,
                    name: id.clone(),
                    language: if id.starts_with("ru_") { "ru" } else { "other" },
                    gender: Gender::Unknown,
                    installed: is_installed(engine, &id, dir),
                    id,
                });
            }
        }
    }
    out
}

/// Stems of `*.onnx` files that have a `.onnx.json` next to them.
fn piper_models_in(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for d in [dir.to_path_buf(), dir.join("piper")] {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "onnx")
                && sidecar(&p).is_file()
                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                && !found.iter().any(|f| f == stem)
            {
                found.push(stem.to_owned());
            }
        }
    }
    found.sort();
    found
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ModelValidation {
    pub ok: bool,
    pub silero: bool,
    pub piper_voices: Vec<String>,
    pub problems: Vec<String>,
}

/// Inspect a user-chosen folder. `ok` means at least one usable model was found.
pub fn validate_folder(dir: &Path) -> ModelValidation {
    let mut problems = Vec::new();
    if !dir.is_dir() {
        problems.push("Folder does not exist.".to_owned());
        return ModelValidation {
            ok: false,
            silero: false,
            piper_voices: vec![],
            problems,
        };
    }
    let silero = match locate(dir, SILERO_REL) {
        Some(p) => std::fs::metadata(&p)
            .map(|m| m.len() > 1_000_000)
            .unwrap_or(false),
        None => false,
    };
    let piper_voices = piper_models_in(dir);
    for d in [dir.to_path_buf(), dir.join("piper")] {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "onnx") && !sidecar(&p).is_file() {
                problems.push(format!(
                    "{}: missing {}.json next to the model.",
                    p.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                    p.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ));
            }
        }
    }
    let ok = silero || !piper_voices.is_empty();
    if !ok && problems.is_empty() {
        problems.push(
            "No Silero v4_ru.pt or Piper .onnx + .onnx.json found in this folder.".to_owned(),
        );
    }
    ModelValidation {
        ok,
        silero,
        piper_voices,
        problems,
    }
}

/// Resolve an executable name or path the same way spawning would.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    let p = Path::new(name);
    if p.components().count() > 1 || p.is_absolute() {
        return p.is_file().then(|| p.to_path_buf());
    }
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&std::env::var_os("PATH")?).find_map(|d| {
        exts.iter()
            .map(|x| d.join(format!("{name}{x}")))
            .find(|c| c.is_file())
    })
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TtsReadiness {
    Ready,
    ModelMissing,
    RuntimeMissing,
}

/// What is missing on the machine for an engine to run.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Ok,
    NoPython,
    NoTorch,
    NoPiper,
}

#[derive(Debug, Clone, Serialize)]
pub struct TtsStatus {
    pub state: TtsReadiness,
    /// Set when `state == runtime_missing`; tells the UI what to offer to install.
    pub missing: Option<RuntimeState>,
    pub detail: String,
}

/// Offline readiness of an engine/voice: model on disk and runtime available.
pub fn status(engine: TtsEngine, voice: &str, dir: &Path, runtime: RuntimeState) -> TtsStatus {
    let (state, missing, detail) = if !is_installed(engine, voice, dir) {
        (
            TtsReadiness::ModelMissing,
            None,
            format!(
                "{} model for '{voice}' not found in {}",
                engine.label(),
                dir.display()
            ),
        )
    } else {
        match runtime {
            RuntimeState::Ok => (
                TtsReadiness::Ready,
                None,
                format!("{} / {voice}", engine.label()),
            ),
            RuntimeState::NoPython => (
                TtsReadiness::RuntimeMissing,
                Some(runtime),
                "Python was not found (needed by Silero).".to_owned(),
            ),
            RuntimeState::NoTorch => (
                TtsReadiness::RuntimeMissing,
                Some(runtime),
                "PyTorch is not installed for Python (needed by Silero).".to_owned(),
            ),
            RuntimeState::NoPiper => (
                TtsReadiness::RuntimeMissing,
                Some(runtime),
                "The Piper program was not found.".to_owned(),
            ),
        }
    };
    TtsStatus {
        state,
        missing,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("tcr-models-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("piper")).unwrap();
        d
    }

    #[test]
    fn manifest_is_sane() {
        for p in PACKS {
            assert!(!p.files.is_empty());
            for f in p.files {
                assert!(f.url.starts_with("https://"));
                if let Some(h) = f.sha256 {
                    assert_eq!(h.len(), 64, "{}", f.rel);
                    assert!(h.bytes().all(|b| b.is_ascii_hexdigit()));
                }
            }
        }
        assert!(
            PACKS
                .iter()
                .any(|p| p.default_selected && p.engine == TtsEngine::Silero)
        );
    }

    #[test]
    fn empty_folder_is_missing_not_a_panic() {
        let d = tmp("empty");
        let v = validate_folder(&d);
        assert!(!v.ok && !v.problems.is_empty());
        assert!(!is_installed(TtsEngine::Silero, "kseniya", &d));
        assert!(!validate_folder(&d.join("nope")).ok);
        assert_eq!(
            status(TtsEngine::Piper, "ru_RU-irina-medium", &d, RuntimeState::Ok).state,
            TtsReadiness::ModelMissing
        );
    }

    #[test]
    fn detects_piper_pair_and_flags_missing_json() {
        let d = tmp("piper");
        std::fs::write(d.join("piper/ru_RU-irina-medium.onnx"), b"x").unwrap();
        let v = validate_folder(&d);
        assert!(!v.ok);
        assert!(v.problems[0].contains("ru_RU-irina-medium.onnx.json"));
        std::fs::write(d.join("piper/ru_RU-irina-medium.onnx.json"), b"{}").unwrap();
        let v = validate_folder(&d);
        assert!(v.ok && v.piper_voices == ["ru_RU-irina-medium"]);
        assert!(is_installed(TtsEngine::Piper, "ru_RU-irina-medium", &d));
        let voices = list_voices(TtsEngine::Piper, &d);
        assert!(
            voices
                .iter()
                .find(|v| v.id == "ru_RU-irina-medium")
                .unwrap()
                .installed
        );
        assert!(
            !voices
                .iter()
                .find(|v| v.id == "ru_RU-denis-medium")
                .unwrap()
                .installed
        );
    }

    #[test]
    fn flat_layout_and_silero_detection() {
        let d = tmp("silero");
        std::fs::write(d.join("v4_ru.pt"), vec![0u8; 1_100_000]).unwrap();
        assert!(validate_folder(&d).silero);
        assert!(is_installed(TtsEngine::Silero, "baya", &d));
        assert!(pack_installed(pack("silero_v4_ru").unwrap(), &d));
    }

    #[test]
    fn runtime_missing_is_reported() {
        let d = tmp("rt");
        std::fs::write(d.join("v4_ru.pt"), b"x").unwrap();
        let s = status(TtsEngine::Silero, "baya", &d, RuntimeState::NoTorch);
        assert_eq!(s.state, TtsReadiness::RuntimeMissing);
        assert_eq!(s.missing, Some(RuntimeState::NoTorch));
        assert_eq!(
            status(TtsEngine::Silero, "baya", &d, RuntimeState::Ok).state,
            TtsReadiness::Ready
        );
    }
}
