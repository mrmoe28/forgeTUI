use std::{
    io::{self, BufRead, BufReader, IsTerminal, Stdout},
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
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

const DEFAULT_MODEL: &str = "ollama/glm-4.7:cloud";
const DEFAULT_AGENT_ROOT: &str = ".forge/agents";

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
    models: Vec<String>,
    next_job_id: usize,
    selected_task: usize,
    selected_agent: usize,
    show_sidebar: bool,
    auto_test: bool,
    tasks: Vec<Task>,
    agents: Vec<Agent>,
    jobs: Vec<BackendJob>,
    transcript: Vec<Message>,
    history: Vec<SessionRecord>,
    pending_review: Option<ReviewState>,
    status: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            input: String::new(),
            model: DEFAULT_MODEL.to_string(),
            models: vec![
                "ollama/glm-4.7:cloud".to_string(),
                "ollama/glm-4.6:cloud".to_string(),
                "ollama/qwen3-coder:480b-cloud".to_string(),
                "ollama/gpt-oss:120b-cloud".to_string(),
                "ollama/minimax-m2:cloud".to_string(),
                "ollama/minimax-m2.1:cloud".to_string(),
                "ollama/kimi-k2.6:cloud".to_string(),
                "ollama/deepseek-v4-flash:cloud".to_string(),
                "ollama/qwen2.5-coder:32b".to_string(),
                "ollama/qwen2.5-coder:7b".to_string(),
                "ollama/hermes3:claude".to_string(),
                "ollama/mistral-small:24b".to_string(),
            ],
            next_job_id: 1,
            selected_task: 0,
            selected_agent: 0,
            show_sidebar: false,
            auto_test: true,
            tasks: vec![
                Task::new("Build TUI shell", "ready", "cargo run"),
                Task::new("Run tests", "queued", "cargo check"),
                Task::new("Show diff", "queued", "git diff --stat"),
            ],
            agents: vec![Agent::new(
                "main",
                "idle",
                "Primary workspace agent",
                PathBuf::from("."),
                None,
            )],
            jobs: Vec::new(),
            transcript: vec![
                Message::system("ForgeTUI started in agent workspace mode."),
                Message::assistant(format!(
                    "Normal prompts run opencode with {DEFAULT_MODEL}. Use /help for commands."
                )),
            ],
            history: Vec::new(),
            pending_review: None,
            status: "ready".to_string(),
        }
    }
}

impl App {
    fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        loop {
            self.drain_backend_events();
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
            }
            | KeyEvent {
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
            } => self.spawn_real_agent("Review the project and wait for a scoped task."),
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
                code: KeyCode::Char('x'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.cancel_latest_job(),
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
        self.handle_command_or_prompt(&submitted);
    }

    fn handle_command_or_prompt(&mut self, submitted: &str) {
        let mut parts = submitted.splitn(2, ' ');
        let command = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or_default().trim();

        match command {
            "/agent" => self.spawn_real_agent(if rest.is_empty() {
                "Review the project and wait for a scoped task."
            } else {
                rest
            }),
            "/approve" => self.approve_review(),
            "/cancel" => self.cancel_latest_job(),
            "/diff" => self.show_pending_diff(),
            "/history" => self.show_history(),
            "/model" => self.set_or_show_model(rest),
            "/models" => self.show_models(),
            "/reject" => self.reject_review(),
            "/run" => self.run_selected_task(),
            "/sidebar" => self.toggle_sidebar(),
            "/test" => self.run_tests_for_workspace(PathBuf::from(".")),
            "/autotest" => self.toggle_auto_test(),
            "/help" => self.show_help(),
            _ => self.start_main_job(submitted),
        }
    }

    fn show_help(&mut self) {
        self.transcript.push(Message::assistant(
            "Commands: /model [provider/model], /models, /agent <task>, /cancel, /diff, /approve, /reject, /test, /autotest, /history, /sidebar. Normal prompts run opencode in the main workspace.",
        ));
    }

    fn start_main_job(&mut self, prompt: &str) {
        if self.has_running_main_job() {
            self.transcript
                .push(Message::system("main opencode job is already running"));
            return;
        }

        let before = WorkspaceSnapshot::capture(PathBuf::from("."));
        self.start_opencode_job("main", prompt, PathBuf::from("."), before, None);
    }

