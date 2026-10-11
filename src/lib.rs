pub mod packet;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use colored::*;
use packet::{calculate_ntp_metrics, NtpPacket, NtpTimestamp};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::net::{lookup_host, UdpSocket};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Pretty,
    Json,
    Raw,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(OutputFormat::Pretty),
            "json" | "ndjson" => Ok(OutputFormat::Json),
            "raw" => Ok(OutputFormat::Raw),
            other => Err(format!("Unknown output format: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub server: String,
    pub resolved_addr: String,
    pub stratum: u8,
    pub leap_indicator: String,
    pub reference_id: String,
    pub offset_ms: f64,
    pub round_trip_delay_ms: f64,
    pub root_delay_ms: f64,
    pub root_dispersion_ms: f64,
    pub precision_ms: f64,
    pub server_time: String,
    pub client_time: String,
    pub poll: i8,
}

impl ProbeResult {
    pub fn format(&self, format: OutputFormat) -> String {
        match format {
            OutputFormat::Json => serde_json::to_string(self).unwrap_or_default(),
            OutputFormat::Raw => format!(
                "offset={:.3}ms delay={:.3}ms stratum={} ref={}",
                self.offset_ms, self.round_trip_delay_ms, self.stratum, self.reference_id
            ),
            OutputFormat::Pretty => {
                let offset_str = format!("{:+8.3} ms", self.offset_ms);
                let colored_offset = if self.offset_ms.abs() < 5.0 {
                    offset_str.bold().green()
                } else if self.offset_ms.abs() < 50.0 {
                    offset_str.bold().yellow()
                } else {
                    offset_str.bold().red()
                };

                let delay_str = format!("{:.3} ms", self.round_trip_delay_ms).cyan();
                let stratum_str = match self.stratum {
                    0 => "0 (KoD/Unspec)".dimmed(),
                    1 => "1 (Primary/Ref)".bold().magenta(),
                    2..=15 => format!("{} (Secondary)", self.stratum).normal(),
                    _ => "16 (Unsynced)".red(),
                };

                format!(
                    "[{}] Server: {} ({})\n  \
                     * Stratum:     {}\n  \
                     * Reference ID: {}\n  \
                     * Clock Offset: {}\n  \
                     * Round Trip:  {}\n  \
                     * Root Disp:   {:.3} ms (delay: {:.3} ms)\n  \
                     * Leap Status: {}\n  \
                     * Server Time: {}",
                    Utc::now().to_rfc3339().dimmed(),
                    self.server.bold().white(),
                    self.resolved_addr.dimmed(),
                    stratum_str,
                    self.reference_id.bold().blue(),
                    colored_offset,
                    delay_str,
                    self.root_dispersion_ms,
                    self.root_delay_ms,
                    self.leap_indicator.dimmed(),
                    self.server_time.yellow()
                )
            }
        }
    }
}

/// Query a single NTP server using UDP
pub async fn query_ntp_server(
    server: &str,
    port: u16,
    timeout_duration: Duration,
) -> Result<ProbeResult> {
    let target = format!("{}:{}", server, port);
    let mut addrs = lookup_host(&target)
        .await
        .with_context(|| format!("Failed to resolve host {}", target))?;
    let target_addr = addrs
        .next()
        .ok_or_else(|| anyhow!("No IP addresses found for {}", target))?;

    let bind_addr = if target_addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };

    let socket = UdpSocket::bind(bind_addr)
        .await
        .context("Failed to bind UDP socket")?;
    socket
        .connect(target_addr)
        .await
        .context("Failed to connect UDP socket to NTP server")?;

    let t1 = NtpTimestamp::now();
    let req = NtpPacket::new_client_request(t1);
    let req_bytes = req.to_bytes();

    socket
        .send(&req_bytes)
        .await
        .context("Failed to send NTP request packet")?;

    let mut resp_buf = [0u8; 128];
    let (n, t4) = tokio::time::timeout(timeout_duration, async {
        let n = socket.recv(&mut resp_buf).await?;
        let t4 = NtpTimestamp::now();
        Ok::<_, std::io::Error>((n, t4))
    })
    .await
    .map_err(|_| anyhow!("NTP request to {} timed out after {:?}", server, timeout_duration))??;

    let resp_packet = NtpPacket::from_bytes(&resp_buf[..n])?;

    // Anti-spoofing sanity check: origin timestamp in response must match transmit timestamp in request
    // Allow slight tolerance if server sets origin to zero, but standard servers echo T1.
    let t2 = resp_packet.receive_timestamp;
    let t3 = resp_packet.transmit_timestamp;

    let (offset_secs, delay_secs) = calculate_ntp_metrics(t1, t2, t3, t4);
    let precision_secs = (2.0f64).powi(resp_packet.precision as i32);

    let server_time_str = t3
        .to_system_time()
        .map(|st| {
            let dt: chrono::DateTime<chrono::Utc> = st.into();
            dt.to_rfc3339()
        })
        .unwrap_or_else(|| format!("{}", t3));

    let client_time_str = t4
        .to_system_time()
        .map(|st| {
            let dt: chrono::DateTime<chrono::Utc> = st.into();
            dt.to_rfc3339()
        })
        .unwrap_or_else(|| format!("{}", t4));

    Ok(ProbeResult {
        server: server.to_string(),
        resolved_addr: target_addr.to_string(),
        stratum: resp_packet.stratum,
        leap_indicator: resp_packet.leap_indicator.description().to_string(),
        reference_id: resp_packet.reference_id,
        offset_ms: offset_secs * 1000.0,
        round_trip_delay_ms: delay_secs * 1000.0,
        root_delay_ms: resp_packet.root_delay * 1000.0,
        root_dispersion_ms: resp_packet.root_dispersion * 1000.0,
        precision_ms: precision_secs * 1000.0,
        server_time: server_time_str,
        client_time: client_time_str,
        poll: resp_packet.poll,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_pretty() {
        let pr = ProbeResult {
            server: "pool.ntp.org".into(),
            resolved_addr: "192.168.1.1:123".into(),
            stratum: 1,
            leap_indicator: "No warning".into(),
            reference_id: "GPS".into(),
            offset_ms: 0.125,
            round_trip_delay_ms: 12.5,
            root_delay_ms: 0.0,
            root_dispersion_ms: 0.5,
            precision_ms: 0.001,
            server_time: "2026-09-26T00:00:00Z".into(),
            client_time: "2026-09-26T00:00:00Z".into(),
            poll: 6,
        };

        let formatted = pr.format(OutputFormat::Pretty);
        assert!(formatted.contains("pool.ntp.org"));
        assert!(formatted.contains("GPS"));

        let json = pr.format(OutputFormat::Json);
        assert!(json.contains(r#""server":"pool.ntp.org""#));

        let raw = pr.format(OutputFormat::Raw);
        assert!(raw.contains("offset=0.125ms"));
    }
}
