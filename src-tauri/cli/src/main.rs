//! `jotter`: search, read, and edit a Jotter vault from the command line, or serve it to an
//! MCP client with `jotter mcp`. Answers are JSON. Errors go to standard error as
//! `{"error": "..."}` with exit code 1.

mod mcp;
mod vaults;

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use jotter_core::agent::AgentVault;
use serde_json::{json, Value};

#[derive(Parser)]
#[command(name = "jotter", version, about = "Work with a Jotter vault from the command line")]
struct Cli {
    /// The vault folder. Defaults to the vault last opened in the Jotter app.
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
    /// Print the shared context note that every AI assistant reads first.
    Context,
    /// Replace the shared context note. Reads standard input unless --text is given.
    SetContext {
        #[arg(long)]
        text: Option<String>,
    },
    /// Replace exact text in a note. The text must appear once unless --all is given.
    Edit {
        path: String,
        #[arg(long)]
        old: String,
        #[arg(long)]
        new: String,
        /// Replace every occurrence.
        #[arg(long)]
        all: bool,
    },
    /// Set or remove frontmatter properties. Values are JSON; plain text counts as a string.
    Props {
        path: String,
        /// key=value, such as status=done or rating=3. Repeat for more.
        #[arg(long = "set", value_name = "KEY=VALUE")]
        set: Vec<String>,
        /// A property to remove. Repeat for more.
        #[arg(long = "remove", value_name = "KEY")]
        remove: Vec<String>,
    },
    /// Add or remove tags in a note's frontmatter.
    Tag {
        path: String,
        #[arg(long = "add", value_name = "TAG")]
        add: Vec<String>,
        #[arg(long = "remove", value_name = "TAG")]
        remove: Vec<String>,
    },
    /// Add text to today's daily note. Reads standard input unless --text is given.
    Daily {
        #[arg(long)]
        text: Option<String>,
    },
    /// Save notes to the conversation note with this title. Reads standard input unless --text is given.
    SaveConversation {
        title: String,
        #[arg(long)]
        text: Option<String>,
    },
    /// List the vaults the Jotter app has opened, newest first.
    Vaults,
    /// Serve the vault to an MCP client over standard input and output.
    Mcp {
        /// Stay on this vault: hide list_vaults and refuse open_vault.
        #[arg(long)]
        lock_vault: bool,
    },
}

fn run(cli: Cli) -> Result<Value, String> {
    let data_dir = jotter_core::platform::app_data_dir()?;
    let root = cli.vault.or_else(|| vaults::last_opened(&data_dir));
    if let Command::Vaults = cli.command {
        // An empty recent list is the only way to have no vault here, so there is nothing to list.
        return Ok(root.map_or(json!([]), |root| vaults::list(&data_dir, &root, &root)));
    }
    let root = match root {
        Some(root) => root,
        None => {
            return Err("No vault given and the Jotter app has not opened one. Give the vault folder with --vault or the JOTTER_VAULT variable"
                .to_string())
        }
    };
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
        Command::Context => agent.context(),
        Command::SetContext { text } => agent.set_context(&text_or_stdin(text)?),
        Command::Edit { path, old, new, all } => agent.edit(&path, &old, &new, all),
        Command::Props { path, set, remove } => agent.set_properties(&path, &parse_props(&set)?, &remove),
        Command::Tag { path, add, remove } => agent.update_tags(&path, &add, &remove),
        Command::Daily { text } => agent.append_to_daily(&text_or_stdin(text)?),
        Command::SaveConversation { title, text } => agent.save_conversation(&title, &text_or_stdin(text)?),
        Command::Vaults => unreachable!("handled before the vault opens"),
        Command::Mcp { lock_vault } => {
            mcp::serve(mcp::Session::new(agent, cli.read_only, data_dir).locked_to_start_vault(lock_vault))?;
            Ok(Value::Null)
        }
    }
}

/// Reads `key=value` pairs. A value that parses as JSON keeps its type; anything else is text.
fn parse_props(pairs: &[String]) -> Result<serde_json::Map<String, Value>, String> {
    pairs
        .iter()
        .map(|pair| {
            let (key, value) = pair.split_once('=').ok_or_else(|| format!("'{pair}' needs the form key=value"))?;
            let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
            Ok((key.trim().to_string(), value))
        })
        .collect()
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
