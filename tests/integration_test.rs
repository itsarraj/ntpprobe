use ntpprobe::packet::{LeapIndicator, NtpMode, NtpPacket, NtpTimestamp};
use ntpprobe::{query_ntp_server, OutputFormat};
use std::time::Duration;
use tokio::net::UdpSocket;

#[tokio::test]
async fn test_mock_ntp_stratum1_probe() {
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let (n, peer) = server_socket.recv_from(&mut buf).await.unwrap();
        let t2 = NtpTimestamp::now();

        let req = NtpPacket::from_bytes(&buf[..n]).unwrap();
        assert_eq!(req.mode, NtpMode::Client);
        assert_eq!(req.version, 4);

        tokio::time::sleep(Duration::from_millis(5)).await;
        let t3 = NtpTimestamp::now();

        let resp = NtpPacket {
            leap_indicator: LeapIndicator::NoWarning,
            version: 4,
            mode: NtpMode::Server,
            stratum: 1,
            poll: 4,
            precision: -18,
            root_delay: 0.0005,
            root_dispersion: 0.001,
            reference_id: "GPS".into(),
            ref_timestamp: t2,
            origin_timestamp: req.transmit_timestamp,
            receive_timestamp: t2,
            transmit_timestamp: t3,
        };

        server_socket.send_to(&resp.to_bytes(), peer).await.unwrap();
    });

    let res = query_ntp_server(
        &server_addr.ip().to_string(),
        server_addr.port(),
        Duration::from_millis(1000),
    )
    .await
    .unwrap();

    assert_eq!(res.stratum, 1);
    assert_eq!(res.reference_id, "GPS");
    assert!(res.leap_indicator.contains("No warning"));
    assert!(res.round_trip_delay_ms >= 0.0);
    assert_eq!(res.poll, 4);

    let pretty = res.format(OutputFormat::Pretty);
    assert!(pretty.contains("GPS"));
    assert!(pretty.contains("1 (Primary/Ref)"));

    server_task.await.unwrap();
}

#[tokio::test]
async fn test_mock_ntp_stratum2_ipv4_refid() {
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let (n, peer) = server_socket.recv_from(&mut buf).await.unwrap();
        let t2 = NtpTimestamp::now();

        let req = NtpPacket::from_bytes(&buf[..n]).unwrap();
        let t3 = NtpTimestamp::now();

        let resp = NtpPacket {
            leap_indicator: LeapIndicator::NoWarning,
            version: 4,
            mode: NtpMode::Server,
            stratum: 2,
            poll: 6,
            precision: -16,
            root_delay: 0.012,
            root_dispersion: 0.005,
            reference_id: "192.168.1.1".into(),
            ref_timestamp: t2,
            origin_timestamp: req.transmit_timestamp,
            receive_timestamp: t2,
            transmit_timestamp: t3,
        };

        server_socket.send_to(&resp.to_bytes(), peer).await.unwrap();
    });

    let res = query_ntp_server(
        &server_addr.ip().to_string(),
        server_addr.port(),
        Duration::from_millis(1000),
    )
    .await
    .unwrap();

    assert_eq!(res.stratum, 2);
    assert_eq!(res.reference_id, "192.168.1.1");
    assert!(res.root_delay_ms > 10.0);

    server_task.await.unwrap();
}

#[tokio::test]
async fn test_mock_ntp_leap_alarm() {
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_socket.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let (n, peer) = server_socket.recv_from(&mut buf).await.unwrap();
        let t2 = NtpTimestamp::now();
        let req = NtpPacket::from_bytes(&buf[..n]).unwrap();
        let t3 = NtpTimestamp::now();

        let resp = NtpPacket {
            leap_indicator: LeapIndicator::AlarmNotSynchronized,
            version: 4,
            mode: NtpMode::Server,
            stratum: 16,
            poll: 0,
            precision: 0,
            root_delay: 0.0,
            root_dispersion: 1.0,
            reference_id: "INIT".into(),
            ref_timestamp: NtpTimestamp::ZERO,
            origin_timestamp: req.transmit_timestamp,
            receive_timestamp: t2,
            transmit_timestamp: t3,
        };

        server_socket.send_to(&resp.to_bytes(), peer).await.unwrap();
    });

    let res = query_ntp_server(
        &server_addr.ip().to_string(),
        server_addr.port(),
        Duration::from_millis(1000),
    )
    .await
    .unwrap();

    assert_eq!(res.stratum, 16);
    assert!(res.leap_indicator.contains("not synchronized"));

    server_task.await.unwrap();
}

#[tokio::test]
async fn test_concurrent_multi_server_audit() {
    // Spin up two servers
    let s1 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let s2 = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let addr1 = s1.local_addr().unwrap();
    let addr2 = s2.local_addr().unwrap();

    let t1 = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let (n, peer) = s1.recv_from(&mut buf).await.unwrap();
        let req = NtpPacket::from_bytes(&buf[..n]).unwrap();
        let now = NtpTimestamp::now();
        let resp = NtpPacket {
            leap_indicator: LeapIndicator::NoWarning,
            version: 4,
            mode: NtpMode::Server,
            stratum: 1,
            poll: 4,
            precision: -20,
            root_delay: 0.0,
            root_dispersion: 0.0001,
            reference_id: "ATOM".into(),
            ref_timestamp: now,
            origin_timestamp: req.transmit_timestamp,
            receive_timestamp: now,
            transmit_timestamp: now,
        };
        s1.send_to(&resp.to_bytes(), peer).await.unwrap();
    });

    let t2 = tokio::spawn(async move {
        let mut buf = [0u8; 128];
        let (n, peer) = s2.recv_from(&mut buf).await.unwrap();
        let req = NtpPacket::from_bytes(&buf[..n]).unwrap();
        let now = NtpTimestamp::now();
        let resp = NtpPacket {
            leap_indicator: LeapIndicator::NoWarning,
            version: 4,
            mode: NtpMode::Server,
            stratum: 2,
            poll: 4,
            precision: -18,
            root_delay: 0.005,
            root_dispersion: 0.001,
            reference_id: "10.0.0.1".into(),
            ref_timestamp: now,
            origin_timestamp: req.transmit_timestamp,
            receive_timestamp: now,
            transmit_timestamp: now,
        };
        s2.send_to(&resp.to_bytes(), peer).await.unwrap();
    });

    let r1 = query_ntp_server(
        &addr1.ip().to_string(),
        addr1.port(),
        Duration::from_millis(1000),
    )
    .await
    .unwrap();

    let r2 = query_ntp_server(
        &addr2.ip().to_string(),
        addr2.port(),
        Duration::from_millis(1000),
    )
    .await
    .unwrap();

    assert_eq!(r1.reference_id, "ATOM");
    assert_eq!(r2.reference_id, "10.0.0.1");

    t1.await.unwrap();
    t2.await.unwrap();
}
