//! CloakPipe CLI — entrypoint for the privacy proxy.

mod commands;
mod presets;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "cloakpipe")]
#[command(about = "Privacy middleware for LLM & RAG pipelines")]
#[command(version)]
struct Cli {
    /// Path to configuration file or bundled preset name (e.g. dpdp.toml)
    #[arg(short, long, default_value = "cloakpipe.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the CloakPipe proxy server
    Start,
    /// Test detection on sample text
    Test {
        /// Text to test detection on
        #[arg(short, long)]
        text: Option<String>,
        /// File to read test text from
        #[arg(short, long)]
        file: Option<String>,
    },
    /// Show vault statistics
    Stats,
    /// Initialize a new cloakpipe.toml config file
    Init,
    /// Interactive guided setup (industry profiles, detection tuning)
    Setup,
    /// Manage bundled configuration presets
    Presets {
        #[command(subcommand)]
        action: PresetCommands,
    },
    /// Start as an MCP server (for agent integrations)
    Mcp,
    /// CloakTree: vectorless document retrieval
    Tree {
        #[command(subcommand)]
        action: TreeCommands,
    },
    /// ADCPE: encrypt/decrypt embedding vectors
    Vector {
        #[command(subcommand)]
        action: VectorCommands,
    },
    /// Manage active sessions (context-aware pseudonymization)
    Sessions {
        #[command(subcommand)]
        action: SessionCommands,
    },
    /// NER backend helpers (install sidecar dependencies, etc.)
    Ner {
        #[command(subcommand)]
        action: NerCommands,
    },
    /// Scan files/directories for PII (RAG pre-indexing pipeline)
    Scan {
        /// Input file or directory (recursively scans .txt, .md, .json, .csv)
        input: String,
        /// Output directory for masked files (default: <input>-masked)
        #[arg(short, long)]
        output: Option<String>,
        /// Masking strategy: token or format-preserving
        #[arg(long, default_value = "token")]
        strategy: String,
        /// Only detect, don't mask (prints report)
        #[arg(long)]
        detect_only: bool,
        /// Minimum confidence threshold (0.0–1.0)
        #[arg(long, default_value = "0.5")]
        min_confidence: f64,
    },
}

#[derive(Subcommand)]
pub enum PresetCommands {
    /// Install bundled presets into the user config directory
    Install,
    /// List bundled presets and where they resolve from
    List,
}

#[derive(Subcommand)]
pub enum NerCommands {
    /// Install a supported NER backend locally
    Install {
        /// Backend to install
        #[arg(long, value_enum, default_value_t = NerInstallBackend::GlinerPii)]
        backend: NerInstallBackend,
        /// Print the install command without running it
        #[arg(long)]
        dry_run: bool,
        /// Python interpreter to use (defaults to python3/python/py auto-detect)
        #[arg(long)]
        python: Option<String>,
        /// Skip verifying that the backend imports after install
        #[arg(long)]
        no_verify: bool,
    },
    /// Start a supported NER backend sidecar locally
    Start {
        /// Backend to start
        #[arg(long, value_enum, default_value_t = NerInstallBackend::GlinerPii)]
        backend: NerInstallBackend,
        /// Python interpreter to use (defaults to the managed venv when present)
        #[arg(long)]
        python: Option<String>,
        /// Host to bind the sidecar to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind the sidecar to
        #[arg(long, default_value_t = 9111)]
        port: u16,
        /// Confidence threshold passed to the sidecar
        #[arg(long, default_value_t = 0.4)]
        threshold: f64,
        /// Print the launch command without running it
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum NerInstallBackend {
    #[value(alias = "gliner_pii")]
    GlinerPii,
}

#[derive(Subcommand)]
pub enum TreeCommands {
    /// Build a tree index from a document
    Index {
        /// Path to the document (PDF, TXT, MD)
        file: String,
        /// Skip LLM-generated summaries (offline mode)
        #[arg(long)]
        no_summaries: bool,
    },
    /// Search a tree index with a natural language query
    Search {
        /// Path to the tree index JSON file
        index: String,
        /// The search query
        query: String,
    },
    /// List all tree indices
    List,
    /// Query a document end-to-end (index + search + extract + answer)
    Query {
        /// Path to the document (or existing tree index JSON)
        file: String,
        /// The question to answer
        question: String,
    },
    /// Show tree index details
    Show {
        /// Path to the tree index JSON file
        index: String,
    },
}

#[derive(Subcommand)]
pub enum SessionCommands {
    /// List all active sessions
    List,
    /// Inspect a session's entity map and coreferences
    Inspect {
        /// Session ID to inspect
        session_id: String,
    },
    /// Flush (delete) a specific session
    Flush {
        /// Session ID to flush
        session_id: String,
    },
    /// Flush all sessions
    FlushAll,
}

#[derive(Subcommand)]
pub enum VectorCommands {
    /// Encrypt embedding vectors from a JSON file
    Encrypt {
        /// Input JSON file (array of float arrays)
        input: String,
        /// Output file for encrypted vectors
        output: String,
        /// Vector dimensions
        #[arg(long, default_value = "1536")]
        dim: usize,
    },
    /// Decrypt embedding vectors
    Decrypt {
        /// Input JSON file (encrypted vectors)
        input: String,
        /// Output file for decrypted vectors
        output: String,
        /// Vector dimensions
        #[arg(long, default_value = "1536")]
        dim: usize,
    },
    /// Test ADCPE: encrypt sample vectors and verify distance preservation
    Test {
        /// Vector dimensions to test
        #[arg(long, default_value = "8")]
        dim: usize,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cloakpipe=info,tower_http=info".into()),
        )
        .init();

    match cli.command {
        Commands::Start => commands::start(&cli.config).await,
        Commands::Test { text, file } => commands::test(&cli.config, text, file).await,
        Commands::Stats => commands::stats(&cli.config).await,
        Commands::Init => commands::init().await,
        Commands::Setup => commands::setup().await,
        Commands::Presets { action } => commands::presets(action).await,
        Commands::Mcp => commands::mcp(&cli.config).await,
        Commands::Tree { action } => commands::tree(&cli.config, action).await,
        Commands::Vector { action } => commands::vector(action).await,
        Commands::Sessions { action } => commands::sessions(&cli.config, action).await,
        Commands::Ner { action } => commands::ner(action).await,
        Commands::Scan {
            input,
            output,
            strategy,
            detect_only,
            min_confidence,
        } => {
            commands::scan(
                &cli.config,
                input,
                output,
                strategy,
                detect_only,
                min_confidence,
            )
            .await
        }
    }
}
