use color_eyre::Result;
use gflow::core::job::{Job, JobState};
use ratatui::widgets::{Block, Borders};
use ratatui::{
    border,
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{palette::tailwind, Color, Stylize},
    symbols::{self},
    text::Line,
    widgets::{Tabs, Widget},
    DefaultTerminal,
};
use std::time::Duration;
use strum::{Display, EnumIter, FromRepr, IntoEnumIterator};
use tokio::sync::mpsc;
use tokio::time::interval;

use gflow::client::Client;

struct App {
    state: AppState,
    selected_tab: SelectedTab,
    client: Client,
    jobs: Vec<Job>,
}

impl App {
    fn new() -> Self {
        use clap::Parser;
        let args = crate::cli::GQueue::parse();
        let config = gflow::config::load_config(args.config.as_ref()).unwrap();
        let client = gflow::client::Client::build(&config).expect("Failed to build client");
        Self {
            state: AppState::default(),
            selected_tab: SelectedTab::default(),
            client,
            jobs: vec![],
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum AppState {
    #[default]
    Running,
    Quitting,
}

#[derive(Default, Clone, Copy, Display, FromRepr, EnumIter, PartialEq, Eq)]
enum SelectedTab {
    #[default]
    #[strum(to_string = "Queued")]
    Queued,
    #[strum(to_string = "Running")]
    Running,
    #[strum(to_string = "Finished")]
    Finished,
    #[strum(to_string = "Failed")]
    Failed,
}

impl SelectedTab {
    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::Queued => tailwind::YELLOW,
            Self::Running => tailwind::BLUE,
            Self::Finished => tailwind::GREEN,
            Self::Failed => tailwind::RED,
        }
    }
}

impl From<JobState> for SelectedTab {
    fn from(state: JobState) -> Self {
        match state {
            JobState::Queued => Self::Queued,
            JobState::Running => Self::Running,
            JobState::Finished => Self::Finished,
            JobState::Failed => Self::Failed,
        }
    }
}

pub fn show_tui() -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app = App::new().run(terminal);
    ratatui::restore();
    app
}

enum TuiEvent {
    Key(event::KeyEvent),
    Tick,
    Jobs(Vec<Job>),
}

impl App {
    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(32);
        let tick_tx = tx.clone();
        let jobs_tx = tx.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                interval.tick().await;
                if let Ok(Event::Key(key)) = event::read() {
                    if tx.send(TuiEvent::Key(key)).await.is_err() {
                        break;
                    }
                }
            }
        });
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(250));
            loop {
                interval.tick().await;
                if tick_tx.send(TuiEvent::Tick).await.is_err() {
                    break;
                }
            }
        });
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(2));
            loop {
                interval.tick().await;
                if let Ok(jobs) = client.list_jobs().await {
                    if jobs_tx
                        .send(TuiEvent::Jobs(
                            jobs.json::<Vec<Job>>().await.unwrap_or_default(),
                        ))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        while self.state == AppState::Running {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;

            if let Ok(event) = rx.try_recv() {
                match event {
                    TuiEvent::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Char('l') | KeyCode::Right => self.next_tab(),
                                KeyCode::Char('h') | KeyCode::Left => self.previous_tab(),
                                KeyCode::Char('q') | KeyCode::Esc => self.quit(),
                                _ => {}
                            }
                        }
                    }
                    TuiEvent::Tick => {
                        // Handle UI updates if needed
                    }
                    TuiEvent::Jobs(jobs) => {
                        self.jobs = jobs;
                    }
                }
            }
        }

        Ok(())
    }

    fn next_tab(&mut self) {
        let current = self.selected_tab as usize;
        self.selected_tab = SelectedTab::from_repr(current.saturating_add(1)).unwrap_or_default();
    }

    fn previous_tab(&mut self) {
        let current = self.selected_tab as usize;
        self.selected_tab = SelectedTab::from_repr(current.saturating_sub(1)).unwrap_or_default();
    }

    pub fn quit(&mut self) {
        self.state = AppState::Quitting;
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(3), Min(0), Length(1)]);
        let [header_area, content_area, footer_area] = vertical.areas(area);

        self.render_tabs(header_area, buf);
        self.render_content(content_area, buf);
        self.render_footer(footer_area, buf);
    }
}

