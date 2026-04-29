//! Dynamic certificate generation for TLS integration tests.
//!
//! Builds a CA, server cert (CN=localhost, SAN=127.0.0.1), and client cert
//! signed by the CA. PEM files plus PKCS#12 bundles get written to a tempdir
//! the caller controls. PKCS#12 generation goes through the `openssl` CLI
//! because native_tls on macOS Security Framework can't import ECDSA keys
//! through `Identity::from_pkcs8`.

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
        BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
        KeyUsagePurpose, SanType,
    };

    let mut ca_params = CertificateParams::new(vec!["IEC104 Test CA".to_string()]).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "IEC104 Test CA");
    let ca_key = KeyPair::generate().unwrap();
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();

    let mut server_params = CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    server_params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into().unwrap()),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
    ];
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server_params
        .distinguished_name
        .push(DnType::CommonName, "IEC104 Test Server");
    let server_key = KeyPair::generate().unwrap();
    let server_cert = server_params
        .signed_by(&server_key, &ca_cert, &ca_key)
        .unwrap();

    let mut client_params =
        CertificateParams::new(vec!["IEC104 Test Client".to_string()]).unwrap();
    client_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    client_params
        .distinguished_name
        .push(DnType::CommonName, "IEC104 Test Client");
    let client_key = KeyPair::generate().unwrap();
    let client_cert = client_params
        .signed_by(&client_key, &ca_cert, &ca_key)
        .unwrap();

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

    make_pkcs12(&paths.server_cert, &paths.server_key, &paths.server_pkcs12, PKCS12_PASS);
    make_pkcs12(&paths.client_cert, &paths.client_key, &paths.client_pkcs12, PKCS12_PASS);

    paths
}

fn make_pkcs12(cert: &Path, key: &Path, out: &Path, password: &str) {
    let status = std::process::Command::new("openssl")
        .args([
            "pkcs12",
            "-export",
            "-in",
            cert.to_str().unwrap(),
            "-inkey",
            key.to_str().unwrap(),
            "-out",
            out.to_str().unwrap(),
            "-passout",
            &format!("pass:{}", password),
        ])
        .status()
        .expect("openssl not found — required for PKCS#12 generation");
    assert!(status.success(), "openssl pkcs12 export failed");
}
