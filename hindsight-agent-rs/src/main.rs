mod api;
mod commands;
mod config;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hindsight-agent")]
#[command(about = "Agent CLI for Hindsight Wiki — self-learning knowledge pages for AI agents")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Set up a new agent with Hindsight memory
    Setup {
        /// Agent identifier (e.g., your Hermes profile name or OpenClaw agent name)
        agent_id: String,

        /// Hindsight bank ID for this agent
        #[arg(long)]
        bank_id: String,

        /// Hindsight API URL
        #[arg(long, default_value = "http://localhost:8888", env = "HINDSIGHT_API_URL")]
        api_url: String,

        /// Hindsight API token (for cloud/authenticated instances)
        #[arg(long, env = "HINDSIGHT_API_TOKEN")]
        api_token: Option<String>,

        /// Agent harness
        #[arg(long, value_parser = ["hermes", "openclaw"])]
        harness: String,

        /// Bank template JSON file to import
        #[arg(long)]
        template: Option<String>,

        /// Directory of files to ingest at setup time
        #[arg(long)]
        content: Option<String>,
    },

    /// Manage configured agents
    Agents {
        #[command(subcommand)]
        command: AgentsCommands,
    },

    /// Manage wiki pages (knowledge that evolves from conversations)
    Wiki {
        #[command(subcommand)]
        command: WikiCommands,
    },

    /// Search agent memories
    Recall {
        /// Agent identifier
        agent_id: String,

        /// Search query
        query: String,

        /// Maximum results to return
        #[arg(short = 'n', long, default_value = "10")]
        max_results: u32,

        /// Filter by fact type (repeatable: observation, world, experience)
        #[arg(long = "type")]
        types: Vec<String>,
    },

    /// Ingest a document into agent memory
    Ingest {
        /// Agent identifier
        agent_id: String,

        /// Document title (used as document ID for upsert)
        title: String,

        /// Read content from a file
        #[arg(short = 'f', long = "file")]
        file_path: Option<String>,

        /// Inline content string
        #[arg(short = 'c', long = "content")]
        inline_content: Option<String>,
    },

    /// List documents retained for an agent
    Documents {
        /// Agent identifier
        agent_id: String,
    },

    /// Retain raw content (used by harness plugins)
    Retain {
        /// Agent identifier
        agent_id: String,

        /// Read content from a file (reads stdin if omitted)
        #[arg(long)]
        input: Option<String>,

        /// Document ID for upsert behavior
        #[arg(long)]
        document_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentsCommands {
    /// List all configured agents
    List,
    /// Show details for a specific agent
    Show {
        /// Agent identifier
        agent_id: String,
    },
}

#[derive(Subcommand)]
enum WikiCommands {
    /// List all wiki pages
    List {
        /// Agent identifier
        agent_id: String,
    },
    /// Get a specific wiki page
    Get {
        /// Agent identifier
        agent_id: String,
        /// Page identifier
        page_id: String,
    },
    /// Create a new wiki page
    Create {
        /// Agent identifier
        agent_id: String,
        /// Page identifier (lowercase with hyphens)
        page_id: String,
        /// Page name
        name: String,
        /// Synthesis query — the question the system re-asks to rebuild this page
        source_query: String,
    },
    /// Update a wiki page
    Update {
        /// Agent identifier
        agent_id: String,
        /// Page identifier
        page_id: String,
        /// New page name
        #[arg(long)]
        name: Option<String>,
        /// New synthesis query
        #[arg(long)]
        source_query: Option<String>,
    },
    /// Delete a wiki page
    Delete {
        /// Agent identifier
        agent_id: String,
        /// Page identifier
        page_id: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Setup {
            agent_id,
            bank_id,
            api_url,
            api_token,
            harness,
            template,
            content,
        } => commands::setup::setup(
            &agent_id,
            &bank_id,
            &api_url,
            api_token.as_deref(),
            &harness,
            template.as_deref(),
            content.as_deref(),
        ),

        Commands::Agents { command } => match command {
            AgentsCommands::List => commands::agents::list(),
            AgentsCommands::Show { agent_id } => commands::agents::show(&agent_id),
        },

        Commands::Wiki { command } => match command {
            WikiCommands::List { agent_id } => commands::wiki::list(&agent_id),
            WikiCommands::Get { agent_id, page_id } => commands::wiki::get(&agent_id, &page_id),
            WikiCommands::Create {
                agent_id,
                page_id,
                name,
                source_query,
            } => commands::wiki::create(&agent_id, &page_id, &name, &source_query),
            WikiCommands::Update {
                agent_id,
                page_id,
                name,
                source_query,
            } => commands::wiki::update(
                &agent_id,
                &page_id,
                name.as_deref(),
                source_query.as_deref(),
            ),
            WikiCommands::Delete { agent_id, page_id } => {
                commands::wiki::delete(&agent_id, &page_id)
            }
        },

        Commands::Recall {
            agent_id,
            query,
            max_results,
            types,
        } => commands::recall::recall(&agent_id, &query, max_results, &types),

        Commands::Ingest {
            agent_id,
            title,
            file_path,
            inline_content,
        } => commands::ingest::ingest(
            &agent_id,
            &title,
            file_path.as_deref(),
            inline_content.as_deref(),
        ),

        Commands::Documents { agent_id } => commands::documents::list(&agent_id),

        Commands::Retain {
            agent_id,
            input,
            document_id,
        } => commands::retain::retain(&agent_id, input.as_deref(), document_id.as_deref()),
    };

    if let Err(e) = result {
        eprintln!("Error: {:#}", e);
        std::process::exit(1);
    }
}