    fn start_opencode_job(
        &mut self,
        label: &str,
        prompt: &str,
        workspace: PathBuf,
        before: WorkspaceSnapshot,
        agent_index: Option<usize>,
    ) {
        let id = self.next_job_id;
        self.next_job_id += 1;
        let model = self.model.clone();
        let label = label.to_string();
        let prompt = prompt.to_string();
        let (tx, rx) = mpsc::channel();

        self.status = format!("running job #{id}");
        let backend = BackendKind::for_model(&model);
        self.transcript.push(Message::system(format!(
            "job #{id}: {} {model} in {}",
            backend.label(),
            workspace.display()
        )));

        thread::spawn({
            let model = model.clone();
            let prompt = prompt.clone();
            let workspace = workspace.clone();
            move || run_model_worker(model, prompt, workspace, tx)
        });

        self.jobs.push(BackendJob {
            id,
            label,
            prompt,
            model,
            workspace,
            before,
            rx,
            pid: None,
            running: true,
            status: "running".to_string(),
            output: String::new(),
            agent_index,
            kind: JobKind::Opencode,
        });
    }

    fn drain_backend_events(&mut self) {
        let mut idx = 0;
        while idx < self.jobs.len() {
            let mut still_running = self.jobs[idx].running;

            loop {
                match self.jobs[idx].rx.try_recv() {
                    Ok(BackendEvent::Started(pid)) => {
                        self.jobs[idx].pid = Some(pid);
                    }
                    Ok(BackendEvent::Stdout(line)) => {
                        if !self.jobs[idx].output.is_empty() {
                            self.jobs[idx].output.push('\n');
                        }
                        self.jobs[idx].output.push_str(&line);
                        self.push_streamed_assistant_line(format!(
                            "[job #{}] {line}",
                            self.jobs[idx].id
                        ));
                    }
                    Ok(BackendEvent::Stderr(line)) => {
                        self.transcript.push(Message::system(format!(
                            "job #{}: {line}",
                            self.jobs[idx].id
                        )));
                    }
                    Ok(BackendEvent::Error(message)) => {
                        self.transcript.push(Message::system(format!(
                            "job #{}: {message}",
                            self.jobs[idx].id
                        )));
                    }
                    Ok(BackendEvent::Done(success)) => {
                        self.finish_job(idx, success);
                        still_running = false;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.finish_job(idx, false);
                        still_running = false;
                        break;
                    }
                }
            }

            if still_running {
                idx += 1;
            } else {
                idx += 1;
            }
        }

        if self.jobs.iter().any(|job| job.running) {
            let count = self.jobs.iter().filter(|job| job.running).count();
            self.status = format!("{count} job(s) running");
        } else if self.status.contains("running") {
            self.status = "ready".to_string();
        }
    }

    fn finish_job(&mut self, idx: usize, success: bool) {
        if !self.jobs[idx].running {
            return;
        }

        self.jobs[idx].running = false;
        self.jobs[idx].status = if success { "done" } else { "failed" }.to_string();
        let before = self.jobs[idx].before.clone();
        let after = WorkspaceSnapshot::capture(self.jobs[idx].workspace.clone());
        let changed = !after.status.trim().is_empty() || !after.diff_stat.trim().is_empty();

        self.transcript.push(Message::system(format!(
            "job #{} finished: {}",
            self.jobs[idx].id, self.jobs[idx].status
        )));

        self.show_snapshot_summary(&before, &after);

        if let Some(agent_index) = self.jobs[idx].agent_index {
            if let Some(agent) = self.agents.get_mut(agent_index) {
                agent.status = self.jobs[idx].status.clone();
            }
        } else if changed {
            self.pending_review = Some(ReviewState {
                job_id: self.jobs[idx].id,
                workspace: self.jobs[idx].workspace.clone(),
                before_clean: self.jobs[idx].before.is_clean(),
                before_status: self.jobs[idx].before.status.clone(),
                after_status: after.status.clone(),
                diff_stat: after.diff_stat.clone(),
                diff: after.diff.clone(),
                accepted: false,
            });

            self.transcript.push(Message::system(
                "changes are pending review: use /diff, /approve, or /reject",
            ));

            if self.auto_test {
                self.run_tests_for_workspace(self.jobs[idx].workspace.clone());
            }
        }

        self.history.push(SessionRecord {
            job_id: self.jobs[idx].id,
            label: self.jobs[idx].label.clone(),
            model: self.jobs[idx].model.clone(),
            workspace: self.jobs[idx].workspace.display().to_string(),
            prompt: self.jobs[idx].prompt.clone(),
            status: self.jobs[idx].status.clone(),
            changed,
        });
    }

