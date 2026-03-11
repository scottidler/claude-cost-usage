use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ccu",
    about = "Claude Code cost and usage tracker",
    version = env!("GIT_DESCRIBE"),
    after_help = "Parses Claude Code JSONL session logs to compute cost summaries."
)]
pub struct Cli {
    /// Path to config file
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Override ~/.claude/projects/ scan path
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Filter to a specific model
    #[arg(long)]
    pub model: Option<String>,

    /// Skip the cost cache, recompute from JSONL
    #[arg(long)]
    pub no_cache: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show cost for a specific session (by ID or "current")
    Session {
        /// Session ID or "current"
        id: String,
    },
    /// Show today's total cost (default)
    Today {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Show per-session breakdown
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show yesterday's total cost
    Yesterday {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Show per-session breakdown
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show daily costs for a date range
    Daily {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Number of days to show
        #[arg(short, long, default_value = "7")]
        days: u32,
    },
    /// Show weekly cost summary
    Weekly {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Number of weeks to show
        #[arg(short, long, default_value = "4")]
        weeks: u32,
    },
    /// Show monthly cost summary
    Monthly {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,
    },
    /// Manage model pricing configuration
    Pricing {
        /// Fetch current pricing from Anthropic and update config
        #[arg(long)]
        update: bool,

        /// Display current pricing table
        #[arg(long)]
        show: bool,

        /// Read pricing from a local markdown file instead of fetching
        #[arg(long)]
        from: Option<PathBuf>,
    },
}
