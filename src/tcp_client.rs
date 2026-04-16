use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::TcpStream;

/// Async TCP client for the ElysianDB key-value text protocol.
///
/// Protocol: line-delimited commands over a raw TCP socket.
/// Send `COMMAND [args]\n`, read response line(s) terminated by `\n`.
pub struct ElysianTcpClient {
    reader: BufReader<tokio::io::ReadHalf<TcpStream>>,
    writer: BufWriter<tokio::io::WriteHalf<TcpStream>>,
}

impl ElysianTcpClient {
    /// Connect to `127.0.0.1:{port}`.
    pub async fn connect(port: u16) -> Result<Self> {
        let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .context("TCP connect failed")?;

        let (read_half, write_half) = tokio::io::split(stream);

        Ok(Self {
            reader: BufReader::new(read_half),
            writer: BufWriter::new(write_half),
        })
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    async fn send(&mut self, cmd: &str) -> Result<()> {
        self.writer
            .write_all(cmd.as_bytes())
            .await
            .context("TCP write failed")?;
        self.writer
            .write_all(b"\n")
            .await
            .context("TCP write newline failed")?;
        self.writer.flush().await.context("TCP flush failed")?;
        Ok(())
    }

    async fn read_line(&mut self) -> Result<String> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .await
            .context("TCP read failed")?;
        // Strip trailing newline / carriage-return
        let trimmed = line.trim_end_matches(&['\r', '\n'][..]).to_string();
        Ok(trimmed)
    }

    async fn command(&mut self, cmd: &str) -> Result<String> {
        self.send(cmd).await?;
        self.read_line().await
    }

    // ------------------------------------------------------------------
    // Commands
    // ------------------------------------------------------------------

    /// `PING` → expects `PONG`.
    pub async fn ping(&mut self) -> Result<String> {
        self.command("PING").await
    }

    /// `SET key value` → expects `OK`.
    pub async fn set(&mut self, key: &str, value: &str) -> Result<String> {
        self.command(&format!("SET {key} {value}")).await
    }

    /// `SET TTL=N key value` → expects `OK`.
    pub async fn set_ttl(&mut self, key: &str, value: &str, ttl: u64) -> Result<String> {
        self.command(&format!("SET TTL={ttl} {key} {value}")).await
    }

    /// `GET key` → returns the stored value.
    pub async fn get(&mut self, key: &str) -> Result<String> {
        self.command(&format!("GET {key}")).await
    }

    /// `MGET key1 key2 ...` → returns one line per key.
    pub async fn mget(&mut self, keys: &[&str]) -> Result<Vec<String>> {
        let cmd = format!("MGET {}", keys.join(" "));
        self.send(&cmd).await?;

        let mut results = Vec::with_capacity(keys.len());
        for _ in 0..keys.len() {
            results.push(self.read_line().await?);
        }
        Ok(results)
    }

    /// `DEL key` → expects `Deleted N`.
    pub async fn del(&mut self, key: &str) -> Result<String> {
        self.command(&format!("DEL {key}")).await
    }

    /// `RESET` → clears all KV keys, expects `OK`.
    pub async fn reset(&mut self) -> Result<String> {
        self.command("RESET").await
    }

    /// `SAVE` → forces flush to disk, expects `OK`.
    pub async fn save(&mut self) -> Result<String> {
        self.command("SAVE").await
    }
}

#[cfg(test)]
mod tests {
    // TCP client tests require a running ElysianDB instance, so unit tests
    // are limited to compile-time verification. Integration tests will
    // exercise the full protocol against a live server.

    #[test]
    fn tcp_client_module_compiles() {
        // Intentionally empty — validates that the module compiles.
    }
}