    fn show_snapshot_summary(&mut self, before: &WorkspaceSnapshot, after: &WorkspaceSnapshot) {
        let before_status = if before.status.trim().is_empty() {
            "clean".to_string()
        } else {
            before.status.clone()
        };
        let after_status = if after.status.trim().is_empty() {
            "clean".to_string()
        } else {
            after.status.clone()
        };

        self.transcript.push(Message::system(format!(
            "before status:\n{before_status}\nafter status:\n{after_status}"
        )));

        if !after.diff_stat.trim().is_empty() {
            self.transcript
                .push(Message::system(format!("diff stat:\n{}", after.diff_stat)));
        }
    }

    fn push_streamed_assistant_line(&mut self, line: String) {
        if let Some(Message {
            role: Role::Assistant,
            body,
        }) = self.transcript.last_mut()
        {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(&line);
        } else {
            self.transcript.push(Message::assistant(line));
        }
    }

    fn spawn_real_agent(&mut self, task: &str) {
        let agent_number = self.agents.len();
        let name = format!("agent-{agent_number}");
        let workspace = PathBuf::from(DEFAULT_AGENT_ROOT).join(&name);
        let branch = format!("forge/{name}");

        match create_worktree(&workspace, &branch) {
            Ok(()) => {
                let agent_index = self.agents.len();
                self.agents.push(Agent::new(
                    &name,
                    "running",
                    task,
                    workspace.clone(),
                    Some(0),
                ));
                self.selected_agent = agent_index;
                self.status = format!("spawned {name}");
                self.transcript.push(Message::system(format!(
                    "spawned {name} in {}",
                    workspace.display()
                )));

                let before = WorkspaceSnapshot::capture(workspace.clone());
                self.start_opencode_job(&name, task, workspace, before, Some(agent_index));
                let job_id = self.next_job_id.saturating_sub(1);
                if let Some(agent) = self.agents.get_mut(agent_index) {
                    agent.job_id = Some(job_id);
                }
            }
            Err(err) => {
                self.transcript.push(Message::system(format!(
                    "failed to create agent worktree: {err}"
                )));
            }
        }
    }

    fn cancel_latest_job(&mut self) {
        let Some(job) = self.jobs.iter_mut().rev().find(|job| job.running) else {
            self.transcript
                .push(Message::system("no running job to cancel"));
            return;
        };

        let Some(pid) = job.pid else {
            self.transcript.push(Message::system(format!(
                "job #{} has no process id yet",
                job.id
            )));
            return;
        };

        let result = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status();
        match result {
            Ok(status) if status.success() => {
                job.status = "cancelled".to_string();
                self.status = format!("cancelled job #{}", job.id);
                self.transcript
                    .push(Message::system(format!("sent cancel to job #{}", job.id)));
            }
            Ok(status) => self.transcript.push(Message::system(format!(
                "kill failed for job #{} with status {status}",
                job.id
            ))),
            Err(err) => self.transcript.push(Message::system(format!(
                "failed to cancel job #{}: {err}",
                job.id
            ))),
        }
    }

    fn approve_review(&mut self) {
        let Some(review) = self.pending_review.as_mut() else {
            self.transcript
                .push(Message::system("no pending review to approve"));
            return;
        };

        review.accepted = true;
        self.transcript.push(Message::system(format!(
            "approved changes from job #{}",
            review.job_id
        )));
    }

    fn reject_review(&mut self) {
        let Some(review) = self.pending_review.take() else {
            self.transcript
                .push(Message::system("no pending review to reject"));
            return;
        };

        if !review.before_clean {
            self.transcript.push(Message::system(format!(
                "reject refused because the workspace was not clean before the run:\n{}",
                review.before_status
            )));
            self.pending_review = Some(review);
            return;
        }

        let status = Command::new("git")
            .args(["restore", "--worktree", "--staged", "."])
            .current_dir(&review.workspace)
            .status();

        match status {
            Ok(status) if status.success() => self.transcript.push(Message::system(format!(
                "rejected changes from job #{}",
                review.job_id
            ))),
            Ok(status) => {
                self.transcript
                    .push(Message::system(format!("git restore failed with {status}")));
                self.pending_review = Some(review);
            }
            Err(err) => {
                self.transcript
                    .push(Message::system(format!("failed to run git restore: {err}")));
                self.pending_review = Some(review);
            }
        }
    }

