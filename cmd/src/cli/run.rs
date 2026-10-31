use color_eyre::Result;
use colored::Colorize;
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;

use crate::chat::ChatOptions;
use crate::cli::args::{LoadArgs, SampleArgs, ThinkArgs};
use crate::cli::install::{self, Resolved};
use crate::cli::models::{self, Lookup};

#[derive(clap::Args)]
pub struct RunArgs {
    /// Installed model id, path to a .gguf file, or a registry / Hugging Face id to download
    /// (default: the active chat model)
    pub model: Option<String>,
    /// Prompt to answer once, then exit (no prompt: piped stdin is the prompt)
    #[arg(trailing_var_arg = true)]
    pub words: Vec<String>,
    /// Prompt to answer once, then exit
    #[arg(short = 'p', long = "prompt", value_name = "TEXT", conflicts_with = "words")]
    pub prompt: Option<String>,
    /// Read the prompt from a file
    #[arg(short = 'f', long = "file", value_name = "PATH", conflicts_with_all = ["words", "prompt"])]
    pub file: Option<PathBuf>,
    /// System prompt (-sys)
    #[arg(long = "system-prompt", value_name = "TEXT")]
    pub system_prompt: Option<String>,
    /// Most tokens per answer (default: what the context leaves)
    #[arg(short = 'n', long = "n-predict", visible_alias = "predict", value_name = "N")]
    pub n_predict: Option<usize>,
    /// Keep chatting after answering the prompt
    #[arg(short = 'i', long)]
    pub interactive: bool,
    /// Load the model in this process even when `cluaiz serve` is running (default: use the server)
    #[arg(long)]
    pub local: bool,
    /// When downloading: pick any files of the repository, not only the model formats
    #[arg(short = 'a', long = "all")]
    pub all: bool,
    #[command(flatten)]
    pub load: LoadArgs,
    #[command(flatten)]
    pub sample: SampleArgs,
    #[command(flatten)]
    pub think: ThinkArgs,
}

/// `cluaiz run`: chat with a model, or answer one prompt. A model that is not installed goes through
/// the download flow below (unchanged), which ends with the command to run it.
pub async fn execute(args: RunArgs, verbose: bool) -> Result<()> {
    let name = match args.model.clone() {
        Some(name) => name,
        None => models::chat_model_or_choose()?,
    };
    let (path, id) = match models::lookup(&name) {
        Lookup::Found(path, id) => (path, id),
        Lookup::Ambiguous(_) => return models::resolve_installed(&name).map(|_| ()),
        Lookup::NotInstalled => match install::resolve(&name, args.all).await? {
            Resolved::OnDisk(path) => {
                let id = cluaiz_models::registry::InstalledStateRegistry::id_for_path(&path);
                (path, id)
            }
            Resolved::AlreadyInstalled(id) => {
                println!("  {} This variant is already downloaded as '{}'. Run it with: cluaiz run {}", "⚠️".yellow(), id.cyan(), id.green());
                return Ok(());
            }
            Resolved::Download(plan) => {
                install::confirm(&plan)?;
                let path = install::download(&plan).await?;
                // `run` makes the model it downloaded the active one (as before)
                let id = cluaiz_models::registry::InstalledStateRegistry::id_for_path(&path);
                if plan.manifest.architecture_type == "onnx" {
                    cluaiz_security::PermissionSchema::set_active_embedding_model(id);
                } else {
                    cluaiz_security::PermissionSchema::set_active_chat_model(id);
                }
                return Ok(());
            }
        },
    };

    let mut prompt = match (&args.prompt, &args.file) {
        (Some(p), _) => Some(p.clone()),
        (None, Some(file)) => Some(std::fs::read_to_string(file)?),
        (None, None) if !args.words.is_empty() => Some(args.words.join(" ")),
        _ => None,
    };
    // With no prompt given, piped stdin is the prompt (`cat notes.txt | cluaiz run m`), unless -i asks
    // to chat: then each stdin line is a message. stdin is not read when a prompt is given, so a caller
    // whose stdin never closes (IDE, service) cannot hang the command.
    if prompt.is_none() && !args.interactive && !io::stdin().is_terminal() {
        let mut piped = String::new();
        io::stdin().read_to_string(&mut piped)?;
        if !piped.trim().is_empty() {
            prompt = Some(piped.trim_end().to_string());
        }
    }

    let mut options = ChatOptions::new(&args.load, &args.sample, &args.think);
    options.system = args.system_prompt;
    options.max_tokens = args.n_predict;
    options.verbose = verbose;
    options.local = args.local;
    let interactive = args.interactive || prompt.is_none();
    crate::chat::start(path, id, options, prompt, interactive).await
}
