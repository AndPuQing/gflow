use color_eyre::Result;
use gflow::job::{Job, JobState};
use ratatui::{
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::{Constraint, Layout, Rect},
    style::{palette::tailwind, Color, Stylize},
    symbols,
    text::Line,
    widgets::{Block, Padding, Paragraph, Tabs, Widget},
    DefaultTerminal,
};
use strum::{Display, EnumIter, FromRepr, IntoEnumIterator};

#[derive(Default)]
struct App {
    state: AppState,
    selected_tab: SelectedTab,
    jobs: Vec<Job>,
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

pub fn show_tui(jobs: &[Job]) -> Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let app = App {
        jobs: jobs.to_owned(),
        ..Default::default()
    }
    .run(terminal);
    ratatui::restore();
    app
}

impl App {
    fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while self.state == AppState::Running {
            terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn handle_events(&mut self) -> std::io::Result<()> {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('l') | KeyCode::Right => self.next_tab(),
                    KeyCode::Char('h') | KeyCode::Left => self.previous_tab(),
                    KeyCode::Char('q') | KeyCode::Esc => self.quit(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub fn next_tab(&mut self) {
        self.selected_tab = self.selected_tab.next();
    }

    pub fn previous_tab(&mut self) {
        self.selected_tab = self.selected_tab.previous();
    }

    pub fn quit(&mut self) {
        self.state = AppState::Quitting;
    }
}

impl SelectedTab {
    /// Get the previous tab, if there is no previous tab return the current tab.
    fn previous(self) -> Self {
        let current_index: usize = self as usize;
        let previous_index = current_index.saturating_sub(1);
        Self::from_repr(previous_index).unwrap_or(self)
    }

    /// Get the next tab, if there is no next tab return the current tab.
    fn next(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1);
        Self::from_repr(next_index).unwrap_or(self)
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(1), Min(0), Length(1)]);
        let [header_area, inner_area, footer_area] = vertical.areas(area);

        let horizontal = Layout::horizontal([Min(0), Length(20)]);
        let [tabs_area, title_area] = horizontal.areas(header_area);

        render_title(title_area, buf);
        self.render_tabs(tabs_area, buf);
        let selected_jobs = self
            .jobs
            .iter()
            .filter(|job| SelectedTab::from(job.state.clone()) == self.selected_tab);
        self.selected_tab
            .render_with_jobs(inner_area, buf, &selected_jobs.collect::<Vec<_>>());
        render_footer(footer_area, buf);
    }
}

impl App {
    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let titles = SelectedTab::iter().map(SelectedTab::title);
        let highlight_style = (Color::default(), self.selected_tab.palette().c400);
        let selected_tab_index = self.selected_tab as usize;
        Tabs::new(titles)
            .highlight_style(highlight_style)
            .select(selected_tab_index)
            .padding("", "")
            .divider(" ")
            .render(area, buf);
    }
}

fn render_title(area: Rect, buf: &mut Buffer) {
    "Gflow Job Status".bold().render(area, buf);
}

fn render_footer(area: Rect, buf: &mut Buffer) {
    Line::raw("◄ ► to change tab | Press q to quit")
        .centered()
        .render(area, buf);
}

// Add this implementation for SelectedTab
impl SelectedTab {
    fn render_with_jobs(&self, area: Rect, buf: &mut Buffer, jobs: &[&Job]) {
        let content = jobs
            .iter()
            .map(|job| {
                format!(
                    "Name: {} | Command: {}",
                    job.run_name.as_ref().unwrap(),
                    job.command.as_ref().unwrap()
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        let paragraph = if jobs.is_empty() {
            Paragraph::new("No jobs in this state")
        } else {
            Paragraph::new(content)
        };

        paragraph.block(self.block()).render(area, buf);
    }
}

impl SelectedTab {
    /// Return tab's name as a styled `Line`
    fn title(self) -> Line<'static> {
        format!("  {self}  ")
            .fg(tailwind::SLATE.c600)
            // .bg(self.palette().c900)
            .into()
    }

    /// A block surrounding the tab's content
    fn block(self) -> Block<'static> {
        Block::bordered()
            .border_set(symbols::border::PROPORTIONAL_TALL)
            .padding(Padding::horizontal(1))
            .border_style(self.palette().c400)
    }

    const fn palette(self) -> tailwind::Palette {
        match self {
            Self::Queued => tailwind::BLUE,
            Self::Running => tailwind::EMERALD,
            Self::Finished => tailwind::INDIGO,
            Self::Failed => tailwind::RED,
        }
    }
}
