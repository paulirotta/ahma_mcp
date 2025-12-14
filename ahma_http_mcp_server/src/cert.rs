//! Self-signed certificate generation for localhost
//!
//! This module automatically generates and caches self-signed certificates
//! for localhost use, avoiding the need for manual PKI setup.

use anyhow::{Context, Result};
use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};

const CERT_CACHE_DIR: &str = ".ahma_mcp_certs";
const CERT_FILE: &str = "localhost.crt";
const KEY_FILE: &str = "localhost.key";

/// Get the directory where certificates are cached
fn get_cert_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .context("Could not determine home directory")?;
    
    let mut path = PathBuf::from(home);
    path.push(CERT_CACHE_DIR);
    Ok(path)
}

/// Generate a self-signed certificate for localhost
fn generate_localhost_cert() -> Result<(String, String)> {
    info!("Generating new self-signed certificate for localhost");
    
    let mut params = CertificateParams::default();
    
    // Set subject alternative names for localhost
    params.subject_alt_names = vec![
        SanType::DnsName(rcgen::string::Ia5String::try_from("localhost").unwrap()),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1))),
    ];
    
    // Set distinguished name
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "localhost");
    dn.push(DnType::OrganizationName, "Ahma MCP Server");
    params.distinguished_name = dn;
    
    // Generate key pair
    let key_pair = KeyPair::generate()?;
    
    // Generate certificate
    let cert = params.self_signed(&key_pair)?;
    
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    
    debug!("Certificate generated successfully");
    Ok((cert_pem, key_pem))
}

/// Load or generate certificates for localhost
pub async fn get_or_create_localhost_certs(
    cert_dir_override: Option<&std::path::Path>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let cert_dir = if let Some(d) = cert_dir_override {
        d.to_path_buf()
    } else {
        get_cert_dir()?
    };
    let cert_path = cert_dir.join(CERT_FILE);
    let key_path = cert_dir.join(KEY_FILE);
    
    // Try to load existing certificates
    if cert_path.exists() && key_path.exists() {
        debug!("Loading cached certificates from {:?}", cert_dir);
        
        match (fs::read(&cert_path).await, fs::read(&key_path).await) {
            (Ok(cert), Ok(key)) => {
                info!("Using cached localhost certificates");
                return Ok((cert, key));
            }
            _ => {
                info!("Failed to read cached certificates, regenerating");
            }
        }
    }
    
    // Generate new certificates
    let (cert_pem, key_pem) = generate_localhost_cert()?;
    
    // Create cache directory if it doesn't exist
    fs::create_dir_all(&cert_dir)
        .await
        .context("Failed to create certificate cache directory")?;
    
    // Save certificates
    fs::write(&cert_path, cert_pem.as_bytes())
        .await
        .context("Failed to write certificate file")?;
    
    fs::write(&key_path, key_pem.as_bytes())
        .await
        .context("Failed to write key file")?;
    
    info!("Certificates saved to {:?}", cert_dir);
    
    Ok((cert_pem.into_bytes(), key_pem.into_bytes()))
}

/// Load certificates from PEM format
pub fn load_certs_from_pem(pem: &[u8]) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let mut cursor = std::io::Cursor::new(pem);
    let certs = rustls_pemfile::certs(&mut cursor)
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse certificate PEM")?;
    Ok(certs)
}

/// Load private key from PEM format
pub fn load_private_key_from_pem(pem: &[u8]) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
    let mut cursor = std::io::Cursor::new(pem);
    
    // Try to read as PKCS8 first
    if let Some(key) = rustls_pemfile::pkcs8_private_keys(&mut cursor)
        .next()
    {
        return key
            .map(rustls::pki_types::PrivateKeyDer::Pkcs8)
            .context("Failed to parse PKCS8 private key");
    }
    
    // Reset cursor and try RSA format
    cursor.set_position(0);
    if let Some(key) = rustls_pemfile::rsa_private_keys(&mut cursor)
        .next()
    {
        return key
            .map(rustls::pki_types::PrivateKeyDer::Pkcs1)
            .context("Failed to parse RSA private key");
    }
    
    // Reset cursor and try EC format
    cursor.set_position(0);
    if let Some(key) = rustls_pemfile::ec_private_keys(&mut cursor)
        .next()
    {
        return key
            .map(rustls::pki_types::PrivateKeyDer::Sec1)
            .context("Failed to parse EC private key");
    }
    
    anyhow::bail!("No valid private key found in PEM data")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_cert_generation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let result = get_or_create_localhost_certs(Some(temp_dir.path())).await;
        assert!(result.is_ok());
        
        let (cert_pem, key_pem) = result.unwrap();
        assert!(!cert_pem.is_empty());
        assert!(!key_pem.is_empty());
        
        // Verify we can parse the generated certificates
        let certs = load_certs_from_pem(&cert_pem);
        assert!(certs.is_ok());
        assert!(!certs.unwrap().is_empty());
        
        let key = load_private_key_from_pem(&key_pem);
        assert!(key.is_ok());
    }
}

