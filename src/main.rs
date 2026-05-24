use std::{
    io::{self, IsTerminal, Stdout},
    time::Duration,
};

use anyhow::{bail, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};

fn main() -> Result<()> {
    let mut terminal = init_terminal()?;
    let app_result = App::default().run(&mut terminal);
    restore_terminal(&mut terminal)?;
    app_result
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("ForgeTUI must be run from an interactive terminal. Try: cd /home/mrmoe28/coding-tui && cargo run --bin forge");
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Workspace,
    Tasks,
    Agents,
    Output,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Workspace => Self::Tasks,
            Self::Tasks => Self::Agents,
            Self::Agents => Self::Output,
            Self::Output => Self::Workspace,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Workspace => "Workspace",
            Self::Tasks => "Tasks",
            Self::Agents => "Agents",
            Self::Output => "Output",
        }
    }
}

struct App {
    focus: Focus,
    selected_task: usize,
    selected_agent: usize,
    tasks: Vec<Task>,
    agents: Vec<Agent>,
    output: Vec<String>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            focus: Focus::Workspace,
            selected_task: 0,
            selected_agent: 0,
            tasks: vec![
                Task::new("Build TUI shell", "ready", "cargo run"),
                Task::new("Run tests", "queued", "cargo test"),
                Task::new("Watch project", "queued", "watch -n 2 cargo check"),
            ],
            agents: vec![
                Agent::new("planner", "idle", "Break down implementation work"),
                Agent::new("worker-1", "idle", "Implement scoped code changes"),
                Agent::new("reviewer", "idle", "Review diffs before merge"),
            ],
            output: vec![
                "ForgeTUI initialized.".to_string(),
                "Press Tab to move focus, a to spawn a placeholder agent, q to quit.".to_string(),
            ],
        }
    }
}

impl App {
    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    if self.handle_key(key) {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key {
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('q'),
                ..
            } => return true,
            KeyEvent {
                code: KeyCode::Tab, ..
            } => self.focus = self.focus.next(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.move_selection(1),
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.move_selection(-1),
            KeyEvent {
                code: KeyCode::Char('a'),
                ..
            } => self.spawn_placeholder_agent(),
            KeyEvent {
                code: KeyCode::Char('r'),
                ..
            } => self.run_selected_task(),
            _ => {}
        }

        false
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Tasks => {
                self.selected_task = bounded_index(self.selected_task, self.tasks.len(), delta);
            }
            Focus::Agents => {
                self.selected_agent = bounded_index(self.selected_agent, self.agents.len(), delta);
            }
            _ => {}
        }
    }

    fn spawn_placeholder_agent(&mut self) {
        let name = format!("agent-{}", self.agents.len() + 1);
        self.agents
            .push(Agent::new(&name, "spawned", "Awaiting task assignment"));
        self.selected_agent = self.agents.len() - 1;
        self.focus = Focus::Agents;
        self.output
            .push(format!("Spawned placeholder subagent `{name}`."));
    }

    fn run_selected_task(&mut self) {
        if let Some(task) = self.tasks.get_mut(self.selected_task) {
            task.status = "started".to_string();
            self.output
                .push(format!("Task `{}` would run: {}", task.name, task.command));
        }
    }

    fn render(&self, frame: &mut Frame) {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.render_header(frame, root[0]);
        self.render_body(frame, root[1]);
        self.render_footer(frame, root[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            Span::styled(
                "ForgeTUI",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  unified coding workspace"),
        ]);
        frame.render_widget(
            panel("Status").title_bottom(Line::from(self.focus.title())),
            area,
        );
        frame.render_widget(Paragraph::new(title), inner(area));
    }

    fn render_body(&self, frame: &mut Frame, area: Rect) {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(28),
                Constraint::Percentage(34),
                Constraint::Percentage(38),
            ])
            .split(area);

        self.render_workspace(frame, columns[0]);

        let middle = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(columns[1]);
        self.render_tasks(frame, middle[0]);
        self.render_agents(frame, middle[1]);

        self.render_output(frame, columns[2]);
    }

    fn render_workspace(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from("Project: forge-tui"),
            Line::from("Binary: forge"),
            Line::from("Mode: local orchestration"),
            Line::from("Isolation: planned worktrees"),
            Line::from("Diff review: planned"),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(focused_panel("Workspace", self.focus == Focus::Workspace)),
            area,
        );
    }

    fn render_tasks(&self, frame: &mut Frame, area: Rect) {
        let items = self.tasks.iter().enumerate().map(|(idx, task)| {
            let marker = if idx == self.selected_task {
                "> "
            } else {
                "  "
            };
            ListItem::new(Line::from(vec![
                Span::raw(marker),
                Span::styled(&task.name, Style::default().fg(Color::White)),
                Span::raw("  "),
                Span::styled(&task.status, Style::default().fg(Color::Yellow)),
            ]))
        });

        frame.render_widget(
            List::new(items).block(focused_panel("Tasks", self.focus == Focus::Tasks)),
            area,
        );
    }

    fn render_agents(&self, frame: &mut Frame, area: Rect) {
        let items = self.agents.iter().enumerate().map(|(idx, agent)| {
            let marker = if idx == self.selected_agent {
                "> "
            } else {
                "  "
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::raw(marker),
                    Span::styled(&agent.name, Style::default().fg(Color::White)),
                    Span::raw("  "),
                    Span::styled(&agent.status, Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(&agent.purpose, Style::default().fg(Color::DarkGray)),
                ]),
            ])
        });

        frame.render_widget(
            List::new(items).block(focused_panel("Agents", self.focus == Focus::Agents)),
            area,
        );
    }

    fn render_output(&self, frame: &mut Frame, area: Rect) {
        let text = self
            .output
            .iter()
            .rev()
            .take(area.height.saturating_sub(2) as usize)
            .rev()
            .map(|line| Line::from(line.as_str()))
            .collect::<Vec<_>>();

        frame.render_widget(
            Paragraph::new(text)
                .block(focused_panel("Output", self.focus == Focus::Output))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let help = "Tab focus | Up/Down select | r run task | a spawn agent | q quit";
        frame.render_widget(Paragraph::new(help).block(panel("Keys")), area);
    }
}

struct Task {
    name: String,
    status: String,
    command: String,
}

impl Task {
    fn new(name: &str, status: &str, command: &str) -> Self {
        Self {
            name: name.to_string(),
            status: status.to_string(),
            command: command.to_string(),
        }
    }
}

struct Agent {
    name: String,
    status: String,
    purpose: String,
}

impl Agent {
    fn new(name: &str, status: &str, purpose: &str) -> Self {
        Self {
            name: name.to_string(),
            status: status.to_string(),
            purpose: purpose.to_string(),
        }
    }
}

fn bounded_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }

    let max = len as isize - 1;
    (current as isize + delta).clamp(0, max) as usize
}

fn focused_panel(title: &'static str, focused: bool) -> Block<'static> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    panel(title).border_style(style)
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default().title(title).borders(Borders::ALL)
}

fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}
