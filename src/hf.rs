// hugging face access: parse links, list gguf files, download with resume

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use ureq::http::Response;
use ureq::tls::{RootCerts, TlsConfig};
use ureq::{Agent, Body};

use crate::ui;

const HUB: &str = "https://huggingface.co";
const USER_AGENT: &str = concat!("ollama-gguf/", env!("CARGO_PKG_VERSION"));

// trust the windows certificate store like browsers do, so antivirus or company https proxies work
static AGENT: LazyLock<Agent> = LazyLock::new(|| {
    let tls = TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build();
    Agent::config_builder().tls_config(tls).build().new_agent()
});

// a hugging face model repository, optionally pointing at one file inside it
pub struct Repo {
    pub id: String,
    pub revision: String,
    pub file: Option<String>,
}

// one file in a repository, path is relative to the repo root
#[derive(Clone, Deserialize)]
pub struct RemoteFile {
    #[serde(rename = "rfilename")]
    pub path: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize)]
struct RepoInfo {
    siblings: Vec<RemoteFile>,
}

impl Repo {
    // accept huggingface.co/user/repo with optional /tree, /blob or /resolve parts, hf.co/user/repo or user/repo
    pub fn parse(input: &str) -> Option<Repo> {
        let link = input.split(['?', '#']).next()?.trim_end_matches('/');
        let link = link.split_once("://").map_or(link, |(_, rest)| rest);
        let path = match ["huggingface.co/", "www.huggingface.co/", "hf.co/"].iter().find_map(|host| link.strip_prefix(host)) {
            Some(path) => path,
            None if input.contains("://") => return None,
            None => link,
        };

        let parts: Vec<&str> = path.split('/').collect();
        let (owner, name, revision, file) = match parts.as_slice() {
            [owner, name] => (*owner, *name, "main", None),
            [owner, name, "tree", revision, ..] => (*owner, *name, *revision, None),
            [owner, name, "blob" | "resolve", revision, file @ ..] if !file.is_empty() => {
                (*owner, *name, *revision, Some(file.join("/")))
            }
            _ => return None,
        };

        let valid = |part: &str, extra: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
        (valid(owner, "-_") && valid(name, "-_.")).then(|| Repo {
            id: format!("{owner}/{name}"),
            revision: revision.to_string(),
            file,
        })
    }
}

// every gguf file in the repo, sorted by path like the hugging face file view
pub fn list_gguf(repo: &Repo, token: Option<&str>) -> Result<Vec<RemoteFile>> {
    let url = format!("{HUB}/api/models/{}/revision/{}?blobs=true", repo.id, repo.revision);
    let info: RepoInfo = get(&url, token, None)?
        .body_mut()
        .read_json()
        .context("unexpected answer from Hugging Face")?;

    let mut files: Vec<RemoteFile> = info
        .siblings
        .into_iter()
        .filter(|file| file.path.to_lowercase().ends_with(".gguf"))
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

// download one repo file to dest, continuing a .part file left by an interrupted run
pub fn download(repo: &Repo, file: &RemoteFile, dest: &Path, token: Option<&str>) -> Result<()> {
    if fs::metadata(dest).is_ok_and(|meta| meta.len() == file.size) {
        ui::note("already downloaded, reusing it");
        return Ok(());
    }
    fs::create_dir_all(dest.parent().context("invalid download path")?)?;

    let part = dest.with_extension("gguf.part");
    let mut done = fs::metadata(&part).map_or(0, |meta| meta.len());
    if done > file.size {
        done = 0;
    }

    if done < file.size {
        let url = format!("{HUB}/{}/resolve/{}/{}", repo.id, repo.revision, file.path);
        let response = get(&url, token, (done > 0).then_some(done))?;

        // a server that ignores the range sends the whole file, so start over
        if response.status().as_u16() != 206 {
            done = 0;
        }
        let out = if done > 0 { OpenOptions::new().append(true).open(&part)? } else { File::create(&part)? };

        let bar = ui::download_bar(file.size, done);
        let mut writer = BufWriter::with_capacity(1 << 20, out);
        io::copy(&mut bar.wrap_read(response.into_body().into_reader()), &mut writer)
            .and_then(|_| writer.flush())
            .context("download interrupted, run the same command again to resume")?;
        bar.finish_and_clear();
    }

    let size = fs::metadata(&part)?.len();
    if size != file.size {
        bail!("download incomplete ({} of {}), run the same command again to resume", ui::size(size), ui::size(file.size));
    }
    if dest.exists() {
        fs::remove_file(dest)?;
    }
    fs::rename(&part, dest)?;
    Ok(())
}

// get request with optional token and resume offset, http errors become readable messages
fn get(url: &str, token: Option<&str>, from: Option<u64>) -> Result<Response<Body>> {
    let mut request = AGENT.get(url).header("User-Agent", USER_AGENT);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(from) = from {
        request = request.header("Range", format!("bytes={from}-"));
    }

    request.call().map_err(|err| match err {
        ureq::Error::StatusCode(401) => anyhow!("repo not found, or it is private and needs --token (or HF_TOKEN)"),
        ureq::Error::StatusCode(403) => anyhow!("access denied, accept the model terms on huggingface.co and use --token"),
        ureq::Error::StatusCode(404) => anyhow!("not found on Hugging Face: {url}"),
        err => anyhow!(err).context(format!("request to {url} failed")),
    })
}

#[cfg(test)]
mod tests {
    use super::Repo;

    fn parts(input: &str) -> Option<(String, String, Option<String>)> {
        Repo::parse(input).map(|repo| (repo.id, repo.revision, repo.file))
    }

    #[test]
    fn parses_links_and_names() {
        let repo = |id: &str, revision: &str, file: Option<&str>| Some((id.into(), revision.into(), file.map(Into::into)));
        assert_eq!(parts("https://huggingface.co/ZY-Lee4/QJev3.5-0.8B-GGUF/tree/main"), repo("ZY-Lee4/QJev3.5-0.8B-GGUF", "main", None));
        assert_eq!(parts("hf.co/user/model-GGUF/blob/dev/sub/m.Q4_K_M.gguf?download=true"), repo("user/model-GGUF", "dev", Some("sub/m.Q4_K_M.gguf")));
        assert_eq!(parts("user/repo"), repo("user/repo", "main", None));
        assert_eq!(parts("https://example.com/user/repo"), None);
        assert_eq!(parts("example.com/repo"), None);
        assert_eq!(parts("hello"), None);
    }
}
