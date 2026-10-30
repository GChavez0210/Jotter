//! `jotter`: search, read, and edit a Jotter vault from the command line, or serve it to an
//! MCP client with `jotter mcp`. Answers are JSON. Errors go to standard error as
//! `{"error": "..."}` with exit code 1.

mod mcp;

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use jotter_core::agent::AgentVault;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "jotter", version, about = "Work with a Jotter vault from the command line")]
struct Cli {
    /// The vault folder.
    #[arg(long, global = true, env = "JOTTER_VAULT")]
    vault: Option<PathBuf>,
    /// Refuse every command that would change the vault.
    #[arg(long, global = true)]
    read_only: bool,
    /// Indent the JSON answer.
    #[arg(long, global = true)]
    pretty: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Full-text search.
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// List notes with their metadata and link counts.
    List {
        /// Only notes under this folder.
        #[arg(long)]
        folder: Option<String>,
    },
    /// Print a note.
    Read { path: String },
    /// Create a note. Reads the text from standard input unless --content is given.
    Create {
        path: String,
        #[arg(long)]
        content: Option<String>,
    },
    /// Add text to the end of a note. Reads standard input unless --text is given.
    Append {
        path: String,
        #[arg(long)]
        text: Option<String>,
        /// Create the note when it does not exist.
        #[arg(long)]
        create: bool,
    },
    /// Rename or move a note or folder, rewriting links to it.
    Rename {
        from: String,
        to: String,
        /// Leave links that point to the note unchanged.
        #[arg(long)]
        no_update_links: bool,
    },
    /// Move a note or folder to the Recycle Bin.
    Delete { path: String },
    /// Notes that link to a note.
    Backlinks { path: String },
    /// All tags, or the notes that carry one tag.
    Tags {
        #[arg(long)]
        tag: Option<String>,
    },
    /// Checklist items that have a due date.
    Tasks,
    /// Broken links, unused attachments, empty notes, and duplicate titles.
    Health,
    /// Stored versions of a note, newest first.
    Versions { path: String },
    /// Print one stored version of a note.
    Version { path: String, id: String },
    /// Serve the vault to an MCP client over standard input and output.
    Mcp,
}

fn run(cli: Cli) -> Result<Value, String> {
    let root = cli
        .vault
        .ok_or_else(|| "Give the vault folder with --vault or the JOTTER_VAULT variable".to_string())?;
    let data_dir = jotter_core::platform::app_data_dir()?;
    let agent = AgentVault::open(&root, cli.read_only, &data_dir)?;
    match cli.command {
        Command::Search { query, limit } => agent.search(&query, limit),
        Command::List { folder } => agent.list(folder.as_deref()),
        Command::Read { path } => agent.read(&path),
        Command::Create { path, content } => agent.create(&path, &text_or_stdin(content)?),
        Command::Append { path, text, create } => agent.append(&path, &text_or_stdin(text)?, create),
        Command::Rename { from, to, no_update_links } => agent.rename(&from, &to, !no_update_links),
        Command::Delete { path } => agent.delete(&path),
        Command::Backlinks { path } => agent.backlinks(&path),
        Command::Tags { tag } => agent.tags(tag.as_deref()),
        Command::Tasks => agent.tasks(),
        Command::Health => agent.health(),
        Command::Versions { path } => agent.versions(&path),
        Command::Version { path, id } => agent.version(&path, &id),
        Command::Mcp => {
            mcp::serve(&agent, cli.read_only)?;
            Ok(Value::Null)
        }
    }
}

fn text_or_stdin(text: Option<String>) -> Result<String, String> {
    match text {
        Some(text) => Ok(text),
        None => {
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|e| format!("Cannot read standard input: {e}"))?;
            Ok(buffer)
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let pretty = cli.pretty;
    match run(cli) {
        Ok(Value::Null) => ExitCode::SUCCESS,
        Ok(value) => {
            let text = if pretty { serde_json::to_string_pretty(&value) } else { serde_json::to_string(&value) };
            println!("{}", text.unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{}", json!({ "error": message }));
            ExitCode::FAILURE
        }
    }
}
