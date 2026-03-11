#![deny(clippy::unwrap_used)]
#![deny(dead_code)]
#![deny(unused_variables)]

use chrono::{Datelike, Local, NaiveDate};
use clap::Parser;
use eyre::{Context, Result};
use log::{info, warn};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

mod cache;
mod cli;
mod config;
mod output;
mod parser;
mod pricing;
mod scanner;

use cli::{Cli, Command};
use config::Config;
use output::{DaySummary, SessionSummary};

fn setup_logging() -> Result<()> {
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ccu")
        .join("logs");

    fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    let log_file = log_dir.join("ccu.log");

    let target = Box::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .context("Failed to open log file")?,
    );

    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(target))
        .init();

    info!("Logging initialized, writing to: {}", log_file.display());
    Ok(())
}

/// Compute daily summaries from JSONL files for a date range
fn compute_summaries(
    cli: &Cli,
    config: &Config,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<(Vec<DaySummary>, Vec<SessionSummary>)> {
    let projects_dir = cli
        .path
        .clone()
        .or_else(|| config.projects_dir.clone())
        .or_else(scanner::default_projects_dir)
        .ok_or_else(|| eyre::eyre!("Could not determine Claude projects directory"))?;

    info!("Scanning: {}", projects_dir.display());

    let all_files = scanner::find_session_files(&projects_dir)?;
    let filtered = scanner::filter_by_date_range(&all_files, start, end);

    info!("Processing {} files (of {} total)", filtered.len(), all_files.len());

    // Try cache for single-day, non-verbose, no-filter queries
    let mtime_hash = cache::compute_mtime_hash(&filtered);
    if !cli.no_cache
        && !cli.verbose
        && cli.model.is_none()
        && start == end
        && let Some(cached) = cache::load_cached_day(start, mtime_hash)
    {
        let summary = DaySummary {
            date: start,
            cost: cached.cost,
            sessions: cached.sessions,
        };
        return Ok((vec![summary], Vec::new()));
    }

    let pricing_table = pricing::default_pricing_table();

    // Parse all files in parallel
    let file_paths: Vec<_> = filtered.iter().map(|f| f.path.clone()).collect();
    let all_entries: Vec<_> = file_paths
        .par_iter()
        .filter_map(|path| match parser::parse_jsonl_file(path) {
            Ok(entries) => Some(entries),
            Err(e) => {
                warn!("Failed to parse {}: {}", path.display(), e);
                None
            }
        })
        .flatten()
        .collect();

    // Group by day and session, compute costs
    let mut day_costs: BTreeMap<NaiveDate, (f64, HashSet<String>)> = BTreeMap::new();
    let mut session_costs: BTreeMap<String, (f64, usize)> = BTreeMap::new();

    for entry in &all_entries {
        let date = parser::local_date(&entry.timestamp);
        if date < start || date > end {
            continue;
        }

        // Apply model filter if specified
        if let Some(ref model_filter) = cli.model {
            let normalized = pricing::normalize_model_id(&entry.model);
            if normalized != model_filter {
                continue;
            }
        }

        let normalized = pricing::normalize_model_id(&entry.model);
        let base_pricing = match pricing_table.get(normalized) {
            Some(p) => p,
            None => {
                warn!("Unknown model: {} (normalized: {})", entry.model, normalized);
                continue;
            }
        };

        let effective_pricing = config.apply_overrides(normalized, base_pricing);
        let cost = pricing::calculate_cost(&effective_pricing, &entry.usage);

        let day_entry = day_costs.entry(date).or_insert_with(|| (0.0, HashSet::new()));
        day_entry.0 += cost;
        day_entry.1.insert(entry.session_id.clone());

        let session_entry = session_costs.entry(entry.session_id.clone()).or_insert((0.0, 0));
        session_entry.0 += cost;
        session_entry.1 += 1;
    }

    let day_summaries: Vec<DaySummary> = day_costs
        .into_iter()
        .rev()
        .map(|(date, (cost, sessions))| {
            let session_count = sessions.len();
            // Save to cache (skip if --no-cache)
            if !cli.no_cache
                && let Err(e) = cache::save_cached_day(date, cost, session_count, mtime_hash)
            {
                warn!("Failed to save cache for {}: {}", date, e);
            }
            DaySummary {
                date,
                cost,
                sessions: session_count,
            }
        })
        .collect();

    let session_summaries: Vec<SessionSummary> = session_costs
        .into_iter()
        .map(|(session_id, (cost, entries))| SessionSummary {
            session_id,
            cost,
            entries,
        })
        .collect();

    // Prune old cache entries
    if !cli.no_cache
        && let Err(e) = cache::prune_cache(90)
    {
        warn!("Failed to prune cache: {}", e);
    }

    Ok((day_summaries, session_summaries))
}

fn run(cli: &Cli, config: &Config) -> Result<()> {
    let today = Local::now().date_naive();

    match &cli.command {
        None | Some(Command::Today) => {
            let (days, sessions) = compute_summaries(cli, config, today, today)?;
            let summary = days.first().cloned().unwrap_or(DaySummary {
                date: today,
                cost: 0.0,
                sessions: 0,
            });

            if cli.json {
                println!("{}", output::format_today_json(&summary));
            } else {
                println!("{}", output::format_today_text(&summary));
                if cli.verbose {
                    let today_sessions: Vec<_> = sessions.into_iter().filter(|s| s.cost > 0.0).collect();
                    if !today_sessions.is_empty() {
                        println!("{}", output::format_verbose_sessions(&today_sessions));
                    }
                }
            }
        }
        Some(Command::Daily) => {
            let start = today - chrono::Duration::days(i64::from(cli.days) - 1);
            let (days, ..) = compute_summaries(cli, config, start, today)?;

            if cli.json {
                println!("{}", output::format_daily_json(&days));
            } else {
                println!("{}", output::format_daily_text(&days));
            }
        }
        Some(Command::Monthly) => {
            // Get data for the last 12 months
            let start = NaiveDate::from_ymd_opt(today.year() - 1, today.month(), 1)
                .unwrap_or(NaiveDate::from_ymd_opt(today.year() - 1, 1, 1).expect("valid date"));
            let (days, ..) = compute_summaries(cli, config, start, today)?;

            // Group by month
            let mut months: BTreeMap<String, (f64, HashSet<String>)> = BTreeMap::new();
            // We need session info per month, but we already have day summaries
            // Re-aggregate from day summaries
            for day in &days {
                let month_key = format!("{}-{:02}", day.date.year(), day.date.month());
                let entry = months.entry(month_key).or_insert_with(|| (0.0, HashSet::new()));
                entry.0 += day.cost;
                // We don't have per-day session IDs here, use session count as approximation
                for i in 0..day.sessions {
                    entry.1.insert(format!("{}_{}", day.date, i));
                }
            }

            let month_list: Vec<(String, f64, usize)> = months
                .into_iter()
                .rev()
                .map(|(month, (cost, sessions))| (month, cost, sessions.len()))
                .collect();

            if cli.json {
                println!("{}", output::format_monthly_json(&month_list));
            } else {
                println!("{}", output::format_monthly_text(&month_list));
            }
        }
        Some(Command::Session { id }) => {
            // For session command, scan all recent files (last 30 days)
            let start = today - chrono::Duration::days(30);
            let (_, sessions) = compute_summaries(cli, config, start, today)?;

            if id == "current" {
                // Show the most recent session (highest cost as heuristic)
                if let Some(session) = sessions
                    .iter()
                    .max_by(|a, b| a.cost.partial_cmp(&b.cost).unwrap_or(std::cmp::Ordering::Equal))
                {
                    println!(
                        "Session {}: ${:.2} ({} entries)",
                        &session.session_id[..8.min(session.session_id.len())],
                        session.cost,
                        session.entries
                    );
                } else {
                    println!("No sessions found");
                }
            } else {
                // Find session by ID prefix
                let matches: Vec<_> = sessions
                    .iter()
                    .filter(|s| s.session_id.starts_with(id.as_str()))
                    .collect();

                match matches.len() {
                    0 => println!("No session found matching '{}'", id),
                    1 => {
                        let s = &matches[0];
                        println!(
                            "Session {}: ${:.2} ({} entries)",
                            &s.session_id[..8.min(s.session_id.len())],
                            s.cost,
                            s.entries
                        );
                    }
                    _ => {
                        println!("Multiple sessions match '{}':", id);
                        for s in matches {
                            println!(
                                "  {} ${:.2} ({} entries)",
                                &s.session_id[..8.min(s.session_id.len())],
                                s.cost,
                                s.entries
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    setup_logging().context("Failed to setup logging")?;

    let cli = Cli::parse();
    let config = Config::load(cli.config.as_ref()).context("Failed to load configuration")?;

    info!("Starting with config from: {:?}", cli.config);

    run(&cli, &config).context("Application failed")?;

    Ok(())
}
