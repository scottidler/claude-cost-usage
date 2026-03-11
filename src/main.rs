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

mod average;
mod cache;
mod cli;
mod config;
mod graph;
mod output;
mod parser;
mod pricing;
mod scanner;
mod table;
mod update;

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
    verbose: bool,
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
        && !verbose
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
    let mut session_costs: BTreeMap<String, (f64, usize, chrono::DateTime<chrono::Utc>)> = BTreeMap::new();

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
        let model_pricing = match config.pricing.get(normalized) {
            Some(p) => p,
            None => {
                warn!("Unknown model: {} (normalized: {})", entry.model, normalized);
                continue;
            }
        };

        let cost = pricing::calculate_cost(model_pricing, &entry.usage);

        let day_entry = day_costs.entry(date).or_insert_with(|| (0.0, HashSet::new()));
        day_entry.0 += cost;
        day_entry.1.insert(entry.session_id.clone());

        let session_entry = session_costs
            .entry(entry.session_id.clone())
            .or_insert((0.0, 0, entry.timestamp));
        session_entry.0 += cost;
        session_entry.1 += 1;
        if entry.timestamp > session_entry.2 {
            session_entry.2 = entry.timestamp;
        }
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
        .map(|(session_id, (cost, entries, last_active))| SessionSummary {
            session_id,
            cost,
            entries,
            last_active,
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

fn subtract_months(date: NaiveDate, n: u32) -> NaiveDate {
    let total_months = date.year() * 12 + date.month() as i32 - 1 - n as i32;
    let target_year = total_months.div_euclid(12);
    let target_month = (total_months.rem_euclid(12) + 1) as u32;
    NaiveDate::from_ymd_opt(target_year, target_month, 1).expect("valid date")
}

fn run(cli: &Cli, config: &Config) -> Result<()> {
    let today = Local::now().date_naive();

    match &cli.command {
        None | Some(Command::Today { .. }) => {
            let (json, total, verbose) = match &cli.command {
                Some(Command::Today { json, total, verbose }) => (*json, *total, *verbose),
                _ => (false, false, false),
            };
            let (days, sessions) = compute_summaries(cli, config, today, today, verbose)?;
            let summary = days.first().cloned().unwrap_or(DaySummary {
                date: today,
                cost: 0.0,
                sessions: 0,
            });

            if total {
                println!("{:.2}", summary.cost);
            } else if json {
                println!("{}", output::format_today_json(&summary));
            } else {
                println!("{}", output::format_today_text(&summary));
                if verbose {
                    let today_sessions: Vec<_> = sessions.into_iter().filter(|s| s.cost > 0.0).collect();
                    if !today_sessions.is_empty() {
                        println!("{}", output::format_verbose_sessions(&today_sessions));
                    }
                }
            }
        }
        Some(Command::Yesterday { json, total, verbose }) => {
            let yesterday = today - chrono::Duration::days(1);
            let (days, sessions) = compute_summaries(cli, config, yesterday, yesterday, *verbose)?;
            let summary = days.first().cloned().unwrap_or(DaySummary {
                date: yesterday,
                cost: 0.0,
                sessions: 0,
            });

            if *total {
                println!("{:.2}", summary.cost);
            } else if *json {
                println!("{}", output::format_yesterday_json(&summary));
            } else {
                println!("{}", output::format_yesterday_text(&summary));
                if *verbose {
                    let yesterday_sessions: Vec<_> = sessions.into_iter().filter(|s| s.cost > 0.0).collect();
                    if !yesterday_sessions.is_empty() {
                        println!("{}", output::format_verbose_sessions(&yesterday_sessions));
                    }
                }
            }
        }
        Some(Command::Daily {
            json,
            total,
            days: num_days,
            average: show_avg,
            graph: show_graph,
        }) => {
            let start = today - chrono::Duration::days(i64::from(*num_days) - 1);
            let (days, ..) = compute_summaries(cli, config, start, today, false)?;

            if *total {
                let sum: f64 = days.iter().map(|d| d.cost).sum();
                println!("{:.2}", sum);
            } else {
                let avg = if *show_avg {
                    let sum: f64 = days.iter().map(|d| d.cost).sum();
                    let eff = average::effective_days(&days);
                    if eff >= 0.01 { Some((sum / eff, eff)) } else { Some((0.0, eff)) }
                } else {
                    None
                };

                if *json {
                    println!("{}", output::format_daily_json(&days, avg));
                } else {
                    if *show_graph {
                        println!("{}", graph::format_daily_text_with_bars(&days));
                    } else {
                        println!("{}", output::format_daily_text(&days));
                    }
                    if let Some((avg_cost, _)) = avg {
                        println!("{}", average::format_average_text("day", avg_cost));
                    }
                    if *show_graph {
                        println!("\n{}", graph::daily_sparkline(&days));
                        if let Some(chart) = graph::daily_chart(&days) {
                            println!("\n{}", chart);
                        }
                    }
                }
            }
        }
        Some(Command::Weekly {
            json,
            total,
            weeks: num_weeks,
            average: show_avg,
            graph: show_graph,
        }) => {
            // Monday of the current ISO week
            let current_monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
            // Go back (num_weeks - 1) more weeks
            let start = current_monday - chrono::Duration::weeks(i64::from(*num_weeks) - 1);
            let (days, ..) = compute_summaries(cli, config, start, today, false)?;

            // Group by ISO week
            let mut weeks: BTreeMap<String, (f64, HashSet<String>)> = BTreeMap::new();
            for day in &days {
                let week_key = format!("{}-W{:02}", day.date.iso_week().year(), day.date.iso_week().week());
                let entry = weeks.entry(week_key).or_insert_with(|| (0.0, HashSet::new()));
                entry.0 += day.cost;
                for i in 0..day.sessions {
                    entry.1.insert(format!("{}_{}", day.date, i));
                }
            }

            let week_list: Vec<(String, f64, usize)> = weeks
                .into_iter()
                .rev()
                .map(|(week, (cost, sessions))| (week, cost, sessions.len()))
                .collect();

            if *total {
                let sum: f64 = week_list.iter().map(|(_, cost, _)| cost).sum();
                println!("{:.2}", sum);
            } else {
                let avg = if *show_avg {
                    let sum: f64 = week_list.iter().map(|(_, cost, _)| cost).sum();
                    let eff = average::effective_weeks(&week_list);
                    if eff >= 0.01 { Some((sum / eff, eff)) } else { Some((0.0, eff)) }
                } else {
                    None
                };

                if *json {
                    println!("{}", output::format_weekly_json(&week_list, avg));
                } else {
                    if *show_graph {
                        println!("{}", graph::format_weekly_text_with_bars(&week_list));
                    } else {
                        println!("{}", output::format_weekly_text(&week_list));
                    }
                    if let Some((avg_cost, _)) = avg {
                        println!("{}", average::format_average_text("week", avg_cost));
                    }
                    if *show_graph {
                        println!("\n{}", graph::weekly_sparkline(&week_list));
                        if let Some(chart) = graph::weekly_chart(&week_list) {
                            println!("\n{}", chart);
                        }
                    }
                }
            }
        }
        Some(Command::Monthly {
            json,
            total,
            months: num_months,
            average: show_avg,
            graph: show_graph,
        }) => {
            let current_month_start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("valid date");
            let start = subtract_months(current_month_start, *num_months - 1);
            let (days, ..) = compute_summaries(cli, config, start, today, false)?;

            // Group by month
            let mut months: BTreeMap<String, (f64, HashSet<String>)> = BTreeMap::new();
            for day in &days {
                let month_key = format!("{}-{:02}", day.date.year(), day.date.month());
                let entry = months.entry(month_key).or_insert_with(|| (0.0, HashSet::new()));
                entry.0 += day.cost;
                for i in 0..day.sessions {
                    entry.1.insert(format!("{}_{}", day.date, i));
                }
            }

            let month_list: Vec<(String, f64, usize)> = months
                .into_iter()
                .rev()
                .map(|(month, (cost, sessions))| (month, cost, sessions.len()))
                .collect();

            if *total {
                let sum: f64 = month_list.iter().map(|(_, cost, _)| cost).sum();
                println!("{:.2}", sum);
            } else {
                let avg = if *show_avg {
                    let sum: f64 = month_list.iter().map(|(_, cost, _)| cost).sum();
                    let eff = average::effective_months(&month_list);
                    if eff >= 0.01 { Some((sum / eff, eff)) } else { Some((0.0, eff)) }
                } else {
                    None
                };

                if *json {
                    println!("{}", output::format_monthly_json(&month_list, avg));
                } else {
                    if *show_graph {
                        println!("{}", graph::format_monthly_text_with_bars(&month_list));
                    } else {
                        println!("{}", output::format_monthly_text(&month_list));
                    }
                    if let Some((avg_cost, _)) = avg {
                        println!("{}", average::format_average_text("month", avg_cost));
                    }
                    if *show_graph {
                        println!("\n{}", graph::monthly_sparkline(&month_list));
                        if let Some(chart) = graph::monthly_chart(&month_list) {
                            println!("\n{}", chart);
                        }
                    }
                }
            }
        }
        Some(Command::Pricing { .. }) => {
            // Handled in main() before run() is called
            unreachable!("Pricing command should be handled before run()")
        }
        Some(Command::Session { id }) => {
            // For session command, scan all recent files (last 30 days)
            let start = today - chrono::Duration::days(30);
            let (_, sessions) = compute_summaries(cli, config, start, today, false)?;

            if id == "current" {
                // Show the most recently active session
                if let Some(session) = sessions.iter().max_by_key(|s| s.last_active) {
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

    // Phase 1: handle commands that don't need config
    if let Some(Command::Pricing { update, from, .. }) = &cli.command
        && (*update || from.is_some())
    {
        return update::run(from.as_ref());
    }

    // Phase 2: all other commands require config
    let config = Config::load(cli.config.as_ref()).context("Failed to load configuration")?;

    info!("Starting with config from: {:?}", cli.config);

    // Pricing --show needs config, handled here
    if let Some(Command::Pricing { .. }) = &cli.command {
        return update::show(&config);
    }

    run(&cli, &config).context("Application failed")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subtract_months_same_year() {
        let date = NaiveDate::from_ymd_opt(2026, 6, 1).expect("valid date");
        let result = subtract_months(date, 3);
        assert_eq!(result, NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date"));
    }

    #[test]
    fn test_subtract_months_cross_year() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date");
        let result = subtract_months(date, 5);
        assert_eq!(result, NaiveDate::from_ymd_opt(2025, 10, 1).expect("valid date"));
    }

    #[test]
    fn test_subtract_months_january_edge() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid date");
        let result = subtract_months(date, 1);
        assert_eq!(result, NaiveDate::from_ymd_opt(2025, 12, 1).expect("valid date"));
    }

    #[test]
    fn test_subtract_months_zero() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date");
        let result = subtract_months(date, 0);
        assert_eq!(result, date);
    }

    #[test]
    fn test_subtract_months_twelve() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date");
        let result = subtract_months(date, 12);
        assert_eq!(result, NaiveDate::from_ymd_opt(2025, 3, 1).expect("valid date"));
    }
}
