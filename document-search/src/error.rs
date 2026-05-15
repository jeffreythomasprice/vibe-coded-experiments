#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("config: {0}")]
    Config(#[from] crate::config::ConfigError),

    #[error("db: {0}")]
    Db(#[from] crate::db::DbError),

    #[error("ollama: {0}")]
    Ollama(#[from] crate::ollama::OllamaError),

    #[error("llm: {0}")]
    Llm(#[from] crate::llm::LlmError),

    #[error("ingest: {0}")]
    Ingest(#[from] crate::ingest::IngestError),

    #[error("command: {0}")]
    Command(#[from] crate::commands::CommandError),

    #[error("server: {0}")]
    Server(#[from] crate::server::ServerError),

    #[error("client: {0}")]
    Client(#[from] crate::client::ClientError),

    #[error("cli: {0}")]
    Cli(String),

    #[error("logging init failed: {0}")]
    Logging(String),
}