    fn show_pending_diff(&mut self) {
        let Some(review) = &self.pending_review else {
            self.transcript.push(Message::system("no pending diff"));
            return;
        };

        let diff = if review.diff.trim().is_empty() {
            "(no diff)".to_string()
        } else {
            truncate_chars(&review.diff, 6000)
        };
        self.transcript.push(Message::system(format!(
            "job #{} diff:\n{}\n\nstatus:\n{}\n\nstat:\n{}",
            review.job_id, diff, review.after_status, review.diff_stat
        )));
    }

    fn set_or_show_model(&mut self, rest: &str) {
        if rest.is_empty() {
            self.transcript
                .push(Message::system(format!("current model: {}", self.model)));
            return;
        }

        self.model = rest.to_string();
        self.status = format!("model {}", self.model);
        self.transcript
            .push(Message::system(format!("model set to {}", self.model)));
    }

    fn show_models(&mut self) {
        self.transcript.push(Message::system(format!(
            "known models:\n{}",
            self.models.join("\n")
        )));
    }

    fn show_history(&mut self) {
        if self.history.is_empty() {
            self.transcript
                .push(Message::system("no session history yet"));
            return;
        }

        let lines = self
            .history
            .iter()
            .rev()
            .take(12)
            .map(|record| {
                format!(
                    "#{} {} {} changed={} model={} workspace={} prompt={}",
                    record.job_id,
                    record.label,
                    record.status,
                    record.changed,
                    record.model,
                    record.workspace,
                    truncate_chars(&record.prompt, 80)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.transcript
            .push(Message::system(format!("history:\n{lines}")));
    }

    fn run_selected_task(&mut self) {
        if let Some(task) = self.tasks.get_mut(self.selected_task) {
            task.status = "started".to_string();
            let command = task.command.clone();
            let name = task.name.clone();
            self.status = format!("started {name}");
            self.transcript
                .push(Message::system(format!("running task `{name}`: {command}")));
            self.run_shell_job(&format!("task:{name}"), &command, PathBuf::from("."));
        }
    }

    fn run_tests_for_workspace(&mut self, workspace: PathBuf) {
        self.run_shell_job("auto-test", "cargo check", workspace);
    }

    fn run_shell_job(&mut self, label: &str, command: &str, workspace: PathBuf) {
        let id = self.next_job_id;
        self.next_job_id += 1;
        let (tx, rx) = mpsc::channel();
        let command = command.to_string();
        let before = WorkspaceSnapshot::capture(workspace.clone());

        thread::spawn({
            let command = command.clone();
            let workspace = workspace.clone();
            move || run_shell_worker(command, workspace, tx)
        });

        self.jobs.push(BackendJob {
            id,
            label: label.to_string(),
            prompt: command,
            model: "shell".to_string(),
            workspace,
            before,
            rx,
            pid: None,
            running: true,
            status: "running".to_string(),
            output: String::new(),
            agent_index: None,
            kind: JobKind::Shell,
        });
    }

    fn toggle_auto_test(&mut self) {
        self.auto_test = !self.auto_test;
        self.transcript.push(Message::system(format!(
            "auto-test {}",
            if self.auto_test {
                "enabled"
            } else {
                "disabled"
            }
        )));
    }

    fn toggle_sidebar(&mut self) {
        self.show_sidebar = !self.show_sidebar;
        self.status = if self.show_sidebar {
            "sidebar shown".to_string()
        } else {
            "sidebar hidden".to_string()
        };
    }

    fn has_running_main_job(&self) -> bool {
        self.jobs.iter().any(|job| {
            job.running && job.workspace == PathBuf::from(".") && job.kind == JobKind::Opencode
        })
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
        let running = self.jobs.iter().filter(|job| job.running).count();
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
            Span::raw("  "),
            Span::styled(
                format!("jobs:{running}"),
                Style::default().fg(Color::Yellow),
            ),
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
                Constraint::Length(7),
                Constraint::Length(7),
                Constraint::Length(9),
                Constraint::Min(0),
            ])
            .split(area);

        self.render_project(frame, rows[0]);
        self.render_tasks(frame, rows[1]);
        self.render_agents(frame, rows[2]);
        self.render_jobs(frame, rows[3]);
    }

    fn render_project(&self, frame: &mut Frame, area: Rect) {
        let review = self
            .pending_review
            .as_ref()
            .map(|review| format!("review  job #{}", review.job_id))
            .unwrap_or_else(|| "review  none".to_string());
        let lines = vec![
            Line::from("project forge-tui"),
            Line::from("binary  forge"),
            Line::from(format!("model   {}", self.model)),
            Line::from(format!(
                "tests   {}",
                if self.auto_test { "auto" } else { "manual" }
            )),
            Line::from(review),
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
                    Span::styled(
                        format!("{} ({})", agent.workspace.display(), agent.purpose),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
            ])
        });

