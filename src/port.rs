use std::net::TcpListener;

use anyhow::{bail, Context, Result};

pub struct AvailablePorts {
    pub http_port: u16,
    pub tcp_port: u16,
}

/// Find two distinct available TCP ports on localhost.
///
/// Strategy: bind to `127.0.0.1:0` and let the OS assign an ephemeral port,
/// read the port number, then close the listener. Repeat for the second port.
pub fn find_available_ports() -> Result<AvailablePorts> {
    let http_port = find_one_port().context("Failed to find available HTTP port")?;
    let tcp_port = find_one_port().context("Failed to find available TCP port")?;

    if http_port == tcp_port {
        bail!(
            "OS assigned the same port ({http_port}) twice. \
             This is unexpected — please retry."
        );
    }

    Ok(AvailablePorts {
        http_port,
        tcp_port,
    })
}

fn find_one_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .context("Could not bind to 127.0.0.1:0 — no ports available")?;
    let port = listener
        .local_addr()
        .context("Could not read local address from bound socket")?
        .port();
    // Listener is dropped here, releasing the port.
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_available_ports_returns_distinct_ports() {
        let ports = find_available_ports().unwrap();
        assert!(ports.http_port > 0);
        assert!(ports.tcp_port > 0);
        assert_ne!(ports.http_port, ports.tcp_port);
    }

    #[test]
    fn test_find_one_port_returns_nonzero() {
        let port = find_one_port().unwrap();
        assert!(port > 0);
    }
}
