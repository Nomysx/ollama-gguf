// terminal output and prompts, status goes to stderr so stdout stays clean for --list

use std::fmt::Display;
use std::io::{self, IsTerminal};

use anyhow::{Result, anyhow};
use console::{Term, style};
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Input, Select};
use indicatif::{DecimalBytes, ProgressBar, ProgressStyle};
use windows_sys::Win32::System::Console::GetConsoleProcessList;

pub fn banner() {
    eprintln!("{}  {}\n", style("ollama-gguf").cyan().bold(), style("install GGUF models into Ollama").dim());
}

// a main step of the install
pub fn step(message: impl Display) {
    eprintln!("{} {}", style("::").cyan().bold(), style(message).bold());
}

// a detail under the current step
pub fn note(message: impl Display) {
    eprintln!("   {}", style(message).dim());
}

pub fn success(message: impl Display) {
    eprintln!("{} {message}", style("ok").green().bold());
}

pub fn error(err: &anyhow::Error) {
    eprintln!("{} {err:#}", style("error:").red().bold());
}

// ask for a link or path, an empty answer opens the file browser
pub fn ask_source() -> Result<String> {
    let answer: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Hugging Face link or .gguf file (Enter to browse)")
        .allow_empty(true)
        .interact_text()?;
    if !answer.trim().is_empty() {
        return Ok(answer);
    }

    let path = rfd::FileDialog::new()
        .set_title("Choose a GGUF model")
        .add_filter("GGUF model", &["gguf"])
        .pick_file()
        .ok_or_else(|| anyhow!("no file selected"))?;
    note(format!("selected {}", path.display()));
    Ok(path.display().to_string())
}

// arrow key menu, esc cancels
pub fn pick(prompt: &str, items: &[String]) -> Result<usize> {
    Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(items)
        .default(0)
        .max_length(12)
        .interact_opt()?
        .ok_or_else(|| anyhow!("cancelled"))
}

// text question, enter keeps the default
pub fn ask(prompt: &str, default: &str) -> Result<String> {
    let answer: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(default.to_string())
        .interact_text()?;
    Ok(answer.trim().to_string())
}

// yes or no question, enter means no
pub fn confirm(prompt: &str) -> Result<bool> {
    Ok(Confirm::with_theme(&ColorfulTheme::default()).with_prompt(prompt).default(false).interact()?)
}

// progress bar for a download that may resume from bytes already on disk
pub fn download_bar(total: u64, done: u64) -> ProgressBar {
    let style = ProgressStyle::with_template(
        "   [{bar:30.cyan/blue}] {decimal_bytes}/{decimal_total_bytes}  {decimal_bytes_per_sec}  eta {eta}",
    )
    .expect("valid progress template")
    .progress_chars("=> ");
    let bar = ProgressBar::new(total).with_style(style).with_position(done);
    bar.reset_eta();
    bar
}

// size in decimal units, the way hugging face shows it
pub fn size(bytes: u64) -> String {
    DecimalBytes(bytes).to_string()
}

// true when the console window belongs to this process alone, meaning a double-click or a file drop on the exe
pub fn launched_from_explorer() -> bool {
    let mut ids = [0u32; 2];
    // safety: the buffer is valid for ids.len() entries
    unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) == 1 }
}

pub fn wait_for_enter() {
    if !io::stdin().is_terminal() {
        return;
    }
    eprint!("\n{}", style("Press Enter to exit").dim());
    let _ = io::stdin().read_line(&mut String::new());
}

// ctrl+c inside a menu would leave the cursor hidden, so show it before exiting
pub fn restore_cursor_on_ctrl_c() {
    let _ = ctrlc::set_handler(|| {
        let _ = Term::stderr().show_cursor();
        std::process::exit(130);
    });
}
