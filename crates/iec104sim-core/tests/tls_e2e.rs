use iec104sim_core::data_point::DataPointValue;
use iec104sim_core::master::{MasterConfig, MasterConnection, TlsConfig};
use iec104sim_core::slave::{SlaveServer, SlaveTlsConfig, SlaveTransportConfig, Station};
use iec104sim_core::types::AsduTypeId;
use std::process::Command;
use tokio::time::{sleep, Duration};

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

// =========================================================================
// Tool availability check
// =========================================================================

fn check_tools_available() -> bool {
    let tcpdump_ok = Command::new("tcpdump").arg("--version").output().is_ok();
    let tshark_ok = Command::new("tshark").arg("--version").output().is_ok();
    if !tcpdump_ok {
        eprintln!("SKIP: tcpdump not found in PATH");
    }
    if !tshark_ok {
        eprintln!("SKIP: tshark not found in PATH. Install with: brew install wireshark");
    }
    tcpdump_ok && tshark_ok
}

// =========================================================================
// Module: cert_gen — Dynamic certificate generation with rcgen
// =========================================================================

mod cert_gen {
    use std::path::{Path, PathBuf};

    pub struct TestCerts {
        pub ca_cert_pem: String,
        pub server_cert_pem: String,
        pub server_key_pem: String,
        pub client_cert_pem: String,
        pub client_key_pem: String,
    }

    pub struct CertPaths {
        pub ca_cert: PathBuf,
        pub server_cert: PathBuf,
        pub server_key: PathBuf,
        pub server_pkcs12: PathBuf,
        pub client_cert: PathBuf,
        pub client_key: PathBuf,
        pub client_pkcs12: PathBuf,
    }

    /// PKCS#12 password used for all generated identities in tests.
    pub const PKCS12_PASS: &str = "iec104test";

    /// Generate a full certificate chain: CA -> Server + Client.
    pub fn generate() -> TestCerts {
        use rcgen::{
            CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, BasicConstraints,
            KeyUsagePurpose, SanType, KeyPair,
        };

        // --- CA certificate ---
        let mut ca_params = CertificateParams::new(vec!["IEC104 Test CA".to_string()]).unwrap();
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        ca_params.distinguished_name.push(DnType::CommonName, "IEC104 Test CA");
        let ca_key = KeyPair::generate().unwrap();
        let ca_cert = ca_params.self_signed(&ca_key).unwrap();

        // --- Server certificate ---
        let mut server_params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        server_params.subject_alt_names = vec![
            SanType::DnsName("localhost".try_into().unwrap()),
            SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        ];
        server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        server_params.distinguished_name.push(DnType::CommonName, "IEC104 Test Server");
        let server_key = KeyPair::generate().unwrap();
        let server_cert = server_params.signed_by(&server_key, &ca_cert, &ca_key).unwrap();

        // --- Client certificate ---
        let mut client_params = CertificateParams::new(vec!["IEC104 Test Client".to_string()]).unwrap();
        client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        client_params.distinguished_name.push(DnType::CommonName, "IEC104 Test Client");
        let client_key = KeyPair::generate().unwrap();
        let client_cert = client_params.signed_by(&client_key, &ca_cert, &ca_key).unwrap();

        TestCerts {
            ca_cert_pem: ca_cert.pem(),
            server_cert_pem: server_cert.pem(),
            server_key_pem: server_key.serialize_pem(),
            client_cert_pem: client_cert.pem(),
            client_key_pem: client_key.serialize_pem(),
        }
    }

    /// Write all PEM files to the given directory and generate PKCS#12 bundles.
    pub fn write_to_dir(certs: &TestCerts, dir: &Path) -> CertPaths {
        let paths = CertPaths {
            ca_cert: dir.join("ca.pem"),
            server_cert: dir.join("server.pem"),
            server_key: dir.join("server-key.pem"),
            server_pkcs12: dir.join("server.p12"),
            client_cert: dir.join("client.pem"),
            client_key: dir.join("client-key.pem"),
            client_pkcs12: dir.join("client.p12"),
        };
        std::fs::write(&paths.ca_cert, &certs.ca_cert_pem).unwrap();
        std::fs::write(&paths.server_cert, &certs.server_cert_pem).unwrap();
        std::fs::write(&paths.server_key, &certs.server_key_pem).unwrap();
        std::fs::write(&paths.client_cert, &certs.client_cert_pem).unwrap();
        std::fs::write(&paths.client_key, &certs.client_key_pem).unwrap();

        // Generate PKCS#12 bundles via openssl CLI (required on macOS with native-tls
        // because the Security framework cannot import ECDSA keys via from_pkcs8).
        make_pkcs12(
            &paths.server_cert, &paths.server_key, &paths.server_pkcs12, PKCS12_PASS,
        );
        make_pkcs12(
            &paths.client_cert, &paths.client_key, &paths.client_pkcs12, PKCS12_PASS,
        );

        paths
    }

