use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use horon_messenger_sdk::{
    ChatQuery, Client, DynProvider, Envelope, Error, LoginOptions, MessageQuery, Meta,
    ProviderKind, SendRequest,
};
use serde_json::{Map, Value};

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
    #[arg(
        long,
        global = true,
        env = "HORON_MESSENGER_CONFIG_DIR",
        value_name = "DIR"
    )]
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

    /// Authentication.
    Auth(AuthArgs),
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
        Ok(client) => match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime.block_on(dispatch(&client, cli.command)),
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

async fn dispatch(client: &Client, command: Command) -> Envelope {
    match command {
        Command::Providers => Envelope::success(client.describe(), Meta::now(None)),
        Command::Whoami { provider } => match resolve(&provider) {
            Ok(provider) => {
                Envelope::from_result(client.whoami(provider).await, Meta::now(Some(provider)))
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Chats {
            provider,
            search,
            limit,
        } => match resolve(&provider) {
            Ok(provider) => {
                let query = ChatQuery { search, limit };
                Envelope::from_result(
                    client.list_chats(provider, &query).await,
                    Meta::now(Some(provider)),
                )
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
                Envelope::from_result(
                    client.send_message(provider, &request).await,
                    Meta::now(Some(provider)),
                )
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Members { provider, chat } => match resolve(&provider) {
            Ok(provider) => Envelope::from_result(
                client.list_members(provider, &chat).await,
                Meta::now(Some(provider)),
            ),
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Snapshot { provider } => match resolve(&provider) {
            Ok(provider) => {
                Envelope::from_result(client.snapshot(provider).await, Meta::now(Some(provider)))
            }
            Err(error) => Envelope::failure(&error, Meta::now(None)),
        },
        Command::Auth(args) => dispatch_auth(client, args).await,
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
                    Envelope::from_result(
                        auth.logout(account.as_deref()).await,
                        Meta::now(Some(kind)),
                    )
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
