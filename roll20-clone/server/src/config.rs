use std::net::{IpAddr, SocketAddr};

/// Runtime configuration, sourced from environment variables (see `.env`).
pub struct Config {
    /// IP address to bind to. Defaults to `127.0.0.1`.
    pub host: IpAddr,
    /// Port to bind to. Defaults to `8001`.
    pub port: u16,
}

impl Config {
    pub fn from_env() -> Self {
        let host = std::env::var("SERVER_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string())
            .parse()
            .expect("SERVER_HOST must be a valid IP address");

        let port = std::env::var("SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8001);

        Self { host, port }
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}
