//! CloakPipe CLI — entrypoint for the privacy proxy.

mod commands;
mod presets;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "cloakpipe")]
#[command(about = "Privacy middleware for LLM & RAG pipelines")]
#[command(version)]
struct Cli {
    /// Path to configuration file or bundled preset name (e.g. dpdp.toml).
    /// Omit to discover the nearest project config, then global ~/.cloakpipe/cloakpipe.toml.
    #[arg(short, long)]
    config: Option<String>,

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
    /// Create or edit policy files interactively
    Policy {
        #[command(subcommand)]
        action: PolicyCommands,
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
    /// NER backend helpers (download models, start sidecars, etc.)
    Ner {
        #[command(subcommand)]
        action: NerCommands,
    },
    /// Explicit HTTP proxy helpers (CA setup for HTTPS inspection)
    HttpProxy {
        #[command(subcommand)]
        action: HttpProxyCommands,
    },
    /// Scan files/directories for PII (RAG pre-indexing pipeline)
    Scan {
        /// Input file or directory (recursively scans .txt, .md, .json, .csv)
        input: String,
        /// Output directory for masked files (default: <input>-masked)
        #[arg(short, long)]
        output: Option<String>,
        /// Masking strategy: similar, token, or format-preserving
        #[arg(long, default_value = "similar")]
        strategy: String,
        /// Only detect, don't mask (prints report)
        #[arg(long)]
        detect_only: bool,
        /// Minimum confidence threshold (0.0–1.0)
        #[arg(long, default_value = "0.5")]
        min_confidence: f64,
        /// Disable NER detection for this scan
        #[arg(long)]
        no_ner: bool,
    },
    /// Restore a masked file or directory using exported vault mappings
    Restore {
        /// Masked input file or directory
        input: String,
        /// Output file or directory. File input prints to stdout when omitted.
        #[arg(short, long)]
        output: Option<String>,
        /// Path to vault-mappings.json. Defaults to the input directory.
        #[arg(short, long)]
        mappings: Option<String>,
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
pub enum PolicyCommands {
    /// Create or edit the active policy file interactively
    Edit,
}

#[derive(Subcommand)]
pub enum NerCommands {
    /// Start a supported NER backend sidecar locally
    Start {
        /// Backend to start
        #[arg(long, value_enum, default_value_t = NerStartBackend::GlinerPii)]
        backend: NerStartBackend,
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
    /// Download or bootstrap a supported NER model/runtime
    Download {
        /// Model/runtime to download or bootstrap
        #[arg(long, value_enum, default_value_t = NerDownloadModel::DistilbertPii)]
        model: NerDownloadModel,
        /// Re-download even if DistilBERT-PII model files already exist
        #[arg(long)]
        force: bool,
        /// Print the download/setup command without running it
        #[arg(long)]
        dry_run: bool,
        /// Python interpreter to use for the managed GLiNER-PII runtime
        #[arg(long)]
        python: Option<String>,
        /// Skip verifying that GLiNER-PII imports after download/setup
        #[arg(long)]
        no_verify: bool,
    },
    /// Show the status of downloaded NER models
    Status {
        /// Print the status as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum NerStartBackend {
    #[value(alias = "gliner_pii")]
    GlinerPii,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum NerDownloadModel {
    Bert,
    Gliner,
    #[value(alias = "gliner_pii")]
    GlinerPii,
    #[value(alias = "distilbert_pii")]
    DistilbertPii,
}

#[derive(Subcommand)]
pub enum HttpProxyCommands {
    /// Manage the local root CA used by opt-in HTTPS inspection
    Ca {
        #[command(subcommand)]
        action: HttpProxyCaCommands,
    },
}

#[derive(Subcommand)]
pub enum HttpProxyCaCommands {
    /// Generate the local CloakPipe root CA and host certificate cache directory
    Init {
        /// Overwrite an existing CloakPipe CA certificate/key pair
        #[arg(long)]
        force: bool,
        /// Print what would be created without writing files
        #[arg(long)]
        dry_run: bool,
    },
    /// Show CA file and trust status
    Status,
    /// Print CA certificate, key, and cache paths
    PrintPath,
    /// Print platform-specific trust-store install instructions
    Install {
        /// Platform instructions to print
        #[arg(long, value_enum)]
        platform: Option<CaInstallPlatform>,
    },
    /// Best-effort trust-store install; requires --yes
    Trust {
        /// Confirm that CloakPipe may modify the user trust store
        #[arg(long)]
        yes: bool,
    },
    /// Best-effort trust-store removal; requires --yes
    Untrust {
        /// Confirm that CloakPipe may modify the user trust store
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum CaInstallPlatform {
    Macos,
    Linux,
    Windows,
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
    let config = cli.config.clone();
    let config = config.as_deref();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cloakpipe=info,tower_http=info".into()),
        )
        .init();

    match cli.command {
        Commands::Start => commands::start(config).await,
        Commands::Test { text, file } => commands::test(config, text, file).await,
        Commands::Stats => commands::stats(config).await,
        Commands::Init => commands::init().await,
        Commands::Setup => commands::setup().await,
        Commands::Presets { action } => commands::presets(action).await,
        Commands::Policy { action } => commands::policy(config, action).await,
        Commands::Mcp => commands::mcp(config).await,
        Commands::Tree { action } => commands::tree(config, action).await,
        Commands::Vector { action } => commands::vector(action).await,
        Commands::Sessions { action } => commands::sessions(config, action).await,
        Commands::Ner { action } => commands::ner(action).await,
        Commands::HttpProxy { action } => commands::http_proxy(config, action).await,
        Commands::Scan {
            input,
            output,
            strategy,
            detect_only,
            min_confidence,
            no_ner,
        } => {
            commands::scan(
                config,
                input,
                output,
                strategy,
                detect_only,
                min_confidence,
                no_ner,
            )
            .await
        }
        Commands::Restore {
            input,
            output,
            mappings,
        } => commands::restore(input, output, mappings).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn clap_command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn policy_edit_command_parses() {
        let cli = Cli::parse_from(["cloakpipe", "policy", "edit"]);

        assert!(matches!(
            cli.command,
            Commands::Policy {
                action: PolicyCommands::Edit
            }
        ));
    }

    #[test]
    fn scan_no_ner_flag_parses() {
        let cli = Cli::parse_from([
            "cloakpipe",
            "scan",
            "assets/example.md",
            "--detect-only",
            "--no-ner",
        ]);

        assert!(matches!(cli.command, Commands::Scan { no_ner: true, .. }));
    }

    #[test]
    fn ner_status_command_parses() {
        let cli = Cli::parse_from(["cloakpipe", "ner", "status"]);

        assert!(matches!(
            cli.command,
            Commands::Ner {
                action: NerCommands::Status { json: false }
            }
        ));
    }

    #[test]
    fn ner_status_json_flag_parses() {
        let cli = Cli::parse_from(["cloakpipe", "ner", "status", "--json"]);

        assert!(matches!(
            cli.command,
            Commands::Ner {
                action: NerCommands::Status { json: true }
            }
        ));
    }

    #[test]
    fn ner_download_defaults_to_distilbert_pii() {
        let cli = Cli::parse_from(["cloakpipe", "ner", "download"]);

        assert!(matches!(
            cli.command,
            Commands::Ner {
                action: NerCommands::Download {
                    model: NerDownloadModel::DistilbertPii,
                    ..
                }
            }
        ));
    }

    #[test]
    fn ner_download_model_flag_parses() {
        let cli = Cli::parse_from(["cloakpipe", "ner", "download", "--model", "gliner_pii"]);

        assert!(matches!(
            cli.command,
            Commands::Ner {
                action: NerCommands::Download {
                    model: NerDownloadModel::GlinerPii,
                    ..
                }
            }
        ));
    }

    #[test]
    fn http_proxy_ca_init_command_parses() {
        let cli = Cli::parse_from(["cloakpipe", "http-proxy", "ca", "init", "--dry-run"]);

        assert!(matches!(
            cli.command,
            Commands::HttpProxy {
                action: HttpProxyCommands::Ca {
                    action: HttpProxyCaCommands::Init { dry_run: true, .. }
                }
            }
        ));
    }
}
