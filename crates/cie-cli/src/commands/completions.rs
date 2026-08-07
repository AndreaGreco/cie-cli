//! Completamento automatico per bash, zsh e fish.

use crate::Cli;
use anyhow::{Context, Result, bail};
use clap::CommandFactory;
use clap_complete::Shell;
use std::path::PathBuf;
use std::{env, fs, io};

pub fn execute(shell: Option<Shell>, install: bool) -> Result<()> {
    let shell = shell
        .or_else(Shell::from_env)
        .context("shell non riconosciuta da $SHELL: indicala tu (bash, zsh, fish)")?;

    let mut cmd = Cli::command();
    if !install {
        clap_complete::generate(shell, &mut cmd, "cie", &mut io::stdout());
        return Ok(());
    }

    let path = destinazione(shell)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creazione di {}", dir.display()))?;
    }
    let mut file =
        fs::File::create(&path).with_context(|| format!("scrittura di {}", path.display()))?;
    clap_complete::generate(shell, &mut cmd, "cie", &mut file);

    println!("Completamento {shell} installato in {}", path.display());
    match shell {
        // bash-completion e fish pescano da soli dalle directory standard.
        Shell::Zsh => println!(
            "Aggiungi a ~/.zshrc:  fpath+=({})  &&  autoload -U compinit && compinit",
            path.parent().unwrap_or(&path).display()
        ),
        _ => println!("Apri una nuova shell per usarlo."),
    }
    Ok(())
}

/// Directory che la shell scandaglia da sola all'avvio, secondo XDG.
fn destinazione(shell: Shell) -> Result<PathBuf> {
    let base = |var: &str, fallback: &str| -> Result<PathBuf> {
        if let Ok(d) = env::var(var) {
            return Ok(PathBuf::from(d));
        }
        Ok(PathBuf::from(env::var("HOME").context("$HOME non impostata")?).join(fallback))
    };

    Ok(match shell {
        Shell::Bash => base("XDG_DATA_HOME", ".local/share")?
            .join("bash-completion/completions")
            .join("cie"),
        Shell::Zsh => base("XDG_DATA_HOME", ".local/share")?
            .join("zsh/site-functions")
            .join("_cie"),
        Shell::Fish => base("XDG_CONFIG_HOME", ".config")?
            .join("fish/completions")
            .join("cie.fish"),
        // ponytail: le altre shell stampano e basta, nessuno le usa qui.
        altra => bail!(
            "installazione automatica non prevista per {altra}: usa `cie completions {altra} > <file>`"
        ),
    })
}
