use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const NTP_TO_UNIX_SECONDS: u64 = 2_208_988_800;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NtpTimestamp {
    pub seconds: u32,
    pub fraction: u32,
}

impl NtpTimestamp {
    pub const ZERO: Self = Self {
        seconds: 0,
        fraction: 0,
    };

    pub fn now() -> Self {
        Self::from_system_time(SystemTime::now())
    }

    pub fn from_system_time(time: SystemTime) -> Self {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                let unix_secs = duration.as_secs();
                let ntp_secs = (unix_secs + NTP_TO_UNIX_SECONDS) as u32;
                let nanos = duration.subsec_nanos() as u64;
                // fraction = (nanos * 2^32) / 10^9
                let fraction = ((nanos << 32) / 1_000_000_000) as u32;
                Self {
                    seconds: ntp_secs,
                    fraction,
                }
            }
            Err(_) => Self::ZERO,
        }
    }

    pub fn to_system_time(&self) -> Option<SystemTime> {
        if self.seconds < (NTP_TO_UNIX_SECONDS as u32) {
            return None;
        }
        let unix_secs = (self.seconds as u64) - NTP_TO_UNIX_SECONDS;
        let nanos = ((self.fraction as u64 * 1_000_000_000) >> 32) as u32;
        Some(UNIX_EPOCH + Duration::new(unix_secs, nanos))
    }

    pub fn to_float_seconds(&self) -> f64 {
        (self.seconds as f64) + ((self.fraction as f64) / 4_294_967_296.0)
    }

    pub fn from_float_seconds(val: f64) -> Self {
        let secs = val.floor() as u32;
        let frac = ((val - (secs as f64)) * 4_294_967_296.0).round() as u32;
        Self {
            seconds: secs,
            fraction: frac,
        }
    }

    pub fn to_bytes(&self) -> [u8; 8] {
        let mut b = [0u8; 8];
        b[0..4].copy_from_slice(&self.seconds.to_be_bytes());
        b[4..8].copy_from_slice(&self.fraction.to_be_bytes());
        b
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let seconds = u32::from_be_bytes(bytes[0..4].try_into().unwrap_or_default());
        let fraction = u32::from_be_bytes(bytes[4..8].try_into().unwrap_or_default());
        Self { seconds, fraction }
    }
}

