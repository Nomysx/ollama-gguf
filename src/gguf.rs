// gguf file names: what a file is, how to find it, what to call the model

use std::path::Path;

use crate::hf::RemoteFile;

// the role of a gguf file, judged by its name
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Model,
    Adapter,
    Projector,
    Split,
}

impl Kind {
    pub fn of(path: &str) -> Kind {
        let stem = stem(path).to_lowercase();
        let words: Vec<&str> = stem.split(['-', '_', '.', ' ']).collect();
        let has = |word: &str| words.contains(&word);

        if words.iter().any(|word| word.starts_with("mmproj")) || has("projector") {
            Kind::Projector
        } else if matches!(words.as_slice(), [.., part, "of", total] if is_number(part) && is_number(total)) {
            Kind::Split
        } else if has("lora") || has("adapter") {
            Kind::Adapter
        } else {
            Kind::Model
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Model => "model",
            Kind::Adapter => "LoRA adapter",
            Kind::Projector => "vision projector",
            Kind::Split => "split part",
        }
    }
}

// files matching a name, case insensitive: the exact name first, then names ending in it, then names containing it
pub fn find<'a>(files: &'a [RemoteFile], pattern: &str) -> Vec<&'a RemoteFile> {
    let pattern = pattern.to_lowercase();
    let exact = |path: &str| path == pattern || file_name(path) == pattern || stem(path) == pattern;
    let ending = |path: &str| stem(path).strip_suffix(&pattern).is_some_and(|rest| rest.ends_with(['-', '_', '.']));
    let partial = |path: &str| path.contains(&pattern);

    for rule in [&exact as &dyn Fn(&str) -> bool, &ending, &partial] {
        let found: Vec<&RemoteFile> = files.iter().filter(|file| rule(&file.path.to_lowercase())).collect();
        if !found.is_empty() {
            return found;
        }
    }
    Vec::new()
}

// ollama model name from a file name, e.g. Qwen3-8B-Q4_K_M.gguf becomes qwen3-8b-q4_k_m
pub fn model_name(file: &Path) -> String {
    let stem = file.file_stem().map(|stem| stem.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut name = String::new();
    for c in stem.chars() {
        let c = if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' };
        // spaces and symbols in a row become a single dash
        if !(c == '-' && name.ends_with('-')) {
            name.push(c);
        }
    }
    let name = name.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if name.is_empty() { "model".to_string() } else { name.to_string() }
}

fn stem(path: &str) -> &str {
    Path::new(path).file_stem().and_then(|stem| stem.to_str()).unwrap_or(path)
}

fn file_name(path: &str) -> &str {
    Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path)
}

fn is_number(word: &str) -> bool {
    !word.is_empty() && word.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<RemoteFile> {
        paths.iter().map(|path| RemoteFile { path: path.to_string(), size: 0 }).collect()
    }

    fn found(files: &[RemoteFile], pattern: &str) -> Vec<String> {
        find(files, pattern).iter().map(|file| file.path.clone()).collect()
    }

    #[test]
    fn classifies_files_by_name() {
        assert_eq!(Kind::of("Qwen3-8B-Q4_K_M.gguf"), Kind::Model);
        assert_eq!(Kind::of("Flora-7B.Q8_0.gguf"), Kind::Model);
        assert_eq!(Kind::of("qjev35-lora-v14.gguf"), Kind::Adapter);
        assert_eq!(Kind::of("mmproj-model-f16.gguf"), Kind::Projector);
        assert_eq!(Kind::of("BF16/Qwen3-30B-BF16-00001-of-00002.gguf"), Kind::Split);
    }

    #[test]
    fn finds_files_by_exact_ending_or_partial_name() {
        let adapters = files(&["qjev35-lora-v14.gguf", "qjev35-lora-v14_s0.gguf"]);
        assert_eq!(found(&adapters, "lora-v14"), ["qjev35-lora-v14.gguf"]);
        assert_eq!(found(&adapters, "QJEV35-LORA-V14_S0.GGUF"), ["qjev35-lora-v14_s0.gguf"]);

        let models = files(&["m-Q4_K_M.gguf", "m-Q4_K_S.gguf", "m-Q8_0.gguf"]);
        assert_eq!(found(&models, "q4_k").len(), 2);
        assert!(found(&models, "q5").is_empty());
    }

    #[test]
    fn derives_clean_model_names() {
        assert_eq!(model_name(Path::new("Qwen3-8B-Q4_K_M.gguf")), "qwen3-8b-q4_k_m");
        assert_eq!(model_name(Path::new(r"C:\my models\OGG Test (local).gguf")), "ogg-test-local");
        assert_eq!(model_name(Path::new("(((.gguf")), "model");
    }
}
