use textplots::{Chart, LabelBuilder, LabelFormat, Plot, Shape, TickDisplay, TickDisplayBuilder};

use crate::output::DaySummary;

// Left-fractional blocks: index 0 = empty, 1 = 1/8th, ..., 8 = full block
const BLOCKS: [char; 9] = [
    ' ',        // 0/8
    '\u{258F}', // 1/8  Left One Eighth Block
    '\u{258E}', // 2/8  Left One Quarter Block
    '\u{258D}', // 3/8  Left Three Eighths Block
    '\u{258C}', // 4/8  Left Half Block
    '\u{258B}', // 5/8  Left Five Eighths Block
    '\u{258A}', // 6/8  Left Three Quarters Block
    '\u{2589}', // 7/8  Left Seven Eighths Block
    '\u{2588}', // 8/8  Full Block
];

/// Render an inline Unicode horizontal bar
pub fn bar(value: f64, max_value: f64, max_width: usize) -> String {
    if max_value <= 0.0 || value <= 0.0 {
        return String::new();
    }
    let ratio = (value / max_value).min(1.0);
    let total_eighths = (ratio * max_width as f64 * 8.0) as usize;
    let full_blocks = total_eighths / 8;
    let remainder = total_eighths % 8;
    let mut out = String::new();
    for _ in 0..full_blocks {
        out.push(BLOCKS[8]);
    }
    if remainder > 0 {
        out.push(BLOCKS[remainder]);
    }
    out
}

/// Format daily text output with inline bars
pub fn format_daily_text_with_bars(days: &[DaySummary]) -> String {
    let max_cost = days.iter().map(|d| d.cost).fold(0.0_f64, f64::max);
    let mut out = String::new();
    for day in days {
        let bar_str = bar(day.cost, max_cost, 20);
        out.push_str(&format!(
            "{}  ${:>7.2}  ({} session{})  {}\n",
            day.date,
            day.cost,
            day.sessions,
            if day.sessions == 1 { "" } else { "s" },
            bar_str
        ));
    }
    out.trim_end().to_string()
}

/// Format weekly text output with inline bars
pub fn format_weekly_text_with_bars(weeks: &[(String, f64, usize)]) -> String {
    let max_cost = weeks.iter().map(|(_, c, _)| *c).fold(0.0_f64, f64::max);
    let mut out = String::new();
    for (week, cost, sessions) in weeks {
        let bar_str = bar(*cost, max_cost, 20);
        out.push_str(&format!(
            "{}  ${:>7.2}  ({} session{})  {}\n",
            week,
            cost,
            sessions,
            if *sessions == 1 { "" } else { "s" },
            bar_str
        ));
    }
    out.trim_end().to_string()
}

/// Format monthly text output with inline bars
pub fn format_monthly_text_with_bars(months: &[(String, f64, usize)]) -> String {
    let max_cost = months.iter().map(|(_, c, _)| *c).fold(0.0_f64, f64::max);
    let mut out = String::new();
    for (month, cost, sessions) in months {
        let bar_str = bar(*cost, max_cost, 20);
        out.push_str(&format!(
            "{}  ${:>7.2}  ({} session{})  {}\n",
            month,
            cost,
            sessions,
            if *sessions == 1 { "" } else { "s" },
            bar_str
        ));
    }
    out.trim_end().to_string()
}

/// Print a braille line chart from cost data points to stdout.
/// Points are displayed in chronological order (reversed from the input which is newest-first).
pub fn print_chart(costs: &[f64], avg: Option<f64>) {
    if costs.is_empty() {
        return;
    }

    // Reverse to chronological order (input is newest-first)
    let chronological: Vec<f64> = costs.iter().rev().copied().collect();
    let points: Vec<(f32, f32)> = chronological
        .iter()
        .enumerate()
        .map(|(i, c)| (i as f32, *c as f32))
        .collect();

    let xmax = (chronological.len() - 1).max(1) as f32;
    let line_shape = Shape::Lines(&points);

    match avg {
        Some(avg_val) => {
            let avg_shape = Shape::Continuous(Box::new(move |_| avg_val as f32));
            Chart::new(120, 40, 0.0, xmax)
                .y_tick_display(TickDisplay::Dense)
                .y_label_format(LabelFormat::Custom(Box::new(|v| format!("${:.0}", v))))
                .lineplot(&line_shape)
                .lineplot(&avg_shape)
                .display();
        }
        None => {
            Chart::new(120, 40, 0.0, xmax)
                .y_tick_display(TickDisplay::Dense)
                .y_label_format(LabelFormat::Custom(Box::new(|v| format!("${:.0}", v))))
                .lineplot(&line_shape)
                .display();
        }
    }
}

