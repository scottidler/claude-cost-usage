use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ccu",
    about = "Claude Code cost and usage tracker",
    version = env!("GIT_DESCRIBE"),
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

    /// Set log level (trace, debug, info, warn, error)
    #[arg(long, env = "CCU_LOG_LEVEL")]
    pub log_level: Option<String>,

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

        /// Output only the total cost as a plain number
        #[arg(short, long)]
        total: bool,

        /// Show per-session breakdown
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show yesterday's total cost
    Yesterday {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Output only the total cost as a plain number
        #[arg(short, long)]
        total: bool,

        /// Show per-session breakdown
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show daily costs for a date range
    Daily {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Output only the total cost as a plain number
        #[arg(short, long)]
        total: bool,

        /// Number of days to show
        #[arg(short, long, default_value = "7")]
        days: u32,

        /// Show partial-period-weighted average
        #[arg(short, long)]
        average: bool,

        /// Show inline bar charts and braille line chart
        #[arg(short, long)]
        graph: bool,
    },
    /// Show weekly cost summary
    Weekly {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Output only the total cost as a plain number
        #[arg(short, long)]
        total: bool,

        /// Number of weeks to show
        #[arg(short, long, default_value = "4")]
        weeks: u32,

        /// Show partial-period-weighted average
        #[arg(short, long)]
        average: bool,

        /// Show inline bar charts and braille line chart
        #[arg(short, long)]
        graph: bool,
    },
    /// Show monthly cost summary
    Monthly {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,

        /// Output only the total cost as a plain number
        #[arg(short, long)]
        total: bool,

        /// Number of months to show
        #[arg(short, long, default_value = "12", value_parser = clap::value_parser!(u32).range(1..))]
        months: u32,

        /// Show partial-period-weighted average
        #[arg(short, long)]
        average: bool,

        /// Show inline bar charts and braille line chart
        #[arg(short, long)]
        graph: bool,
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