        frame.render_widget(List::new(items).block(panel("agents")), area);
    }

    fn render_jobs(&self, frame: &mut Frame, area: Rect) {
        let items = self.jobs.iter().rev().take(8).map(|job| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("#{} ", job.id), Style::default().fg(Color::Cyan)),
                Span::raw(&job.label),
                Span::raw(" "),
                Span::styled(&job.status, Style::default().fg(Color::Yellow)),
            ]))
        });

        frame.render_widget(List::new(items).block(panel("jobs")), area);
    }

    fn render_composer(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);
        let prompt = if self.input.is_empty() {
            "Ask ForgeTUI to change code, run /agent task, /diff, /approve, /reject..."
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
        let help = "Enter send | /model | /agent | /cancel | /diff | /approve | /reject | /history | Esc quit";
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

#[derive(PartialEq, Eq)]
enum JobKind {
    Opencode,
    Shell,
}

struct BackendJob {
    id: usize,
    label: String,
    prompt: String,
    model: String,
    workspace: PathBuf,
    before: WorkspaceSnapshot,
    rx: Receiver<BackendEvent>,
    pid: Option<u32>,
    running: bool,
    status: String,
    output: String,
    agent_index: Option<usize>,
    kind: JobKind,
}

enum BackendEvent {
    Started(u32),
    Stdout(String),
    Stderr(String),
    Error(String),
    Done(bool),
}

enum StreamKind {
    Stdout,
    Stderr,
}

enum BackendKind {
    Opencode,
    Ollama,
}

impl BackendKind {
    fn for_model(model: &str) -> Self {
        if model.starts_with("ollama/") && !opencode_model_is_configured(model) {
            Self::Ollama
        } else {
            Self::Opencode
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Opencode => "opencode run -m",
            Self::Ollama => "ollama run",
        }
    }
}

#[derive(Clone)]
struct WorkspaceSnapshot {
    status: String,
    diff_stat: String,
    diff: String,
}

impl WorkspaceSnapshot {
    fn capture(workspace: PathBuf) -> Self {
        Self {
            status: run_git_capture(&workspace, &["status", "--short"]),
            diff_stat: run_git_capture(&workspace, &["diff", "--stat"]),
            diff: run_git_capture(&workspace, &["diff"]),
        }
    }

    fn is_clean(&self) -> bool {
        self.status.trim().is_empty() && self.diff.trim().is_empty()
    }
}

struct ReviewState {
    job_id: usize,
    workspace: PathBuf,
    before_clean: bool,
    before_status: String,
    after_status: String,
    diff_stat: String,
    diff: String,
    accepted: bool,
}

struct SessionRecord {
    job_id: usize,
    label: String,
    model: String,
    workspace: String,
    prompt: String,
    status: String,
    changed: bool,
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
    workspace: PathBuf,
    job_id: Option<usize>,
}

impl Agent {
    fn new(
        name: &str,
        status: &str,
        purpose: &str,
        workspace: PathBuf,
        job_id: Option<usize>,
    ) -> Self {
        Self {
            name: name.to_string(),
            status: status.to_string(),
            purpose: purpose.to_string(),
            workspace,
            job_id,
        }
    }
}

fn run_model_worker(
    model: String,
    prompt: String,
    workspace: PathBuf,
    tx: mpsc::Sender<BackendEvent>,
) {
    let backend = BackendKind::for_model(&model);
    let child = match backend {
        BackendKind::Opencode => Command::new("opencode")
            .args(["run", "-m", model.as_str(), prompt.as_str()])
            .current_dir(workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn(),
        BackendKind::Ollama => {
            let model = model.trim_start_matches("ollama/");
            Command::new("ollama")
                .args(["run", model, prompt.as_str()])
                .current_dir(workspace)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
        }
    };

    let mut child = match child {
        Ok(child) => child,
        Err(err) => {
            let _ = tx.send(BackendEvent::Error(format!(
                "failed to start model backend: {err}"
            )));
            let _ = tx.send(BackendEvent::Done(false));
            return;
        }
    };

    let _ = tx.send(BackendEvent::Started(child.id()));
    stream_child(&mut child, tx);
}

fn run_shell_worker(command: String, workspace: PathBuf, tx: mpsc::Sender<BackendEvent>) {
    let mut child = match Command::new("sh")
        .args(["-lc", command.as_str()])
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let _ = tx.send(BackendEvent::Error(format!(
                "failed to start shell task: {err}"
            )));
            let _ = tx.send(BackendEvent::Done(false));
            return;
        }
    };

    let _ = tx.send(BackendEvent::Started(child.id()));
    stream_child(&mut child, tx);
}