    fn make_pkcs12(cert: &Path, key: &Path, out: &Path, password: &str) {
        // Try without -legacy first (modern PKCS#12, compatible with macOS Security framework).
        // OpenSSL 3.x defaults to AES-256-CBC which macOS Security.framework supports.
        let status = std::process::Command::new("openssl")
            .args([
                "pkcs12", "-export",
                "-in",      cert.to_str().unwrap(),
                "-inkey",   key.to_str().unwrap(),
                "-out",     out.to_str().unwrap(),
                "-passout", &format!("pass:{}", password),
            ])
            .status()
            .expect("openssl not found — required for PKCS#12 generation");
        assert!(status.success(), "openssl pkcs12 export failed");
    }
}

// =========================================================================
// Verification test: cert generation
// =========================================================================

#[test]
fn test_cert_generation() {
    let certs = cert_gen::generate();
    assert!(certs.ca_cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(certs.server_cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(certs.server_key_pem.contains("BEGIN PRIVATE KEY"));
    assert!(certs.client_cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(certs.client_key_pem.contains("BEGIN PRIVATE KEY"));

    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());
    assert!(paths.ca_cert.exists());
    assert!(paths.server_cert.exists());
    assert!(paths.server_key.exists());
    assert!(paths.server_pkcs12.exists());
    assert!(paths.client_cert.exists());
    assert!(paths.client_key.exists());
    assert!(paths.client_pkcs12.exists());
}

// =========================================================================
// Module: capture — Packet capture with tcpdump + analysis with tshark
// =========================================================================

mod capture {
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};

    pub struct PacketCapture {
        child: Child,
        pub pcap_path: PathBuf,
    }

    /// Start tcpdump capturing on loopback for the given port.
    pub fn start(test_name: &str, port: u16) -> Result<PacketCapture, String> {
        let pcap_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pcap");
        std::fs::create_dir_all(&pcap_dir).map_err(|e| format!("create pcap dir: {}", e))?;

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let pcap_path = pcap_dir.join(format!("{}_{}.pcap", test_name, timestamp));

        let child = Command::new("tcpdump")
            .args([
                "-i", "lo0",
                "-w", pcap_path.to_str().unwrap(),
                "-s", "0",
                &format!("port {}", port),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn tcpdump: {} (need BPF permissions — try: brew install --cask wireshark)", e))?;

        Ok(PacketCapture { child, pcap_path })
    }

    impl PacketCapture {
        /// Stop capturing. Sends SIGTERM and waits for tcpdump to flush.
        pub fn stop(&mut self) -> Result<(), String> {
            unsafe {
                libc::kill(self.child.id() as i32, libc::SIGTERM);
            }
            self.child.wait().map_err(|e| format!("wait tcpdump: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            Ok(())
        }
    }

    /// Assert that the pcap contains a valid TLS session:
    /// 1. TLS handshake present (ClientHello + ServerHello)
    /// 2. No plaintext IEC 104 frames visible
    /// 3. Encrypted application data present
    pub fn assert_tls_encrypted(pcap_path: &Path, port: u16) {
        let pcap = pcap_path.to_str().unwrap();

        // 1. Check TLS handshake exists
        let output = Command::new("tshark")
            .args(["-r", pcap, "-Y", "tls.handshake", "-T", "fields", "-e", "tls.handshake.type"])
            .output()
            .expect("failed to run tshark");
        let handshake_types = String::from_utf8_lossy(&output.stdout);
        assert!(
            handshake_types.contains("1"),
            "No ClientHello found in pcap: {}\ntshark output: {}",
            pcap, handshake_types
        );
        assert!(
            handshake_types.contains("2"),
            "No ServerHello found in pcap: {}\ntshark output: {}",
            pcap, handshake_types
        );

        // 2. Check no plaintext IEC 104 is visible
        let output = Command::new("tshark")
            .args(["-r", pcap, "-Y", "iec60870_104", "-T", "fields", "-e", "frame.number"])
            .output()
            .expect("failed to run tshark");
        let iec104_frames = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(
            iec104_frames.is_empty(),
            "Plaintext IEC 104 frames leaked through TLS! pcap: {}\nFrame numbers: {}",
            pcap, iec104_frames
        );

        // 3. Check encrypted application data exists
        let output = Command::new("tshark")
            .args([
                "-r", pcap,
                "-Y", &format!("tls.record.content_type == 23 && tcp.port == {}", port),
                "-T", "fields", "-e", "frame.number",
            ])
            .output()
            .expect("failed to run tshark");
        let app_data = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(
            !app_data.is_empty(),
            "No encrypted application data found in pcap: {}",
            pcap
        );

        eprintln!("  TLS assertions passed. pcap: {}", pcap);
    }
}

// =========================================================================
// Test: One-way TLS handshake (server auth only)
// =========================================================================
// multi_thread flavor needed: master.connect() does blocking TLS I/O inside
// the async context; without extra threads the slave's async accept() loop
// would be starved and the handshake would time out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_handshake_one_way() {
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    // Start slave with TLS enabled, no client cert required.
    // Use PKCS#12 for identity — native-tls on macOS cannot import ECDSA keys
    // via from_pkcs8 (Security framework limitation).
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: String::new(),
            key_file: String::new(),
            ca_file: String::new(),
            require_client_cert: false,
            pkcs12_file: paths.server_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "TLS Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    // Start packet capture
    let mut cap = capture::start("tls_handshake_one_way", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    // Connect master with TLS, trusting our CA
    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: false,
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    let connect_result = master.connect().await;
    assert!(connect_result.is_ok(), "TLS connection should succeed: {:?}", connect_result.err());
    sleep(Duration::from_millis(500)).await;

    // Disconnect and stop capture
    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    // Protocol assertions
    capture::assert_tls_encrypted(&cap.pcap_path, port);
}

// =========================================================================
// Test: Mutual TLS handshake (server + client auth)
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_handshake_mtls() {
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    // Slave with mTLS config (PKCS12 for identity)
    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: String::new(),
            key_file: String::new(),
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            require_client_cert: true,
            pkcs12_file: paths.server_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    slave.add_station(Station::with_default_points(1, "mTLS Test", 2)).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut cap = capture::start("tls_handshake_mtls", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    // Master with client cert via PKCS12 (macOS Security framework requires this for ECDSA)
    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: paths.client_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
            accept_invalid_certs: false,
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    let connect_result = master.connect().await;
    assert!(connect_result.is_ok(), "mTLS connection should succeed: {:?}", connect_result.err());
    sleep(Duration::from_millis(500)).await;

    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    capture::assert_tls_encrypted(&cap.pcap_path, port);
}

// =========================================================================
// Test: Full IEC 104 protocol over one-way TLS
//   1. General Interrogation
//   2. Spontaneous (change-of-state)
//   3. Control command (single point)
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_full_protocol() {
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: String::new(),
            key_file: String::new(),
            ca_file: String::new(),
            require_client_cert: false,
            pkcs12_file: paths.server_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    let mut station = Station::new(1, "TLS Protocol Test");
    station.batch_add_points(100, 1, AsduTypeId::MSpNa1, "SP").unwrap();
    station.batch_add_points(200, 1, AsduTypeId::MMeNc1, "FL").unwrap();
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut cap = capture::start("tls_full_protocol", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: String::new(),
            pkcs12_password: String::new(),
            accept_invalid_certs: false,
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    // --- 1. General Interrogation ---
    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let data = master.received_data.read().await;
        assert!(
            data.get(100, AsduTypeId::MSpNa1).is_some(),
            "IOA=100 (SP) should exist after GI"
        );
        assert!(
            data.get(200, AsduTypeId::MMeNc1).is_some(),
            "IOA=200 (Float) should exist after GI"
        );
    }

    // --- 2. Spontaneous (Change-of-State) ---
    {
        let mut stations = slave.stations.write().await;
        let st = stations.get_mut(&1).unwrap();
        let point = st.data_points.get_mut(100, AsduTypeId::MSpNa1).unwrap();
        point.value = DataPointValue::SinglePoint { value: true };
    }
    slave.queue_spontaneous(1, &[(100, AsduTypeId::MSpNa1)]).await;
    sleep(Duration::from_millis(2000)).await;

    {
        let data = master.received_data.read().await;
        let point = data.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(
            point.value,
            DataPointValue::SinglePoint { value: true },
            "Master should receive spontaneous update: SP=true"
        );
    }

    // --- 3. Control Command (single point) ---
    master.send_single_command(100, false, false, 1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let stations = slave.stations.read().await;
        let point = stations.get(&1).unwrap().data_points.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(
            point.value,
            DataPointValue::SinglePoint { value: false },
            "Slave data point should be updated by control command"
        );
    }

    {
        let data = master.received_data.read().await;
        let point = data.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(
            point.value,
            DataPointValue::SinglePoint { value: false },
            "Master should see control writeback via COT=3"
        );
    }

    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    capture::assert_tls_encrypted(&cap.pcap_path, port);
}

// =========================================================================
// Test: Full IEC 104 protocol over mutual TLS
// =========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_mtls_full_protocol() {
    if !check_tools_available() { return; }

    let port = free_port();
    let certs = cert_gen::generate();
    let tmp = tempfile::tempdir().unwrap();
    let paths = cert_gen::write_to_dir(&certs, tmp.path());

    let transport = SlaveTransportConfig {
        bind_address: "127.0.0.1".to_string(),
        port,
        tls: SlaveTlsConfig {
            enabled: true,
            cert_file: String::new(),
            key_file: String::new(),
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            require_client_cert: true,
            pkcs12_file: paths.server_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
        },
    };
    let mut slave = SlaveServer::new(transport);
    let mut station = Station::new(1, "mTLS Protocol Test");
    station.batch_add_points(100, 1, AsduTypeId::MSpNa1, "SP").unwrap();
    station.batch_add_points(200, 1, AsduTypeId::MMeNc1, "FL").unwrap();
    slave.add_station(station).await.unwrap();
    slave.start().await.unwrap();
    sleep(Duration::from_millis(300)).await;

    let mut cap = capture::start("tls_mtls_full_protocol", port)
        .expect("failed to start capture");
    sleep(Duration::from_millis(500)).await;

    let config = MasterConfig {
        target_address: "127.0.0.1".to_string(),
        port,
        common_address: 1,
        tls: TlsConfig {
            enabled: true,
            ca_file: paths.ca_cert.to_str().unwrap().to_string(),
            cert_file: String::new(),
            key_file: String::new(),
            pkcs12_file: paths.client_pkcs12.to_str().unwrap().to_string(),
            pkcs12_password: cert_gen::PKCS12_PASS.to_string(),
            accept_invalid_certs: false,
        },
        ..Default::default()
    };
    let mut master = MasterConnection::new(config);
    master.connect().await.unwrap();
    sleep(Duration::from_millis(500)).await;

    // --- 1. General Interrogation ---
    master.send_interrogation(1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let data = master.received_data.read().await;
        assert!(
            data.get(100, AsduTypeId::MSpNa1).is_some(),
            "IOA=100 (SP) should exist after GI over mTLS"
        );
        assert!(
            data.get(200, AsduTypeId::MMeNc1).is_some(),
            "IOA=200 (Float) should exist after GI over mTLS"
        );
    }

    // --- 2. Spontaneous (Change-of-State) ---
    {
        let mut stations = slave.stations.write().await;
        let st = stations.get_mut(&1).unwrap();
        let point = st.data_points.get_mut(100, AsduTypeId::MSpNa1).unwrap();
        point.value = DataPointValue::SinglePoint { value: true };
    }
    slave.queue_spontaneous(1, &[(100, AsduTypeId::MSpNa1)]).await;
    sleep(Duration::from_millis(2000)).await;

    {
        let data = master.received_data.read().await;
        let point = data.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(
            point.value,
            DataPointValue::SinglePoint { value: true },
            "Master should receive spontaneous update over mTLS"
        );
    }

    // --- 3. Control Command ---
    master.send_single_command(100, false, false, 1).await.unwrap();
    sleep(Duration::from_millis(2000)).await;

    {
        let stations = slave.stations.read().await;
        let point = stations.get(&1).unwrap().data_points.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(
            point.value,
            DataPointValue::SinglePoint { value: false },
            "Slave data point should be updated by control over mTLS"
        );
    }

    {
        let data = master.received_data.read().await;
        let point = data.get(100, AsduTypeId::MSpNa1).unwrap();
        assert_eq!(
            point.value,
            DataPointValue::SinglePoint { value: false },
            "Master should see control writeback over mTLS"
        );
    }

    master.disconnect().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    slave.stop().await.unwrap();
    sleep(Duration::from_millis(300)).await;
    cap.stop().expect("failed to stop capture");

    capture::assert_tls_encrypted(&cap.pcap_path, port);
}
