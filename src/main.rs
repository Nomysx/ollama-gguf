// ollama-gguf: install gguf models into ollama from a hugging face link or a local file

mod gguf;
mod hf;
mod ollama;
mod setup;
mod source;
mod ui;

use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::Parser;

use crate::gguf::Kind;
use crate::hf::{RemoteFile, Repo};
use crate::source::Source;

const EXAMPLES: &str = "\
Examples:
  ollama-gguf                                          step-by-step wizard
  ollama-gguf https://huggingface.co/user/repo         pick a file from the repo
  ollama-gguf user/repo --file Q4_K_M --name mymodel   no questions asked
  ollama-gguf D:\\models\\model.gguf                     install a local file
  ollama-gguf --add-to-path                            use the command from any terminal";

#[derive(Parser)]
#[command(bin_name = "ollama-gguf", version, about ="Install GGUF models into Ollama from a Hugging Face link or a local file", after_help = EXAMPLES)]
struct Args {
    #[arg(help = "Hugging Face link, user/repo name or local .gguf file (omit it for the wizard)")]
    source: Option<String>,

    #[arg(short, long, value_name = "NAME", help = "GGUF file to install, full or partial name like Q4_K_M")]
    file: Option<String>,

    #[arg(short, long, help = "Model name in Ollama [default: taken from the file name]")]
    name: Option<String>,

    #[arg(short, long, value_name = "NAME|PATH", help = "LoRA adapter to apply, a file from the repo or a local .gguf")]
    adapter: Option<String>,

    #[arg(short, long, help = "List the GGUF files in the repo and exit")]
    list: bool,

    #[arg(long, help = "Keep the downloaded files after install")]
    keep: bool,

    #[arg(long, env = "HF_TOKEN", hide_env_values = true, help = "Hugging Face token for private or gated repos")]
    token: Option<String>,

    #[arg(long, help = "Copy this exe to your user folder and add it to PATH")]
    add_to_path: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    ui::restore_cursor_on_ctrl_c();

    let result = if args.add_to_path { setup::add_to_path() } else { run(&args) };
    if let Err(err) = &result {
        ui::error(err);
    }

    // a double-clicked window would close before the result can be read
    if ui::launched_from_explorer() {
        ui::wait_for_enter();
    }
    if result.is_ok() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

// resolve the source, then list it or install from disk or from hugging face
fn run(args: &Args) -> Result<()> {
    let wizard = args.source.is_none();
    let input = match &args.source {
        Some(source) => source.clone(),
        None if io::stdin().is_terminal() => {
            ui::banner();
            ui::ask_source()?
        }
        None => bail!("no source given, run ollama-gguf --help"),
    };

    match Source::parse(&input)? {
        Source::Remote(repo) if args.list => list_files(&repo, args.token.as_deref()),
        Source::Local(_) if args.list => bail!("--list works with Hugging Face links only"),
        Source::Remote(repo) => install_remote(&repo, args, wizard),
        Source::Local(path) => install_local(&path, args, wizard),
    }
}

// install a gguf file that is already on disk, the file itself is left untouched
fn install_local(model: &Path, args: &Args, wizard: bool) -> Result<()> {
    ollama::ensure_installed()?;
    let adapter = args.adapter.as_deref().map(|path| source::local_gguf(Path::new(path))).transpose()?;
    let name = choose_name(args, adapter.as_deref().unwrap_or(model), wizard)?;
    create(&name, model, adapter.as_deref())
}

// pick files from a hugging face repo, download them, install, then remove the downloads
fn install_remote(repo: &Repo, args: &Args, wizard: bool) -> Result<()> {
    ollama::ensure_installed()?;
    let token = args.token.as_deref();

    ui::step(format!("Looking up {} on Hugging Face", repo.id));
    let files = hf::list_gguf(repo, token)?;
    report(&files);
    let models = of_kind(&files, Kind::Model);
    let adapters = of_kind(&files, Kind::Adapter);
    if models.is_empty() {
        bail!("no GGUF model files in {}", repo.id);
    }

    // ask every question before the long download starts
    let model = choose_model(&models, repo.file.as_deref().or(args.file.as_deref()))?;
    let adapter = choose_adapter(&adapters, args.adapter.as_deref(), wizard)?;
    let name = choose_name(args, Path::new(&adapter.unwrap_or(model).path), wizard)?;

    let model_path = fetch(repo, model, token)?;
    let adapter_path = adapter.map(|file| fetch(repo, file, token)).transpose()?;
    let downloads: Vec<&Path> = [Some(model_path.as_path()), adapter_path.as_deref()].into_iter().flatten().collect();

    if let Err(err) = create(&name, &model_path, adapter_path.as_deref()) {
        ui::note(format!("downloads kept in {}, the next run reuses them", downloads_dir().display()));
        return Err(err);
    }
    if args.keep {
        ui::note(format!("downloads kept in {}", downloads_dir().display()));
    } else {
        remove_downloads(&downloads);
    }
    Ok(())
}

// print the repo's gguf files with size and kind to stdout, so the list can be piped
fn list_files(repo: &Repo, token: Option<&str>) -> Result<()> {
    let files = hf::list_gguf(repo, token)?;
    if files.is_empty() {
        bail!("no GGUF files in {}", repo.id);
    }
    for (label, file) in labels(&files).iter().zip(&files) {
        println!("{label}  {}", Kind::of(&file.path).label());
    }
    Ok(())
}

// build the ollama model and show how to run it
fn create(name: &str, model: &Path, adapter: Option<&Path>) -> Result<()> {
    ui::step(format!("Creating {name} in Ollama"));
    ollama::create(name, model, adapter)?;
    ui::success(format!("Installed {name}, run it with: ollama run {name}"));
    Ok(())
}

// pick the model: from the link or --file, automatically when only one fits, otherwise from a menu
fn choose_model<'a>(models: &'a [RemoteFile], pattern: Option<&str>) -> Result<&'a RemoteFile> {
    let candidates = match pattern {
        Some(pattern) => gguf::find(models, pattern),
        None => models.iter().collect(),
    };
    match candidates.as_slice() {
        [] => bail!("no model file matches '{}', see the files with --list", pattern.unwrap_or_default()),
        [only] => Ok(*only),
        many if io::stdin().is_terminal() => Ok(many[ui::pick("Choose a model file", &labels(many.iter().copied()))?]),
        many => bail!("{} files match, choose one with --file:\n  {}", many.len(), paths(many)),
    }
}

