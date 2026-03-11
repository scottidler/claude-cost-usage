use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct DaySummary {
    pub date: NaiveDate,
    pub cost: f64,
    pub sessions: usize,
}

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub cost: f64,
    pub entries: usize,
    pub last_active: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct TodayJson {
    pub today: f64,
    pub sessions: usize,
}

#[derive(Serialize)]
pub struct DailyJson {
    pub days: Vec<DayEntryJson>,
}

#[derive(Serialize)]
pub struct DayEntryJson {
    pub date: String,
    pub cost: f64,
    pub sessions: usize,
}

#[derive(Serialize)]
pub struct MonthlyJson {
    pub months: Vec<MonthEntryJson>,
}

#[derive(Serialize)]
pub struct MonthEntryJson {
    pub month: String,
    pub cost: f64,
    pub sessions: usize,
}

pub fn format_today_text(summary: &DaySummary) -> String {
    format!(
        "Today: ${:.2} ({} session{})",
        summary.cost,
        summary.sessions,
        if summary.sessions == 1 { "" } else { "s" }
    )
}

pub fn format_today_json(summary: &DaySummary) -> String {
    let json = TodayJson {
        today: round_cents(summary.cost),
        sessions: summary.sessions,
    };
    serde_json::to_string(&json).unwrap_or_default()
}

pub fn format_daily_text(days: &[DaySummary]) -> String {
    let mut out = String::new();
    for day in days {
        out.push_str(&format!(
            "{}  ${:>7.2}  ({} session{})\n",
            day.date,
            day.cost,
            day.sessions,
            if day.sessions == 1 { "" } else { "s" }
        ));
    }
    out.trim_end().to_string()
}

pub fn format_daily_json(days: &[DaySummary]) -> String {
    let json = DailyJson {
        days: days
            .iter()
            .map(|d| DayEntryJson {
                date: d.date.to_string(),
                cost: round_cents(d.cost),
                sessions: d.sessions,
            })
            .collect(),
    };
    serde_json::to_string(&json).unwrap_or_default()
}

pub fn format_monthly_text(months: &[(String, f64, usize)]) -> String {
    let mut out = String::new();
    for (month, cost, sessions) in months {
        out.push_str(&format!(
            "{}  ${:>7.2}  ({} session{})\n",
            month,
            cost,
            sessions,
            if *sessions == 1 { "" } else { "s" }
        ));
    }
    out.trim_end().to_string()
}

pub fn format_monthly_json(months: &[(String, f64, usize)]) -> String {
    let json = MonthlyJson {
        months: months
            .iter()
            .map(|(month, cost, sessions)| MonthEntryJson {
                month: month.clone(),
                cost: round_cents(*cost),
                sessions: *sessions,
            })
            .collect(),
    };
    serde_json::to_string(&json).unwrap_or_default()
}

pub fn format_verbose_sessions(sessions: &[SessionSummary]) -> String {
    let mut out = String::new();
    for s in sessions {
        out.push_str(&format!(
            "  {}  ${:.2} ({} entries)\n",
            &s.session_id[..8.min(s.session_id.len())],
            s.cost,
            s.entries
        ));
    }
    out.trim_end().to_string()
}

fn round_cents(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_today_text() {
        let summary = DaySummary {
            date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
            cost: 14.234,
            sessions: 3,
        };
        assert_eq!(format_today_text(&summary), "Today: $14.23 (3 sessions)");
    }

    #[test]
    fn test_format_today_text_singular() {
        let summary = DaySummary {
            date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
            cost: 7.40,
            sessions: 1,
        };
        assert_eq!(format_today_text(&summary), "Today: $7.40 (1 session)");
    }

    #[test]
    fn test_format_today_json() {
        let summary = DaySummary {
            date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
            cost: 14.236,
            sessions: 3,
        };
        let json = format_today_json(&summary);
        assert!(json.contains("\"today\":14.24"));
        assert!(json.contains("\"sessions\":3"));
    }

    #[test]
    fn test_format_daily_text() {
        let days = vec![
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 10).expect("valid date"),
                cost: 14.23,
                sessions: 3,
            },
            DaySummary {
                date: NaiveDate::from_ymd_opt(2026, 3, 9).expect("valid date"),
                cost: 22.17,
                sessions: 5,
            },
        ];
        let text = format_daily_text(&days);
        assert!(text.contains("2026-03-10"));
        assert!(text.contains("14.23"));
        assert!(text.contains("2026-03-09"));
    }

    #[test]
    fn test_round_cents() {
        assert!((round_cents(14.236) - 14.24).abs() < f64::EPSILON);
        assert!((round_cents(14.234) - 14.23).abs() < f64::EPSILON);
    }
}
