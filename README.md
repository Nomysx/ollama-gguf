# ollama-gguf

Install GGUF models into [Ollama](https://ollama.com) from a Hugging Face link or a local file.


```
ollama-gguf https://huggingface.co/ZY-Lee4/QJev3.5-0.8B-GGUF
```


## Start

1. Install Ollama.
2. Double-click `dist\ollama-gguf.exe`, paste a Hugging Face link, or press Enter to pick a local `.gguf` file.


## Use it from CMD or PowerShell

Run this once:

```
dist\ollama-gguf.exe --add-to-path
```

It copies the exe to `%LOCALAPPDATA%\Programs\ollama-gguf` and adds that folder to your user PATH.
Open a new terminal window and `ollama-gguf` works from anywhere.

## Examples

```
ollama-gguf                                               step-by-step wizard
ollama-gguf https://huggingface.co/user/repo              pick a file from a menu
ollama-gguf user/repo --list                              show the GGUF files and exit
ollama-gguf user/repo --file Q4_K_M --name mymodel        no questions asked
ollama-gguf https://huggingface.co/user/repo/blob/main/model-Q8_0.gguf
ollama-gguf "D:\models\my model.gguf" --name mymodel      local file
ollama-gguf user/base-repo --file Q8_0 --adapter lora-v2  base model plus a LoRA adapter
```

## Options

| Option | What it does |
|---|---|
| `SOURCE` | Hugging Face link, `user/repo` name or local `.gguf` file. Leave it out for the wizard |
| `-f, --file NAME` | GGUF file to install, full or partial name like `Q4_K_M` |
| `-n, --name NAME` | Model name in Ollama, by default taken from the file name |
| `-a, --adapter NAME` | LoRA adapter to apply, a file from the same repo or a local `.gguf` path |
| `-l, --list` | List the GGUF files in the repo and exit |
| `--keep` | Keep the downloaded files after install |
| `--token TOKEN` | Hugging Face token for private or gated repos, or set `HF_TOKEN` |
| `--add-to-path` | Copy the exe to your user folder and add it to PATH |

When you give a source on the command line the tool only asks what it cannot decide,
which file to use when several match. Without a source the wizard asks everything:
file, optional LoRA adapter and model name, and confirms before replacing an existing model.

## How it works

1. Reads the file list and sizes from the Hugging Face API.
2. Downloads the file to `%LOCALAPPDATA%\ollama-gguf\downloads`. An interrupted download
   continues where it stopped when you run the same command again.
3. Writes a Modelfile (`FROM <file>` plus `ADAPTER <file>` for LoRA) and runs `ollama create`.
4. Deletes the download, because Ollama stores its own copy. Use `--keep` to keep it.
   If `ollama create` fails, the download is kept and reused on the next run.

Local files are never moved or deleted.

## Which files are offered

| File | Handling |
|---|---|
| Models like `model-Q4_K_M.gguf` | offered in the menu |
| LoRA adapters (`lora` or `adapter` in the name) | offered as an optional extra in the wizard, or with `--adapter` |
| Split models (`-00001-of-00003.gguf`) | skipped, Ollama cannot import models split into parts |
| Vision projectors (`mmproj`) | skipped |

## Notes

- A LoRA adapter only works with the base model it was trained on. Many repos also ship
  merged models that already contain the adapter, so do not stack an adapter on those.
- For gated models, accept the terms on the model page, create a token at
  huggingface.co/settings/tokens and pass it with `--token` or the `HF_TOKEN` variable.
- HTTPS uses the Windows certificate store and Windows proxy settings, so it also works
  behind antivirus HTTPS scanning or a company proxy.

## Build from source

Needs [Rust](https://rustup.rs).

```
.\build.ps1
```

Runs the tests, builds the release exe and copies it to `dist\`. The C runtime is linked
statically (see `.cargo\config.toml`), so the exe runs on any 64-bit Windows 10 or 11 PC.

| File | Role |
|---|---|
| `src\main.rs` | command line options and the install flow |
| `src\source.rs` | decides whether the input is a link, a repo name or a local file |
| `src\hf.rs` | Hugging Face links, file list and resumable downloads |
| `src\gguf.rs` | file name rules: kind of file, matching, model names |
| `src\ollama.rs` | Modelfile and `ollama create` |
| `src\ui.rs` | menus, prompts, progress bar and messages |
| `src\setup.rs` | `--add-to-path` |

## Uninstall

Delete `%LOCALAPPDATA%\Programs\ollama-gguf` and remove that folder from your user PATH
(Start > "Edit environment variables for your account" > Path).