impl App {
    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let titles: Vec<String> = SelectedTab::iter()
            .map(|tab| {
                let count = self
                    .jobs
                    .iter()
                    .filter(|job| SelectedTab::from(job.state.clone()) == tab)
                    .count();
                format!("{tab} ({count})")
            })
            .collect();
        let highlight_style = self.selected_tab.palette().c700;
        Tabs::new(titles)
            .block(
                Block::default()
                    .borders(border!(ALL))
                    .border_type(ratatui::widgets::BorderType::Rounded),
            )
            .select(self.selected_tab as usize)
            .highlight_style(highlight_style)
            .divider(symbols::line::VERTICAL)
            .render(area, buf);
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(border!(ALL))
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(self.selected_tab.to_string());

        let inner_area = block.inner(area);
        block.render(area, buf);

        let constraints = vec![
            Constraint::Length(5),  // ID
            Constraint::Length(1),  // Separator
            Constraint::Length(20), // Name
            Constraint::Length(1),  // Separator
            Constraint::Length(30), // Command
            Constraint::Length(1),  // Separator
            Constraint::Length(5),  // GPUs
            Constraint::Length(1),  // Separator
            Constraint::Min(10),    // Status
        ];

        let horizontal = Layout::horizontal(constraints);
        let columns: [Rect; 9] = horizontal.areas(inner_area);
        // Render header
        let headers = [
            ("ID", columns[0]),
            ("│", columns[1]),
            ("RunName", columns[2]),
            ("│", columns[3]),
            ("Command", columns[4]),
            ("│", columns[5]),
            ("GPUs", columns[6]),
            ("│", columns[7]),
            ("Status", columns[8]),
        ];

        for (text, area) in headers {
            let style = if text == "│" {
                tailwind::ZINC.c500
            } else {
                Color::default()
            };
            Line::from(text).style(style).render(area, buf);
        }

        let separator_y = inner_area.y + 1;
        let separator = "─".repeat(inner_area.width as usize);
        Line::from(separator).dark_gray().render(
            Rect::new(inner_area.x, separator_y, inner_area.width, 1),
            buf,
        );
        let filtered_jobs: Vec<&Job> = self
            .jobs
            .iter()
            .filter(|job| SelectedTab::from(job.state.clone()) == self.selected_tab)
            .collect();

        if filtered_jobs.is_empty() {
            let empty_msg = "No jobs in this state";
            Line::from(empty_msg).dark_gray().centered().render(
                Rect::new(inner_area.x, separator_y + 1, inner_area.width, 1),
                buf,
            );
            return;
        }
        for (idx, job) in filtered_jobs.iter().enumerate() {
            let y = separator_y + 1 + idx as u16;
            if y >= inner_area.bottom() {
                break;
            }

            let row_areas: [Rect; 9] =
                horizontal.areas(Rect::new(inner_area.x, y, inner_area.width, 1));

            let gpu_count = job.gpu_ids.as_ref().map_or(0, |ids| ids.len());
            let status_style = SelectedTab::from(job.state.clone()).palette().c700;

            // Render each column of the row
            let id_str = job.id.to_string();
            let gpu_count_str = format!("{gpu_count:>3}");
            let state_str = job.state.to_string();
            let columns: [(&str, Color); 9] = [
                (&id_str, tailwind::ZINC.c100),
                ("│", tailwind::ZINC.c500),
                (job.run_name.as_deref().unwrap_or("-"), tailwind::ZINC.c100),
                ("│", tailwind::ZINC.c500),
                (job.command.as_deref().unwrap_or("-"), tailwind::ZINC.c400),
                ("│", tailwind::ZINC.c500),
                (&gpu_count_str, tailwind::YELLOW.c500),
                ("│", tailwind::ZINC.c500),
                (&state_str, status_style),
            ];

            for ((text, style), area) in columns.iter().zip(row_areas.iter()) {
                Line::from(*text).style(*style).render(*area, buf);
            }
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        Line::from(vec![
            "←/→".yellow(),
            " Change Tab │ ".dark_gray(),
            "q".yellow(),
            " Quit".dark_gray(),
        ])
        .centered()
        .render(area, buf);
    }
}
