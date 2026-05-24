use std::{
    io::{self, IsTerminal, Stdout},
    process::Command,
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
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
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

struct App {
    input: String,
    model: String,
    selected_task: usize,
    selected_agent: usize,
    show_sidebar: bool,
    tasks: Vec<Task>,
    agents: Vec<Agent>,
    transcript: Vec<Message>,
    status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input: String::new(),
            model: "ollama/glm-4.7:cloud".to_string(),
            selected_task: 0,
            selected_agent: 0,
            show_sidebar: true,
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
            transcript: vec![
                Message::system("ForgeTUI started in agent workspace mode."),
                Message::assistant("Type a request and press Enter to run opencode with ollama/glm-4.7:cloud. Use /agent, /run, /sidebar, or /help for local commands."),
            ],
            status: "ready".to_string(),
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
                code: KeyCode::Esc, ..
            } => return true,
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => self.submit_input(),
            KeyEvent {
                code: KeyCode::Char('\n' | '\r'),
                ..
            } => self.submit_input(),
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => {
                self.input.pop();
            }
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.spawn_placeholder_agent(),
            KeyEvent {
                code: KeyCode::Char('r'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.run_selected_task(),
            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.toggle_sidebar(),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => self.selected_task = bounded_index(self.selected_task, self.tasks.len(), 1),
            KeyEvent {
                code: KeyCode::Up, ..
            } => self.selected_task = bounded_index(self.selected_task, self.tasks.len(), -1),
            KeyEvent {
                code: KeyCode::Tab, ..
            } => self.selected_agent = bounded_index(self.selected_agent, self.agents.len(), 1),
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => self.selected_agent = bounded_index(self.selected_agent, self.agents.len(), -1),
            KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => self.input.push(ch),
            _ => {}
        }

        false
    }

    fn submit_input(&mut self) {
        let submitted = self.input.trim().to_string();
        self.input.clear();

        if submitted.is_empty() {
            return;
        }

        self.transcript.push(Message::user(&submitted));

        match submitted.as_str() {
            "/agent" => self.spawn_placeholder_agent(),
            "/run" => self.run_selected_task(),
            "/sidebar" => self.toggle_sidebar(),
            "/help" => self.transcript.push(Message::assistant(
                "Commands: /agent spawns a placeholder agent, /run starts the selected task, /sidebar toggles the side panels. Normal prompts run opencode with the configured model.",
            )),
            _ => {
                self.run_opencode(&submitted);
            }
        }
    }

    fn run_opencode(&mut self, prompt: &str) {
        self.status = format!("running {}", self.model);
        self.transcript.push(Message::system(format!(
            "Running `opencode run -m {} ...`",
            self.model
        )));

        let output = Command::new("opencode")
            .args(["run", "-m", self.model.as_str(), prompt])
            .output();

        match output {
            Ok(output) => {
                let stdout = clean_command_output(&String::from_utf8_lossy(&output.stdout));
                let stderr = clean_command_output(&String::from_utf8_lossy(&output.stderr));

                if !stdout.trim().is_empty() {
                    self.transcript.push(Message::assistant(stdout.trim()));
                }

                if !stderr.trim().is_empty() {
                    self.transcript
                        .push(Message::system(format!("stderr:\n{}", stderr.trim())));
                }

                if !output.status.success() {
                    self.transcript.push(Message::system(format!(
                        "opencode exited with status {}",
                        output.status
                    )));
                }
            }
            Err(err) => {
                self.transcript
                    .push(Message::system(format!("failed to start opencode: {err}")));
            }
        }

        self.status = "ready".to_string();
    }

    fn spawn_placeholder_agent(&mut self) {
        let name = format!("agent-{}", self.agents.len() + 1);
        self.agents
            .push(Agent::new(&name, "spawned", "Awaiting scoped coding task"));
        self.selected_agent = self.agents.len() - 1;
        self.status = format!("spawned {name}");
        self.transcript.push(Message::system(format!(
            "Spawned placeholder subagent `{name}`."
        )));
    }

    fn run_selected_task(&mut self) {
        if let Some(task) = self.tasks.get_mut(self.selected_task) {
            task.status = "started".to_string();
            self.status = format!("started {}", task.name);
            self.transcript.push(Message::system(format!(
                "Task `{}` would run: {}",
                task.name, task.command
            )));
        }
    }

    fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
        self.status = if self.show_sidebar {
            "sidebar shown".to_string()
        } else {
            "sidebar hidden".to_string()
        };
    }

    fn render(&self, frame: &mut Frame) {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(frame.area());

        self.render_header(frame, root[0]);
        self.render_workspace(frame, root[1]);
        self.render_composer(frame, root[2]);
        self.render_keys(frame, root[3]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let header = Line::from(vec![
            Span::styled(
                "ForgeTUI",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("agent workspace", Style::default().fg(Color::Gray)),
            Span::raw("  "),
            Span::styled(&self.status, Style::default().fg(Color::Green)),
        ]);
        frame.render_widget(Paragraph::new(header), area);
    }

    fn render_workspace(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);

        if !self.show_sidebar {
            self.render_transcript(frame, area);
            return;
        }

        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(area);

        self.render_transcript(frame, columns[0]);
        self.render_sidebar(frame, columns[1]);
    }

    fn render_transcript(&self, frame: &mut Frame, area: Rect) {
        let lines = self
            .transcript
            .iter()
            .flat_map(Message::render)
            .collect::<Vec<_>>();
        let visible = lines
            .into_iter()
            .rev()
            .take(area.height.saturating_sub(2) as usize)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        frame.render_widget(
            Paragraph::new(visible)
                .block(panel("conversation"))
                .wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_sidebar(&self, frame: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(7),
                Constraint::Min(0),
            ])
            .split(area);

        self.render_project(frame, rows[0]);
        self.render_tasks(frame, rows[1]);
        self.render_agents(frame, rows[2]);
    }

    fn render_project(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from("project  forge-tui"),
            Line::from("binary   forge"),
            Line::from("mode     local"),
            Line::from(format!("model   {}", self.model)),
        ];
        frame.render_widget(Paragraph::new(lines).block(panel("workspace")), area);
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
                Span::raw(" "),
                Span::styled(&task.status, Style::default().fg(Color::Yellow)),
            ]))
        });

        frame.render_widget(List::new(items).block(panel("tasks")), area);
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
                    Span::raw(" "),
                    Span::styled(&agent.status, Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(&agent.purpose, Style::default().fg(Color::DarkGray)),
                ]),
            ])
        });

        frame.render_widget(List::new(items).block(panel("agents")), area);
    }

    fn render_composer(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);
        let prompt = if self.input.is_empty() {
            "Ask ForgeTUI to change code, run a task, or spawn an agent..."
        } else {
            self.input.as_str()
        };
        let style = if self.input.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Cyan)),
                Span::styled(prompt, style),
            ]))
            .block(panel("prompt"))
            .wrap(Wrap { trim: false }),
            area,
        );

        let cursor_x = area
            .x
            .saturating_add(3)
            .saturating_add(self.input.len().min(area.width.saturating_sub(5) as usize) as u16);
        frame.set_cursor_position(Position::new(cursor_x, area.y.saturating_add(1)));
    }

    fn render_keys(&self, frame: &mut Frame, area: Rect) {
        let help =
            "Enter send | /agent spawn | /run task | /sidebar toggle | Ctrl-B sidebar | Esc quit";
        frame.render_widget(Paragraph::new(help), area);
    }
}

struct Message {
    role: Role,
    body: String,
}

impl Message {
    fn system(body: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            body: body.into(),
        }
    }

    fn user(body: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            body: body.into(),
        }
    }

    fn assistant(body: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            body: body.into(),
        }
    }

    fn render(&self) -> Vec<Line<'static>> {
        let (label, color) = match self.role {
            Role::System => ("system", Color::DarkGray),
            Role::User => ("you", Color::Cyan),
            Role::Assistant => ("forge", Color::Green),
        };

        vec![
            Line::from(vec![Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )]),
            Line::from(self.body.clone()),
            Line::from(""),
        ]
    }
}

enum Role {
    System,
    User,
    Assistant,
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

fn clean_command_output(input: &str) -> String {
    let mut cleaned = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }

        cleaned.push(ch);
    }

    cleaned
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}
