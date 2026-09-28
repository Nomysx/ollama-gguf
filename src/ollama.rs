// ollama cli calls: presence check, model lookup, model creation from a modelfile

use std::fs;
use std::io;
use std::path::Path;
use std::process::{self, Command, ExitStatus, Stdio};

use anyhow::{Context, Result, bail};

// fail before any download when ollama is missing
pub fn ensure_installed() -> Result<()> {
    if quiet(&["--version"]).is_err() {
        bail!("Ollama was not found, install it from https://ollama.com/download and try again");
    }
    Ok(())
}

pub fn model_exists(name: &str) -> bool {
    quiet(&["show", name]).is_ok_and(|status| status.success())
}

// write a temporary modelfile and let ollama build the model, its progress stays visible
pub fn create(name: &str, model: &Path, adapter: Option<&Path>) -> Result<()> {
    let modelfile = std::env::temp_dir().join(format!("ollama-gguf-{}.Modelfile", process::id()));
    fs::write(&modelfile, modelfile_text(model, adapter)).context("could not write the Modelfile")?;

    let status = Command::new("ollama").args(["create", name, "-f"]).arg(&modelfile).status();
    let _ = fs::remove_file(&modelfile);

    if !status.context("could not run ollama")?.success() {
        bail!("Ollama could not create the model");
    }
    Ok(())
}

// the gguf weights plus an optional lora adapter
fn modelfile_text(model: &Path, adapter: Option<&Path>) -> String {
    let mut lines = vec![format!("FROM {}", model.display())];
    if let Some(adapter) = adapter {
        lines.push(format!("ADAPTER {}", adapter.display()));
    }
    lines.join("\n") + "\n"
}

// run ollama with its output hidden
fn quiet(args: &[&str]) -> io::Result<ExitStatus> {
    Command::new("ollama").args(args).stdout(Stdio::null()).stderr(Stdio::null()).status()
}
