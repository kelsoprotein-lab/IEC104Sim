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
        pub client_cert: PathBuf,
        pub client_key: PathBuf,
    }

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

    /// Write all PEM files to the given directory, return paths.
    pub fn write_to_dir(certs: &TestCerts, dir: &Path) -> CertPaths {
        let paths = CertPaths {
            ca_cert: dir.join("ca.pem"),
            server_cert: dir.join("server.pem"),
            server_key: dir.join("server-key.pem"),
            client_cert: dir.join("client.pem"),
            client_key: dir.join("client-key.pem"),
        };
        std::fs::write(&paths.ca_cert, &certs.ca_cert_pem).unwrap();
        std::fs::write(&paths.server_cert, &certs.server_cert_pem).unwrap();
        std::fs::write(&paths.server_key, &certs.server_key_pem).unwrap();
        std::fs::write(&paths.client_cert, &certs.client_cert_pem).unwrap();
        std::fs::write(&paths.client_key, &certs.client_key_pem).unwrap();
        paths
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
    assert!(paths.client_cert.exists());
    assert!(paths.client_key.exists());
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
