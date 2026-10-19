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
}

/// A downloadable set of files that makes one or more voices usable.
pub struct Pack {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub engine: TtsEngine,
    pub approx_mb: u32,
    pub default_selected: bool,
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
            files: &[
                PackFile {
                    url: concat!(
                        "https://huggingface.co/rhasspy/piper-voices/resolve/c10ece1aade47bb51c153c893d14e5bf8e5b7117/ru/ru_RU/",
                        $dir, "/medium/ru_RU-", $dir, "-medium.onnx"
                    ),
                    rel: concat!("piper/ru_RU-", $dir, "-medium.onnx"),
                    size: Some(63_201_294),
                    sha256: Some($sha),
                },
                PackFile {
                    url: concat!(
                        "https://huggingface.co/rhasspy/piper-voices/resolve/c10ece1aade47bb51c153c893d14e5bf8e5b7117/ru/ru_RU/",
                        $dir, "/medium/ru_RU-", $dir, "-medium.onnx.json"
                    ),
                    rel: concat!("piper/ru_RU-", $dir, "-medium.onnx.json"),
                    size: None,
                    sha256: None,
                },
            ],
        }
    };
}

pub const PACKS: &[Pack] = &[
    Pack {
        id: "silero_v4_ru",
        label: "Silero v4 Russian (5 voices)",
        description: "Kseniya, Xenia, Baya (female); Aidar, Eugene (male). Needs Python + PyTorch installed.",
        engine: TtsEngine::Silero,
        approx_mb: 38,
        default_selected: true,
        files: &[PackFile {
            url: "https://models.silero.ai/models/tts/ru/v4_ru.pt",
            rel: "silero/v4_ru.pt",
            size: Some(40_107_184),
            sha256: Some("896ab96347d5bd781ab97959d4fd6885620e5aab52405d3445626eb7c1414b00"),
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
    p.files.iter().all(|f| locate(dir, f.rel).is_some())
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

#[derive(Debug, Clone, Serialize)]
pub struct TtsStatus {
    pub state: TtsReadiness,
    pub detail: String,
}

/// Offline readiness of an engine/voice: model on disk and runtime executable found.
pub fn status(engine: TtsEngine, voice: &str, dir: &Path, runtime: &str) -> TtsStatus {
    if !is_installed(engine, voice, dir) {
        return TtsStatus {
            state: TtsReadiness::ModelMissing,
            detail: format!(
                "{} model for '{voice}' not found in {}",
                engine.label(),
                dir.display()
            ),
        };
    }
    if find_executable(runtime).is_none() {
        let what = match engine {
            TtsEngine::Silero => "Python (with PyTorch)",
            TtsEngine::Piper => "the Piper executable",
        };
        return TtsStatus {
            state: TtsReadiness::RuntimeMissing,
            detail: format!("'{runtime}' not found. Install {what} or set its path in Settings."),
        };
    }
    TtsStatus {
        state: TtsReadiness::Ready,
        detail: format!("{} / {voice}", engine.label()),
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
            status(TtsEngine::Piper, "ru_RU-irina-medium", &d, "piper").state,
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
        let s = status(
            TtsEngine::Silero,
            "baya",
            &d,
            "definitely-not-a-real-binary-xyz",
        );
        assert_eq!(s.state, TtsReadiness::RuntimeMissing);
    }
}