// on the command line adapters come only from --adapter, the wizard offers them in a menu
fn choose_adapter<'a>(adapters: &'a [RemoteFile], pattern: Option<&str>, wizard: bool) -> Result<Option<&'a RemoteFile>> {
    if let Some(pattern) = pattern {
        return match gguf::find(adapters, pattern).as_slice() {
            [] => bail!("no LoRA adapter matches '{pattern}', see the files with --list"),
            [only] => Ok(Some(*only)),
            many => bail!("{} adapters match, be more specific:\n  {}", many.len(), paths(many)),
        };
    }
    if !wizard || adapters.is_empty() {
        return Ok(None);
    }
    let mut items = vec!["none, install the plain model".to_string()];
    items.extend(labels(adapters));
    let choice = ui::pick("Apply a LoRA adapter?", &items)?;
    Ok(choice.checked_sub(1).map(|index| &adapters[index]))
}

// --name wins, otherwise the file name, which the wizard lets the user edit
fn choose_name(args: &Args, file: &Path, wizard: bool) -> Result<String> {
    if let Some(name) = &args.name {
        return Ok(name.clone());
    }
    let default = gguf::model_name(file);
    if !wizard {
        return Ok(default);
    }
    let name = ui::ask("Model name", &default)?;
    if ollama::model_exists(&name) && !ui::confirm(&format!("{name} already exists, replace it?"))? {
        bail!("cancelled");
    }
    Ok(name)
}

// download one repo file into the downloads folder and return its path
fn fetch(repo: &Repo, file: &RemoteFile, token: Option<&str>) -> Result<PathBuf> {
    let dest = repo.id.split('/').chain(file.path.split('/')).fold(downloads_dir(), |dir, part| dir.join(part));
    ui::step(format!("Downloading {} ({})", file.path, ui::size(file.size)));
    hf::download(repo, file, &dest, token)?;
    Ok(dest)
}

// downloads live here until ollama has made its own copy
fn downloads_dir() -> PathBuf {
    let base = env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(env::temp_dir);
    base.join("ollama-gguf").join("downloads")
}

// delete downloaded files and the folders they leave empty
fn remove_downloads(files: &[&Path]) {
    let root = downloads_dir();
    for file in files {
        let _ = fs::remove_file(file);
        for dir in file.ancestors().skip(1).take_while(|dir| *dir != root) {
            if fs::remove_dir(dir).is_err() {
                break;
            }
        }
    }
}

// say what the repo offers and why some files are left out
fn report(files: &[RemoteFile]) {
    let count = |kind| files.iter().filter(|file| Kind::of(&file.path) == kind).count();
    let mut found = vec![plural(count(Kind::Model), "model file")];
    if count(Kind::Adapter) > 0 {
        found.push(plural(count(Kind::Adapter), "LoRA adapter"));
    }
    ui::note(format!("found {}", found.join(" and ")));
    if count(Kind::Split) > 0 {
        ui::note(format!("skipped {}, Ollama cannot import models split into parts", plural(count(Kind::Split), "split file")));
    }
    if count(Kind::Projector) > 0 {
        ui::note(format!("skipped {}", plural(count(Kind::Projector), "vision projector file")));
    }
}

fn of_kind(files: &[RemoteFile], kind: Kind) -> Vec<RemoteFile> {
    files.iter().filter(|file| Kind::of(&file.path) == kind).cloned().collect()
}

// one aligned "name  size" line per file
fn labels<'a>(files: impl IntoIterator<Item = &'a RemoteFile>) -> Vec<String> {
    let files: Vec<&RemoteFile> = files.into_iter().collect();
    let width = files.iter().map(|file| file.path.len()).max().unwrap_or(0);
    files.iter().map(|file| format!("{:<width$}  {:>10}", file.path, ui::size(file.size))).collect()
}

fn paths(files: &[&RemoteFile]) -> String {
    files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>().join("\n  ")
}

fn plural(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}