/// Print a braille line chart for daily data to stdout
pub fn daily_chart(days: &[DaySummary], avg: Option<f64>) {
    let costs: Vec<f64> = days.iter().map(|d| d.cost).collect();
    print_chart(&costs, avg);
}

/// Print a braille line chart for weekly data to stdout
pub fn weekly_chart(weeks: &[(String, f64, usize)], avg: Option<f64>) {
    let costs: Vec<f64> = weeks.iter().map(|(_, c, _)| *c).collect();
    print_chart(&costs, avg);
}

/// Print a braille line chart for monthly data to stdout
pub fn monthly_chart(months: &[(String, f64, usize)], avg: Option<f64>) {
    let costs: Vec<f64> = months.iter().map(|(_, c, _)| *c).collect();
    print_chart(&costs, avg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_bar_zero_value() {
        assert_eq!(bar(0.0, 100.0, 20), "");
    }

    #[test]
    fn test_bar_zero_max() {
        assert_eq!(bar(50.0, 0.0, 20), "");
    }

    #[test]
    fn test_bar_max_value() {
        let result = bar(100.0, 100.0, 20);
        // Should be 20 full blocks
        assert_eq!(result.chars().count(), 20);
        assert!(result.chars().all(|c| c == '\u{2588}'));
    }

    #[test]
    fn test_bar_half_value() {
        let result = bar(50.0, 100.0, 20);
        // Should be 10 full blocks
        assert_eq!(result.chars().count(), 10);
    }

    #[test]
    fn test_bar_fractional() {
        let result = bar(25.0, 100.0, 20);
        // 25% of 20 = 5 full blocks
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn test_format_daily_text_with_bars() {
        let days = vec![
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
                cost: 20.0,
                sessions: 3,
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 9).expect("valid date"),
                cost: 10.0,
                sessions: 1,
            },
        ];
        let text = format_daily_text_with_bars(&days);
        assert!(text.contains("2026-03-10"));
        assert!(text.contains("2026-03-09"));
        // The max value row should have full bars
        assert!(text.contains('\u{2588}'));
    }

    #[test]
    fn test_daily_chart_no_panic() {
        let days = vec![
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
                cost: 20.0,
                sessions: 3,
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 9).expect("valid date"),
                cost: 10.0,
                sessions: 1,
            },
        ];
        daily_chart(&days, None);
    }

    #[test]
    fn test_daily_chart_with_avg_no_panic() {
        let days = vec![
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
                cost: 20.0,
                sessions: 3,
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 9).expect("valid date"),
                cost: 10.0,
                sessions: 1,
            },
        ];
        daily_chart(&days, Some(15.0));
    }

    #[test]
    fn test_weekly_chart_no_panic() {
        let weeks = vec![
            ("2026-W11".to_string(), 47.82, 12),
            ("2026-W10".to_string(), 123.45, 28),
        ];
        weekly_chart(&weeks, None);
    }

    #[test]
    fn test_monthly_chart_no_panic() {
        let months = vec![("2026-03".to_string(), 200.0, 30), ("2026-02".to_string(), 150.0, 25)];
        monthly_chart(&months, None);
    }

    #[test]
    fn test_chart_empty_data() {
        print_chart(&[], None);
    }

    #[test]
    fn test_chart_single_point() {
        print_chart(&[10.0], None);
    }
}
