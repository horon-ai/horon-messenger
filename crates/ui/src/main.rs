use std::io::{self, Stdout};
use std::time::Duration;

use clap::Parser;
use horon_messenger_sdk::{Chat, ChatKind, ChatQuery, Client, Message, MessageQuery, ProviderKind};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

#[derive(Debug, Parser)]
#[command(name = "horon-messenger-tui", version, about = "Terminal UI for horon-messenger")]
struct Args {
    #[arg(long, value_parser = parse_provider, default_value = "mock")]
    provider: ProviderKind,

    #[arg(long, help = "Print a text snapshot and exit instead of starting the TUI")]
    snapshot: bool,
}

fn parse_provider(value: &str) -> Result<ProviderKind, String> {
    value.parse::<ProviderKind>().map_err(|error| error.to_string())
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let client = Client::builder().build()?;
    let runtime = tokio::runtime::Runtime::new()?;
    if args.snapshot {
        runtime.block_on(print_snapshot(&client, args.provider))?;
        return Ok(());
    }
    run_tui(&runtime, &client, args.provider)
}

async fn print_snapshot(client: &Client, provider: ProviderKind) -> anyhow::Result<()> {
    let chats = client.list_chats(provider, &ChatQuery::default()).await?;
    println!("{}: {} chats", provider.display_name(), chats.len());
    for chat in &chats {
        println!("  {:<16} {:<8} {}", chat.id, chat_kind_label(chat.kind), chat.name);
    }
    if let Some(first) = chats.first() {
        let page = client
            .list_messages(provider, &first.id, &MessageQuery::default())
            .await?;
        println!("\n{} ({} messages):", first.name, page.messages.len());
        for message in &page.messages {
            let who = message.sender_name.clone().unwrap_or_else(|| "?".to_owned());
            println!("  {who}: {}", message.text);
        }
    }
    Ok(())
}

fn chat_kind_label(kind: ChatKind) -> &'static str {
    match kind {
        ChatKind::Direct => "direct",
        ChatKind::Group => "group",
        ChatKind::Channel => "channel",
        ChatKind::OpenChat => "open",
        ChatKind::Thread => "thread",
        ChatKind::Unknown => "unknown",
    }
}

struct App {
    provider: ProviderKind,
    chats: Vec<Chat>,
    chat_state: ListState,
    messages: Vec<Message>,
}

impl App {
    fn load(runtime: &tokio::runtime::Runtime, client: &Client, provider: ProviderKind) -> anyhow::Result<Self> {
        let chats = runtime.block_on(client.list_chats(provider, &ChatQuery::default()))?;
        let mut app = Self {
            provider,
            chats,
            chat_state: ListState::default(),
            messages: Vec::new(),
        };
        if !app.chats.is_empty() {
            app.chat_state.select(Some(0));
        }
        app.reload(runtime, client)?;
        Ok(app)
    }

    fn reload(&mut self, runtime: &tokio::runtime::Runtime, client: &Client) -> anyhow::Result<()> {
        let Some(chat) = self.chat_state.selected().and_then(|index| self.chats.get(index)) else {
            self.messages.clear();
            return Ok(());
        };
        let page = runtime.block_on(client.list_messages(self.provider, &chat.id, &MessageQuery::default()))?;
        self.messages = page.messages;
        Ok(())
    }

    fn select_next(&mut self, runtime: &tokio::runtime::Runtime, client: &Client) -> anyhow::Result<()> {
        let len = self.chats.len();
        if len == 0 {
            return Ok(());
        }
        let next = match self.chat_state.selected() {
            Some(index) if index + 1 < len => index + 1,
            _ => 0,
        };
        self.chat_state.select(Some(next));
        self.reload(runtime, client)
    }

    fn select_previous(&mut self, runtime: &tokio::runtime::Runtime, client: &Client) -> anyhow::Result<()> {
        let len = self.chats.len();
        if len == 0 {
            return Ok(());
        }
        let previous = match self.chat_state.selected() {
            Some(0) | None => len - 1,
            Some(index) => index - 1,
        };
        self.chat_state.select(Some(previous));
        self.reload(runtime, client)
    }
}

fn run_tui(runtime: &tokio::runtime::Runtime, client: &Client, provider: ProviderKind) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let result = event_loop(&mut terminal, runtime, client, provider);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    runtime: &tokio::runtime::Runtime,
    client: &Client,
    provider: ProviderKind,
) -> anyhow::Result<()> {
    let mut app = App::load(runtime, client, provider)?;
    loop {
        terminal.draw(|frame| draw(frame, &app))?;
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.select_next(runtime, client)?,
                KeyCode::Up | KeyCode::Char('k') => app.select_previous(runtime, client)?,
                _ => {}
            }
        }
    }
}

fn draw(frame: &mut ratatui::Frame, app: &App) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(frame.area());

    let items: Vec<ListItem> = app
        .chats
        .iter()
        .map(|chat| ListItem::new(Line::from(chat.name.clone())))
        .collect();
    let chats = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", app.provider.display_name())),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = app.chat_state.clone();
    frame.render_stateful_widget(chats, panes[0], &mut state);

    let lines: Vec<Line> = app
        .messages
        .iter()
        .map(|message| {
            let who = message.sender_name.clone().unwrap_or_else(|| "?".to_owned());
            Line::from(format!("{who}: {}", message.text))
        })
        .collect();
    let messages = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" messages "))
        .wrap(Wrap { trim: true });
    frame.render_widget(messages, panes[1]);
}