impl fmt::Display for NtpTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(st) = self.to_system_time() {
            let dt: chrono::DateTime<chrono::Utc> = st.into();
            write!(f, "{}", dt.to_rfc3339())
        } else {
            write!(f, "{}.{:08x}", self.seconds, self.fraction)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LeapIndicator {
    NoWarning = 0,
    LastMinute61 = 1,
    LastMinute59 = 2,
    AlarmNotSynchronized = 3,
}

impl LeapIndicator {
    pub fn from_code(code: u8) -> Self {
        match code & 0x03 {
            0 => LeapIndicator::NoWarning,
            1 => LeapIndicator::LastMinute61,
            2 => LeapIndicator::LastMinute59,
            _ => LeapIndicator::AlarmNotSynchronized,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            LeapIndicator::NoWarning => "No warning (synchronized)",
            LeapIndicator::LastMinute61 => "Last minute has 61 seconds (+1 leap)",
            LeapIndicator::LastMinute59 => "Last minute has 59 seconds (-1 leap)",
            LeapIndicator::AlarmNotSynchronized => "Clock not synchronized (alarm)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NtpMode {
    Reserved = 0,
    SymmetricActive = 1,
    SymmetricPassive = 2,
    Client = 3,
    Server = 4,
    Broadcast = 5,
    ControlMessage = 6,
    Private = 7,
}

impl NtpMode {
    pub fn from_code(code: u8) -> Self {
        match code & 0x07 {
            0 => NtpMode::Reserved,
            1 => NtpMode::SymmetricActive,
            2 => NtpMode::SymmetricPassive,
            3 => NtpMode::Client,
            4 => NtpMode::Server,
            5 => NtpMode::Broadcast,
            6 => NtpMode::ControlMessage,
            _ => NtpMode::Private,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NtpPacket {
    pub leap_indicator: LeapIndicator,
    pub version: u8,
    pub mode: NtpMode,
    pub stratum: u8,
    pub poll: i8,
    pub precision: i8,
    pub root_delay: f64,       // seconds
    pub root_dispersion: f64,  // seconds
    pub reference_id: String,
    pub ref_timestamp: NtpTimestamp,
    pub origin_timestamp: NtpTimestamp,     // T1
    pub receive_timestamp: NtpTimestamp,    // T2
    pub transmit_timestamp: NtpTimestamp,   // T3
}

impl NtpPacket {
    pub fn new_client_request(t1: NtpTimestamp) -> Self {
        Self {
            leap_indicator: LeapIndicator::NoWarning,
            version: 4,
            mode: NtpMode::Client,
            stratum: 0,
            poll: 0,
            precision: 0,
            root_delay: 0.0,
            root_dispersion: 0.0,
            reference_id: String::new(),
            ref_timestamp: NtpTimestamp::ZERO,
            origin_timestamp: NtpTimestamp::ZERO,
            receive_timestamp: NtpTimestamp::ZERO,
            transmit_timestamp: t1,
        }
    }

    pub fn to_bytes(&self) -> [u8; 48] {
        let mut buf = [0u8; 48];
        let li = (self.leap_indicator as u8) << 6;
        let vn = (self.version & 0x07) << 3;
        let mode = (self.mode as u8) & 0x07;
        buf[0] = li | vn | mode;
        buf[1] = self.stratum;
        buf[2] = self.poll as u8;
        buf[3] = self.precision as u8;

        // Root Delay: 16-bit signed int, 16-bit unsigned fraction
        let rd_scaled = (self.root_delay * 65536.0).round() as i32;
        buf[4..8].copy_from_slice(&rd_scaled.to_be_bytes());

        // Root Dispersion: 16-bit unsigned int, 16-bit unsigned fraction
        let rdisp_scaled = (self.root_dispersion * 65536.0).round() as u32;
        buf[8..12].copy_from_slice(&rdisp_scaled.to_be_bytes());

        // Reference ID (4 bytes)
        if let Ok(ip) = self.reference_id.parse::<std::net::Ipv4Addr>() {
            buf[12..16].copy_from_slice(&ip.octets());
        } else {
            let ref_bytes = self.reference_id.as_bytes();
            let copy_len = ref_bytes.len().min(4);
            buf[12..12 + copy_len].copy_from_slice(&ref_bytes[..copy_len]);
        }

        // Timestamps
        buf[16..24].copy_from_slice(&self.ref_timestamp.to_bytes());
        buf[24..32].copy_from_slice(&self.origin_timestamp.to_bytes());
        buf[32..40].copy_from_slice(&self.receive_timestamp.to_bytes());
        buf[40..48].copy_from_slice(&self.transmit_timestamp.to_bytes());

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 48 {
            return Err(anyhow!(
                "NTP packet too short: expected at least 48 bytes, got {}",
                bytes.len()
            ));
        }

        let first = bytes[0];
        let leap_indicator = LeapIndicator::from_code(first >> 6);
        let version = (first >> 3) & 0x07;
        let mode = NtpMode::from_code(first & 0x07);

        let stratum = bytes[1];
        let poll = bytes[2] as i8;
        let precision = bytes[3] as i8;

        let rd_raw = i32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let root_delay = (rd_raw as f64) / 65536.0;

        let rdisp_raw = u32::from_be_bytes(bytes[8..12].try_into().unwrap());
        let root_dispersion = (rdisp_raw as f64) / 65536.0;

        let ref_bytes = &bytes[12..16];
        let reference_id = if stratum <= 1 {
            // ASCII string (e.g. GPS, PPS, ATOM)
            let s = std::str::from_utf8(ref_bytes)
                .unwrap_or("????")
                .trim_matches('\0')
                .trim();
            s.to_string()
        } else {
            // IPv4 address
            format!(
                "{}.{}.{}.{}",
                ref_bytes[0], ref_bytes[1], ref_bytes[2], ref_bytes[3]
            )
        };

        let ref_timestamp = NtpTimestamp::from_bytes(&bytes[16..24]);
        let origin_timestamp = NtpTimestamp::from_bytes(&bytes[24..32]);
        let receive_timestamp = NtpTimestamp::from_bytes(&bytes[32..40]);
        let transmit_timestamp = NtpTimestamp::from_bytes(&bytes[40..48]);

        Ok(Self {
            leap_indicator,
            version,
            mode,
            stratum,
            poll,
            precision,
            root_delay,
            root_dispersion,
            reference_id,
            ref_timestamp,
            origin_timestamp,
            receive_timestamp,
            transmit_timestamp,
        })
    }
}

/// Calculate RFC 5905 clock offset (delta) and round-trip delay (theta)
/// All return values in seconds (can be negative for offset)
pub fn calculate_ntp_metrics(
    t1: NtpTimestamp, // Client sent
    t2: NtpTimestamp, // Server received
    t3: NtpTimestamp, // Server sent
    t4: NtpTimestamp, // Client received
) -> (f64, f64) {
    let t1_f = t1.to_float_seconds();
    let t2_f = t2.to_float_seconds();
    let t3_f = t3.to_float_seconds();
    let t4_f = t4.to_float_seconds();

    // Delay = (T4 - T1) - (T3 - T2)
    let delay = (t4_f - t1_f) - (t3_f - t2_f);
    // Offset = ((T2 - T1) + (T3 - T4)) / 2
    let offset = ((t2_f - t1_f) + (t3_f - t4_f)) / 2.0;

    (offset, delay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ntp_timestamp_roundtrip() {
        let now = SystemTime::now();
        let ts = NtpTimestamp::from_system_time(now);
        let back = ts.to_system_time().unwrap();
        let diff = if back > now {
            back.duration_since(now).unwrap()
        } else {
            now.duration_since(back).unwrap()
        };
        // Should be within a microsecond
        assert!(diff.as_micros() <= 1);
    }

    #[test]
    fn test_packet_encode_decode() {
        let t1 = NtpTimestamp {
            seconds: 3900000000,
            fraction: 12345678,
        };
        let req = NtpPacket::new_client_request(t1);
        let bytes = req.to_bytes();
        assert_eq!(bytes.len(), 48);

        let decoded = NtpPacket::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.version, 4);
        assert_eq!(decoded.mode, NtpMode::Client);
        assert_eq!(decoded.transmit_timestamp, t1);
    }

    #[test]
    fn test_calculate_metrics() {
        // Assume server clock is exactly 0.05s ahead of client clock.
        // T1 = 1000.00
        // Network delay outbound = 0.02s
        // T2 = 1000.02 + 0.05 = 1000.07
        // Processing time = 0.001s
        // T3 = 1000.071
        // Network delay inbound = 0.02s
        // T4 = 1000.00 + 0.02 + 0.001 + 0.02 = 1000.041
        let t1 = NtpTimestamp::from_float_seconds(1000.00);
        let t2 = NtpTimestamp::from_float_seconds(1000.07);
        let t3 = NtpTimestamp::from_float_seconds(1000.071);
        let t4 = NtpTimestamp::from_float_seconds(1000.041);

        let (offset, delay) = calculate_ntp_metrics(t1, t2, t3, t4);
        // Delay should be (1000.041 - 1000.00) - (1000.071 - 1000.07) = 0.041 - 0.001 = 0.040s (40ms)
        assert!((delay - 0.040).abs() < 1e-6);
        // Offset should be ((1000.07 - 1000.00) + (1000.071 - 1000.041)) / 2 = (0.07 + 0.03) / 2 = 0.050s (+50ms)
        assert!((offset - 0.050).abs() < 1e-6);
    }
}
