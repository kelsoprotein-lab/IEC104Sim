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
