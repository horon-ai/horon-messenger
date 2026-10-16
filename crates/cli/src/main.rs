use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use futures_util::StreamExt;
use horon_messenger_sdk::{
    ChatQuery, Client, ConfigPaths, DynProvider, Envelope, Error, LoginOptions, MessageQuery, Meta, ProviderKind,
    SendRequest,
};
use serde_json::{Map, Value};

mod daemon;
mod update;

#[derive(Debug, Parser)]
#[command(
    name = "horon-messenger",
    version,
    about = "Multi-messenger bridge for AI agents",
    propagate_version = true
)]
struct Cli {
    /// Pretty-print the JSON envelope.
    #[arg(long, global = true)]
    pretty: bool,

    /// Config directory, overriding the platform default.
    #[arg(long, global = true, env = "HORON_MESSENGER_CONFIG_DIR", value_name = "DIR")]
    config_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List registered providers and their capabilities.
    Providers,

    /// Show the authenticated account for a provider.
    Whoami { provider: String },

    /// List chats.
    Chats {
        provider: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },

    /// List or search messages in one chat.
    Messages {
        provider: String,
        chat: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Send a message to a chat.
    Send {
        provider: String,
        chat: String,
        text: String,
        #[arg(long)]
        reply_to: Option<String>,
    },

    /// List the members of a chat.
    Members { provider: String, chat: String },

    /// Print a provider snapshot.
    Snapshot { provider: String },

    /// Show one chat by id.
    Chat { provider: String, chat: String },

    /// Search messages, in one chat or across every chat.
    Search {
        provider: String,
        #[arg(long)]
        chat: Option<String>,
        query: String,
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Mark a chat as read.
    MarkRead { provider: String, chat: String },

    /// Stream events from a provider until interrupted.
    Listen {
        provider: String,
        #[arg(long)]
        chat: Option<String>,
        /// Stop after this many events (default: run until interrupted).
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Authentication.
    Auth(AuthArgs),

    /// Manage the HTTP API server as a background daemon.
    Daemon(DaemonArgs),

    /// Prepare the local configuration and report provider readiness.
    Setup(SetupArgs),

    /// Check for and install a newer release.
    Update(UpdateArgs),
}

#[derive(Debug, Args)]
struct DaemonArgs {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Show whether the server is running.
    Status,
    /// Start the server in the background.
    Start {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Stop the background server.
    Stop,
    /// Restart the background server.
    Restart {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
}

#[derive(Debug, Args)]
struct SetupArgs {
    /// Verify only; do not create the config directory.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Report whether an update is available without installing it.
    #[arg(long)]
    check: bool,
}

#[derive(Debug, Args)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommand,
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Show stored accounts and the current one.
    Status { provider: String },
    /// Sign in.
    Login {
        provider: String,
        #[arg(long)]
        account: Option<String>,
        /// Provider-specific credential field, as `key=value`; repeatable.
        #[arg(long = "param", value_name = "KEY=VALUE", value_parser = parse_param)]
        params: Vec<(String, String)>,
    },
    /// Sign out.
    Logout {
        provider: String,
        #[arg(long)]
        account: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let pretty = cli.pretty;
    let envelope = match build_client(&cli) {
        Ok(client) => match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime.block_on(dispatch(&client, cli.command, pretty)),
            Err(error) => Envelope::failure(
                &Error::Internal {
                    message: format!("failed to start the async runtime: {error}"),
                },
                Meta::now(None),
            ),
        },
        Err(error) => Envelope::failure(&error, Meta::now(None)),
    };
    let rendered = if pretty {
        envelope.to_pretty()
    } else {
        envelope.to_json()
    };
    println!("{rendered}");
    if envelope.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn build_client(cli: &Cli) -> Result<Client, Error> {
    let mut builder = Client::builder();
    if let Some(dir) = &cli.config_dir {
        builder = builder.config_dir(dir.clone());
    }
    builder.build()
}

fn resolve(provider: &str) -> Result<ProviderKind, Error> {
    provider.parse::<ProviderKind>()
}

fn parse_param(input: &str) -> Result<(String, String), String> {
    match input.split_once('=') {
        Some((key, value)) if !key.is_empty() => Ok((key.to_owned(), value.to_owned())),
        _ => Err(format!("expected `key=value`, got `{input}`")),
    }
}

fn params_value(params: Vec<(String, String)>) -> Value {
    if params.is_empty() {
        return Value::Null;
    }
    Value::Object(
        params
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect::<Map<String, Value>>(),
    )
}

async fn dispatch(client: &Client, command: Command, pretty: bool) -> Envelope {
    match command {
        Command::Providers => Envelope::success(client.describe(), Meta::now(None)),
        Command::Whoami { provider } => match resolve(&provider) {
            Ok(provider) => Envelope::from_result(client.whoami(provider).await, Meta::now(Some(provider))),
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Chats {
            provider,
            search,
            limit,
        } => match resolve(&provider) {
            Ok(provider) => {
                let query = ChatQuery { search, limit };
                Envelope::from_result(client.list_chats(provider, &query).await, Meta::now(Some(provider)))
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Messages {
            provider,
            chat,
            search,
            limit,
        } => match resolve(&provider) {
            Ok(provider) => {
                let query = MessageQuery {
                    search,
                    limit,
                    before: None,
                    after: None,
                };
                Envelope::from_result(
                    client.list_messages(provider, &chat, &query).await,
                    Meta::now(Some(provider)),
                )
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Send {
            provider,
            chat,
            text,
            reply_to,
        } => match resolve(&provider) {
            Ok(provider) => {
                let request = SendRequest {
                    chat_id: chat,
                    text,
                    reply_to,
                    attachments: Vec::new(),
                    idempotency_key: None,
                };
                Envelope::from_result(client.send_message(provider, &request).await, Meta::now(Some(provider)))
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Members { provider, chat } => match resolve(&provider) {
            Ok(provider) => {
                Envelope::from_result(client.list_members(provider, &chat).await, Meta::now(Some(provider)))
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Snapshot { provider } => match resolve(&provider) {
            Ok(provider) => Envelope::from_result(client.snapshot(provider).await, Meta::now(Some(provider))),
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Chat { provider, chat } => match resolve(&provider) {
            Ok(provider) => Envelope::from_result(client.get_chat(provider, &chat).await, Meta::now(Some(provider))),
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Search {
            provider,
            chat,
            query,
            limit,
        } => match resolve(&provider) {
            Ok(provider) => {
                let query = MessageQuery {
                    search: Some(query),
                    limit,
                    before: None,
                    after: None,
                };
                Envelope::from_result(
                    client.search_messages(provider, chat.as_deref(), &query).await,
                    Meta::now(Some(provider)),
                )
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::MarkRead { provider, chat } => match resolve(&provider) {
            Ok(provider) => Envelope::from_result(client.mark_read(provider, &chat).await, Meta::now(Some(provider))),
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Listen { provider, chat, limit } => match resolve(&provider) {
            Ok(provider) => dispatch_listen(client, provider, chat.as_deref(), limit, pretty).await,
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Auth(args) => dispatch_auth(client, args).await,
        Command::Daemon(args) => dispatch_daemon(client.paths(), args),
        Command::Setup(args) => dispatch_setup(client, args).await,
        Command::Update(args) => dispatch_update(args).await,
    }
}

async fn dispatch_auth(client: &Client, args: AuthArgs) -> Envelope {
    match args.command {
        AuthCommand::Status { provider } => {
            let kind = match resolve(&provider) {
                Ok(kind) => kind,
                Err(error) => return Envelope::failure(&error, Meta::now(None)),
            };
            match auth_for(client, kind) {
                Ok(instance) => {
                    let auth = instance.auth().expect("auth_for checked auth");
                    Envelope::from_result(auth.status().await, Meta::now(Some(kind)))
                }
                Err(error) => Envelope::failure(&error, Meta::now(Some(kind))),
            }
        }
        AuthCommand::Login {
            provider,
            account,
            params,
        } => {
            let kind = match resolve(&provider) {
                Ok(kind) => kind,
                Err(error) => return Envelope::failure(&error, Meta::now(None)),
            };
            match auth_for(client, kind) {
                Ok(instance) => {
                    let auth = instance.auth().expect("auth_for checked auth");
                    let options = LoginOptions {
                        account_id: account,
                        params: params_value(params),
                        ..LoginOptions::default()
                    };
                    Envelope::from_result(auth.login(&options).await, Meta::now(Some(kind)))
                }
                Err(error) => Envelope::failure(&error, Meta::now(Some(kind))),
            }
        }
        AuthCommand::Logout { provider, account } => {
            let kind = match resolve(&provider) {
                Ok(kind) => kind,
                Err(error) => return Envelope::failure(&error, Meta::now(None)),
            };
            match auth_for(client, kind) {
                Ok(instance) => {
                    let auth = instance.auth().expect("auth_for checked auth");
                    Envelope::from_result(auth.logout(account.as_deref()).await, Meta::now(Some(kind)))
                }
                Err(error) => Envelope::failure(&error, Meta::now(Some(kind))),
            }
        }
    }
}

fn auth_for(client: &Client, kind: ProviderKind) -> Result<DynProvider, Error> {
    let instance = client.provider(kind)?;
    if instance.auth().is_none() {
        return Err(Error::Unsupported {
            provider: kind,
            operation: "auth",
        });
    }
    Ok(instance)
}

async fn dispatch_listen(
    client: &Client,
    kind: ProviderKind,
    chat_id: Option<&str>,
    limit: Option<usize>,
    pretty: bool,
) -> Envelope {
    let mut stream = match client.listen(kind, chat_id).await {
        Ok(stream) => stream,
        Err(error) => return Envelope::failure(&error, Meta::now(Some(kind))),
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut count = 0usize;
    while let Some(item) = stream.next().await {
        match item {
            Ok(event) => {
                let line = if pretty {
                    serde_json::to_string_pretty(&event)
                } else {
                    serde_json::to_string(&event)
                };
                match line {
                    Ok(line) => {
                        let _ = writeln!(out, "{line}");
                    }
                    Err(error) => {
                        return Envelope::failure(
                            &Error::Internal {
                                message: error.to_string(),
                            },
                            Meta::now(Some(kind)),
                        );
                    }
                }
                count += 1;
                if limit.is_some_and(|limit| count >= limit) {
                    break;
                }
            }
            Err(error) => return Envelope::failure(&error, Meta::now(Some(kind))),
        }
    }
    Envelope::success(serde_json::json!({ "events": count }), Meta::now(Some(kind)))
}

fn dispatch_daemon(paths: &ConfigPaths, args: DaemonArgs) -> Envelope {
    let meta = Meta::now(None);
    match args.command {
        DaemonCommand::Status => Envelope::success(daemon::status(paths), meta),
        DaemonCommand::Start { host, port } => Envelope::from_result(daemon::start(paths, &host, port), meta),
        DaemonCommand::Stop => Envelope::from_result(daemon::stop(paths), meta),
        DaemonCommand::Restart { host, port } => Envelope::from_result(daemon::restart(paths, &host, port), meta),
    }
}

async fn dispatch_setup(client: &Client, args: SetupArgs) -> Envelope {
    let meta = Meta::now(None);
    let paths = client.paths();

    let mut created = false;
    if !args.check && !paths.root().exists() {
        if let Err(error) = horon_messenger_sdk::config::create_private_dir(paths.root()) {
            return Envelope::failure(&error, meta);
        }
        created = true;
    }

    let mut providers = Vec::new();
    for (kind, provider) in client.registry().iter() {
        let (auth_supported, authenticated, accounts) = match provider.auth() {
            Some(auth) => match auth.status().await {
                Ok(status) => (true, status.authenticated, status.accounts.len()),
                Err(_) => (true, false, 0),
            },
            None => (false, false, 0),
        };
        providers.push(serde_json::json!({
            "kind": kind.as_str(),
            "name": kind.display_name(),
            "auth_supported": auth_supported,
            "authenticated": authenticated,
            "accounts": accounts,
        }));
    }

    Envelope::success(
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "config_dir": paths.root().display().to_string(),
            "config_dir_created": created,
            "providers": providers,
            "next_steps": [
                "sign in to a provider: horon-messenger auth login <provider>",
                "start the API server: horon-messenger daemon start",
            ],
        }),
        meta,
    )
}

async fn dispatch_update(args: UpdateArgs) -> Envelope {
    let meta = Meta::now(None);
    let current = env!("CARGO_PKG_VERSION");
    let installed_package = std::env::var("HORON_MESSENGER_PACKAGE").ok();
    let package = installed_package
        .clone()
        .unwrap_or_else(|| update::DEFAULT_PACKAGE.to_owned());

    let latest = match update::latest_version(&package).await {
        Ok(latest) => latest,
        Err(error) => return Envelope::failure(&error, meta),
    };
    let available = update::is_newer(&latest, current);

    if args.check || !available || installed_package.is_none() {
        let mut data = serde_json::json!({
            "current": current,
            "latest": latest,
            "package": package,
            "update_available": available,
        });
        if available && installed_package.is_none() {
            data["installed"] = Value::Bool(false);
            data["hint"] = Value::String(format!(
                "this build was not installed from npm; update it with your installer, or run: npm install -g {package}@latest"
            ));
        }
        return Envelope::success(data, meta);
    }

    Envelope::from_result(update::install(&package, current, &latest), meta)
}
