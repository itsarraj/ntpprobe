# ntpprobe

A lightweight, zero-dependency CLI tool for microsecond NTP/SNTP clock drift measurement, stratum hierarchy inspection, jitter auditing, and distributed cluster time preflight checks.

Status: Verified against mock RFC 5905 NTPv4 socket servers and live UDP endpoints; validates Stratum 1 (GPS/ATOM reference clocks), Stratum 2 (IPv4 upstream clocks), RFC 5905 round-trip delay and offset calculations, anti-spoofing origin timestamps, and concurrent multi-server drift thresholds (`--max-offset-ms`). NTS (Network Time Security / RFC 8915) TLS key exchange deferred to `ntpprobe` v0.2.

---

## Features

- **Pure Rust Wire Codec**: Direct binary RFC 5905 (NTPv4) and RFC 4330 (SNTPv4) packet encoder and parser without `libntp`, C bindings, or OS daemon dependencies.
- **Microsecond Precision Metrics**:
  - Round-trip network delay: $\theta = (T_4 - T_1) - (T_3 - T_2)$
  - Clock offset: $\delta = \frac{(T_2 - T_1) + (T_3 - T_4)}{2}$
  - Root delay and root dispersion calculation.
- **Stratum & Reference Clock Decoding**: Automatically parses Stratum 1 reference IDs (`GPS`, `PPS`, `ATOM`, `ACTS`) or Stratum 2+ IPv4 clock addresses.
- **Leap Second & Alarm Detection**: Decodes RFC 5905 Leap Indicator (LI) flags, warning on leap second insertions, deletions, or unsynchronized clock states.
- **Concurrent Multi-Server Auditing**: Concurrently queries multiple NTP endpoints (e.g. Cloudflare, Google, pool.ntp.org), tabulates offsets, and enforces maximum allowable drift limits (`--max-offset-ms`).
- **Continuous Drift Monitoring**: Real-time periodic monitoring mode with configurable interval and sample counts.
- **Multiple Output Formats**:
  - `pretty`: Color-coded human console tables with offset severity thresholds.
  - `json` / `ndjson`: Machine-readable telemetry format for Prometheus exporters, Vector, Datadog, or CI/CD pipelines.
  - `raw`: Minimal shell-scriptable format.

---

## Installation

```bash
cargo install --path .
```

Or build the release binary directly:

```bash
cargo build --release
cp target/release/ntpprobe /usr/local/bin/
```

---

## Usage

### 1. Probe a Single NTP Server

```bash
# Query Cloudflare's public NTP server
ntpprobe probe time.cloudflare.com

# Query local chrony or ntpd daemon
ntpprobe probe 127.0.0.1
```

Example Output:
```text
[2026-09-26T00:50:00Z] Server: time.cloudflare.com (162.159.200.1:123)
  * Stratum:      1 (Primary/Ref)
  * Reference ID: GOOG
  * Clock Offset:   -0.420 ms
  * Round Trip:   14.281 ms
  * Root Disp:    0.015 ms (delay: 0.000 ms)
  * Leap Status:  No warning (synchronized)
  * Server Time:  2026-09-26T00:50:00.123456Z
```

### 2. Multi-Server Drift Audit (Cluster Preflight)

```bash
# Concurrently probe multiple upstream clocks and fail if drift exceeds 20ms
ntpprobe audit time.cloudflare.com time.google.com time.apple.com pool.ntp.org --max-offset-ms 20
```

### 3. Continuous Monitoring with NDJSON

```bash
# Monitor time offset every 5 seconds, piping structured metrics to jq
ntpprobe -f json monitor time.cloudflare.com -i 5 | jq '{time: .server_time, offset: .offset_ms}'
```

---

## Command Line Options

```text
Usage: ntpprobe [OPTIONS] <COMMAND>

Commands:
  probe    Probe a single NTP server and display detailed metrics
  audit    Concurrently audit multiple NTP servers and detect drift outliers
  monitor  Continuously monitor NTP drift over time
  help     Print this message or the help of the given subcommand(s)

Options:
  -p, --port <PORT>        NTP port [default: 123]
  -t, --timeout <TIMEOUT>  Request timeout in milliseconds [default: 3000]
  -f, --format <FORMAT>    Output format: pretty, json, raw [default: pretty]
  -h, --help               Print help
  -V, --version            Print version
```

---

## License

MIT (c) 2026 Animesh Raj. See [LICENSE](LICENSE) for details.
