use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::*;
use ntpprobe::{query_ntp_server, OutputFormat, ProbeResult};
use std::process;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "ntpprobe",
    about = "Microsecond NTP/SNTP clock drift, stratum hierarchy, and jitter auditor",
    version
)]
pub struct Cli {
    /// NTP port
    #[arg(short = 'p', long, default_value_t = 123, global = true)]
    pub port: u16,

    /// Request timeout in milliseconds
    #[arg(short = 't', long, default_value_t = 3000, global = true)]
    pub timeout: u64,

    /// Output format: pretty, json, raw
    #[arg(short = 'f', long, default_value = "pretty", global = true)]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Probe a single NTP server and display detailed metrics
    Probe {
        /// Server hostname or IP address (e.g. pool.ntp.org, time.cloudflare.com)
        server: String,
    },

    /// Concurrently audit multiple NTP servers and detect drift outliers
    Audit {
        /// List of NTP servers to probe
        #[arg(required = true)]
        servers: Vec<String>,

        /// Max allowed offset drift in milliseconds before exit code 1
        #[arg(long)]
        max_offset_ms: Option<f64>,
    },

    /// Continuously monitor NTP drift over time
    Monitor {
        /// Server hostname or IP address
        server: String,

        /// Polling interval in seconds
        #[arg(short = 'i', long, default_value_t = 5)]
        interval: u64,

        /// Number of probes to perform (default: infinite)
        #[arg(short = 'c', long)]
        count: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let timeout = Duration::from_millis(cli.timeout);

    match cli.command {
        Commands::Probe { server } => {
            match query_ntp_server(&server, cli.port, timeout).await {
                Ok(res) => {
                    println!("{}", res.format(cli.format));
                }
                Err(e) => {
                    eprintln!("{} Failed to probe {}: {}", "error:".bold().red(), server, e);
                    process::exit(1);
                }
            }
        }
        Commands::Audit {
            servers,
            max_offset_ms,
        } => {
            let mut tasks = Vec::new();
            for s in servers {
                let port = cli.port;
                let s_clone = s.clone();
                tasks.push(tokio::spawn(async move {
                    let res = query_ntp_server(&s_clone, port, timeout).await;
                    (s_clone, res)
                }));
            }

            let mut results: Vec<(String, Result<ProbeResult, String>)> = Vec::new();
            for t in tasks {
                match t.await {
                    Ok((s, Ok(res))) => results.push((s, Ok(res))),
                    Ok((s, Err(e))) => results.push((s, Err(e.to_string()))),
                    Err(e) => results.push(("join_error".into(), Err(e.to_string()))),
                }
            }

            let mut has_drift_violation = false;

            match cli.format {
                OutputFormat::Json => {
                    let json_results: Vec<serde_json::Value> = results
                        .iter()
                        .map(|(s, r)| match r {
                            Ok(res) => serde_json::to_value(res).unwrap_or_default(),
                            Err(e) => serde_json::json!({
                                "server": s,
                                "error": e,
                            }),
                        })
                        .collect();
                    println!("{}", serde_json::to_string(&json_results).unwrap_or_default());
                }
                OutputFormat::Raw => {
                    for (s, r) in &results {
                        match r {
                            Ok(res) => println!("{}: {}", s, res.format(OutputFormat::Raw)),
                            Err(e) => println!("{}: ERROR {}", s, e),
                        }
                    }
                }
                OutputFormat::Pretty => {
                    println!(
                        "{:<28} {:<18} {:<8} {:<8} {:>12} {:>10}",
                        "SERVER".bold(),
                        "ADDRESS".bold(),
                        "STRATUM".bold(),
                        "REF ID".bold(),
                        "OFFSET".bold(),
                        "RTT".bold()
                    );
                    println!("{}", "-".repeat(90).dimmed());

                    for (s, r) in &results {
                        match r {
                            Ok(res) => {
                                let offset_str = format!("{:+8.3} ms", res.offset_ms);
                                let colored_offset = if res.offset_ms.abs() < 5.0 {
                                    offset_str.green()
                                } else if res.offset_ms.abs() < 50.0 {
                                    offset_str.yellow()
                                } else {
                                    offset_str.red().bold()
                                };

                                println!(
                                    "{:<28} {:<18} {:<8} {:<8} {:>12} {:>10.3} ms",
                                    s.cyan(),
                                    res.resolved_addr.dimmed(),
                                    res.stratum,
                                    res.reference_id,
                                    colored_offset,
                                    res.round_trip_delay_ms
                                );

                                if let Some(max_offset) = max_offset_ms {
                                    if res.offset_ms.abs() > max_offset {
                                        has_drift_violation = true;
                                    }
                                }
                            }
                            Err(e) => {
                                println!("{:<28} {}", s.red(), format!("FAILED: {}", e).dimmed());
                            }
                        }
                    }
                }
            }

            if has_drift_violation {
                if cli.format == OutputFormat::Pretty {
                    eprintln!(
                        "\n{} Clock offset drift exceeded threshold ({} ms)",
                        "error:".bold().red(),
                        max_offset_ms.unwrap_or(0.0)
                    );
                }
                process::exit(1);
            }
        }
        Commands::Monitor {
            server,
            interval,
            count,
        } => {
            let mut iteration = 0usize;
            let mut ticker = tokio::time::interval(Duration::from_secs(interval));

            loop {
                ticker.tick().await;
                match query_ntp_server(&server, cli.port, timeout).await {
                    Ok(res) => {
                        println!("{}", res.format(cli.format));
                    }
                    Err(e) => {
                        eprintln!("{} Failed probe: {}", "warn:".bold().yellow(), e);
                    }
                }

                iteration += 1;
                if let Some(c) = count {
                    if iteration >= c {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