fn stream_child(child: &mut std::process::Child, tx: mpsc::Sender<BackendEvent>) {
    if let Some(stdout) = child.stdout.take() {
        let tx = tx.clone();
        thread::spawn(move || stream_reader(stdout, StreamKind::Stdout, tx));
    }

    if let Some(stderr) = child.stderr.take() {
        let tx = tx.clone();
        thread::spawn(move || stream_reader(stderr, StreamKind::Stderr, tx));
    }

    let success = match child.wait() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            let _ = tx.send(BackendEvent::Error(format!(
                "process exited with status {status}"
            )));
            false
        }
        Err(err) => {
            let _ = tx.send(BackendEvent::Error(format!(
                "failed waiting for process: {err}"
            )));
            false
        }
    };

    let _ = tx.send(BackendEvent::Done(success));
}

fn create_worktree(workspace: &PathBuf, branch: &str) -> Result<(), String> {
    if workspace.exists() {
        return Ok(());
    }

    let output = Command::new("git")
        .args(["worktree", "add", "-b", branch])
        .arg(workspace)
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_git_capture(workspace: &PathBuf, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output();
    match output {
        Ok(output) => clean_command_output(&format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )),
        Err(err) => format!("git failed: {err}"),
    }
}

fn opencode_model_is_configured(model: &str) -> bool {
    matches!(
        model,
        "ollama/deepseek-v4-flash:cloud"
            | "ollama/glm-4.6:cloud"
            | "ollama/glm-4.7:cloud"
            | "ollama/gpt-oss:120b-cloud"
            | "ollama/hermes3:8b"
            | "ollama/hermes3:claude"
            | "ollama/kimi-k2.6:cloud"
            | "ollama/llama3.1:8b"
            | "ollama/minimax-m2:cloud"
            | "ollama/minimax-m2.1:cloud"
            | "ollama/mistral-small:24b"
            | "ollama/qwen2.5-coder:32b"
            | "ollama/qwen2.5-coder:7b"
            | "ollama/qwen2.5-coder:7b-claude"
            | "ollama/qwen2.5:14b"
            | "ollama/qwen3-coder:480b-cloud"
    )
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

fn stream_reader<R>(reader: R, kind: StreamKind, tx: mpsc::Sender<BackendEvent>)
where
    R: io::Read,
{
    let reader = BufReader::new(reader);

    for line in reader.lines() {
        let Ok(line) = line else {
            break;
        };
        let line = clean_command_output(&line);

        if line.trim().is_empty() {
            continue;
        }

        let event = match kind {
            StreamKind::Stdout => BackendEvent::Stdout(line),
            StreamKind::Stderr => BackendEvent::Stderr(line),
        };

        if tx.send(event).is_err() {
            break;
        }
    }
}

fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }

    let mut truncated = input.chars().take(max).collect::<String>();
    truncated.push_str("\n... truncated ...");
    truncated
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}
