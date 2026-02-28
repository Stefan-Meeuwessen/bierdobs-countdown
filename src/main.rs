use std::error::Error;
use std::io;
use std::io::Write;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Utc, Weekday};
use chrono_tz::{Europe::Amsterdam, Tz};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};


type AppResult<T> = Result<T, Box<dyn Error>>;


fn setup_terminal() -> AppResult<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> AppResult<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> AppResult<()> {
    let mut next_bier_time = get_next_friday_1600(Utc::now().with_timezone(&Amsterdam));

    loop {
        let now = Utc::now().with_timezone(&Amsterdam);

        if now >= next_bier_time {
            play_buzzer_for(Duration::from_secs(3))?;
            next_bier_time = get_next_friday_1600(Utc::now().with_timezone(&Amsterdam));
        }

        let now = Utc::now().with_timezone(&Amsterdam);
        let remaining = next_bier_time.signed_duration_since(now);

        terminal.draw(|frame| draw_ui(frame, now, next_bier_time, remaining))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.kind == KeyEventKind::Press
                    && matches!(key_event.code, KeyCode::Char('q') | KeyCode::Esc)
                {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn play_buzzer_for(duration: Duration) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        if play_windows_beep(duration) {
            return Ok(());
        }
    }

    play_terminal_buzzer(duration)?;
    Ok(())
}

fn play_terminal_buzzer(duration: Duration) -> AppResult<()> {
    let ring_interval = Duration::from_millis(180);
    let stop_at = Instant::now() + duration;
    let mut stdout = io::stdout();

    while Instant::now() < stop_at {
        print!("\x07");
        stdout.flush()?;
        std::thread::sleep(ring_interval);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn play_windows_beep(duration: Duration) -> bool {
    let stop_at = Instant::now() + duration;
    let mut high_tone = false;
    let mut beep_succeeded = false;

    while Instant::now() < stop_at {
        let freq = if high_tone { 1_400 } else { 900 };
        let remaining = stop_at.saturating_duration_since(Instant::now());
        let beep_ms = remaining.as_millis().min(180) as u32;
        if beep_ms == 0 {
            break;
        }

        // SAFETY: Win32 Beep is called with bounded frequency/duration values.
        let ok = unsafe { Beep(freq, beep_ms) != 0 };
        beep_succeeded |= ok;

        if !ok {
            std::thread::sleep(Duration::from_millis(beep_ms as u64));
        }

        high_tone = !high_tone;
    }

    beep_succeeded
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn Beep(dw_freq: u32, dw_duration: u32) -> i32;
}

fn draw_ui(frame: &mut Frame, now: DateTime<Tz>, next_bier_time: DateTime<Tz>, remaining: ChronoDuration) {
    let area = frame.area();
    let block = Block::default().title(" BierDobs ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled("Nu", Style::default().add_modifier(Modifier::BOLD))),
        Line::from(format_dutch_datetime(now)),
        Line::from(""),
        Line::from(Span::styled(
            "Volgende Bier-tijd",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(format_dutch_datetime(next_bier_time)),
        Line::from(""),
        Line::from(Span::styled(
            format!("🍺 Nog {} te gaan!", format_countdown(remaining)),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Druk op q of Esc om af te sluiten."),
    ];
    let content_height = lines.len() as u16;
    let text = Text::from(lines);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(content_height),
            Constraint::Min(0),
        ])
        .split(inner);

    let paragraph = Paragraph::new(text).alignment(Alignment::Center);
    frame.render_widget(paragraph, vertical[1]);
}

fn get_next_friday_1600(now: DateTime<Tz>) -> DateTime<Tz> {
    let friday_index = Weekday::Fri.num_days_from_monday() as i64;
    let current_index = now.weekday().num_days_from_monday() as i64;
    let mut days_until = (friday_index - current_index + 7) % 7;

    let mut candidate_date = now.date_naive() + ChronoDuration::days(days_until);
    let mut candidate = amsterdam_at_1600(candidate_date);

    if candidate <= now {
        days_until += 7;
        candidate_date = now.date_naive() + ChronoDuration::days(days_until);
        candidate = amsterdam_at_1600(candidate_date);
    }

    candidate
}

fn amsterdam_at_1600(date: chrono::NaiveDate) -> DateTime<Tz> {
    let naive_datetime = date
        .and_hms_opt(16, 0, 0)
        .expect("16:00:00 is always a valid local time");

    Amsterdam
        .from_local_datetime(&naive_datetime)
        .single()
        .expect("16:00 should map to a single instant in Amsterdam")
}

fn format_dutch_datetime(dt: DateTime<Tz>) -> String {
    format!(
        "{} {:02} {} - {}",
        dutch_weekday_name(dt.weekday()),
        dt.day(),
        dutch_month_name(dt.month()),
        dt.format("%H:%M:%S")
    )
}

fn format_countdown(duration: ChronoDuration) -> String {
    let total_seconds = duration.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn dutch_weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Maandag",
        Weekday::Tue => "Dinsdag",
        Weekday::Wed => "Woensdag",
        Weekday::Thu => "Donderdag",
        Weekday::Fri => "Vrijdag",
        Weekday::Sat => "Zaterdag",
        Weekday::Sun => "Zondag",
    }
}

fn dutch_month_name(month: u32) -> &'static str {
    match month {
        1 => "Januari",
        2 => "Februari",
        3 => "Maart",
        4 => "April",
        5 => "Mei",
        6 => "Juni",
        7 => "Juli",
        8 => "Augustus",
        9 => "September",
        10 => "Oktober",
        11 => "November",
        12 => "December",
        _ => "Onbekend",
    }
}


fn main() -> AppResult<()> {
    let mut terminal = setup_terminal()?;
    let run_result = run_app(&mut terminal);
    restore_terminal(&mut terminal)?;
    run_result
}