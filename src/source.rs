// turn what the user typed into something to install

use std::path::{self, Path, PathBuf};

use anyhow::{Result, anyhow, bail};

use crate::hf::Repo;

// where the model comes from: a gguf file on disk or a hugging face repository
pub enum Source {
    Local(PathBuf),
    Remote(Repo),
}

impl Source {
    // links go to hugging face, existing files and *.gguf paths are local, user/repo is a repo name
    pub fn parse(input: &str) -> Result<Source> {
        let input = input.trim().trim_matches(['"', '\'']);
        if input.is_empty() {
            bail!("nothing to install, give a Hugging Face link or a .gguf file");
        }

        let path = Path::new(input);
        let is_link = input.contains("://") || input.contains("huggingface.co/") || input.starts_with("hf.co/");
        if !is_link && (path.is_file() || is_gguf(path)) {
            return local_gguf(path).map(Source::Local);
        }
        Repo::parse(input)
            .map(Source::Remote)
            .ok_or_else(|| anyhow!("'{input}' is not a Hugging Face link, user/repo name or .gguf file"))
    }
}

// check a local gguf file and return its absolute path, which the modelfile needs
pub fn local_gguf(path: &Path) -> Result<PathBuf> {
    if !path.is_file() {
        bail!("file not found: {}", path.display());
    }
    if !is_gguf(path) {
        bail!("not a .gguf file: {}", path.display());
    }
    Ok(path::absolute(path)?)
}

fn is_gguf(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
}
