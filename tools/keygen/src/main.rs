//! Certificate generation tool for Remote Smartcard
//!
//! Generates CA, server, and client certificates for mTLS.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

/// Certificate generation tool for Remote Smartcard
#[derive(Parser, Debug)]
#[command(name = "rsc-keygen")]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generate Certificate Authority
    Ca {
        /// Output directory for CA files
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// CA Common Name
        #[arg(long, default_value = "RSC Root CA")]
        cn: String,

        /// Validity in days
        #[arg(long, default_value = "3650")]
        days: u32,
    },

    /// Generate server certificate
    Server {
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// CA certificate path
        #[arg(long)]
        ca_cert: PathBuf,

        /// CA key path
        #[arg(long)]
        ca_key: PathBuf,

        /// Server hostname (CN)
        #[arg(long)]
        hostname: String,

        /// Validity in days
        #[arg(long, default_value = "3650")]
        days: u32,
    },

    /// Generate client certificate
    Client {
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,

        /// CA certificate path
        #[arg(long)]
        ca_cert: PathBuf,

        /// CA key path
        #[arg(long)]
        ca_key: PathBuf,

        /// Client name (CN)
        #[arg(long)]
        name: String,

        /// Validity in days
        #[arg(long, default_value = "3650")]
        days: u32,
    },
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Ca { output, cn, days } => {
            generate_ca(&output, &cn, days)?;
        }
        Commands::Server {
            output,
            ca_cert,
            ca_key,
            hostname,
            days,
        } => {
            generate_server_cert(&output, &ca_cert, &ca_key, &hostname, days)?;
        }
        Commands::Client {
            output,
            ca_cert,
            ca_key,
            name,
            days,
        } => {
            generate_client_cert(&output, &ca_cert, &ca_key, &name, days)?;
        }
    }

    Ok(())
}

fn generate_ca(output: &PathBuf, cn: &str, days: u32) -> anyhow::Result<()> {
    std::fs::create_dir_all(output)?;

    let key_path = output.join("ca.key");
    let cert_path = output.join("ca.crt");

    println!("Generating CA private key...");

    // Generate private key
    let status = Command::new("openssl")
        .args(["genrsa", "-out"])
        .arg(&key_path)
        .arg("4096")
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate CA private key");
    }

    println!("Generating CA certificate...");

    // Generate self-signed certificate
    let subject = format!("/CN={}", cn);
    let status = Command::new("openssl")
        .args(["req", "-x509", "-new", "-nodes"])
        .args(["-key"])
        .arg(&key_path)
        .args(["-sha256", "-days"])
        .arg(days.to_string())
        .args(["-out"])
        .arg(&cert_path)
        .args(["-subj", &subject])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate CA certificate");
    }

    println!("CA generated successfully!");
    println!("  Key:  {:?}", key_path);
    println!("  Cert: {:?}", cert_path);

    Ok(())
}

fn generate_server_cert(
    output: &PathBuf,
    ca_cert: &PathBuf,
    ca_key: &PathBuf,
    hostname: &str,
    days: u32,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(output)?;

    let key_path = output.join("server.key");
    let csr_path = output.join("server.csr");
    let cert_path = output.join("server.crt");
    let ext_path = output.join("server_ext.cnf");

    println!("Generating server private key...");

    // Generate private key
    let status = Command::new("openssl")
        .args(["genrsa", "-out"])
        .arg(&key_path)
        .arg("4096")
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate server private key");
    }

    println!("Generating CSR...");

    // Generate CSR
    let subject = format!("/CN={}", hostname);
    let status = Command::new("openssl")
        .args(["req", "-new", "-key"])
        .arg(&key_path)
        .args(["-out"])
        .arg(&csr_path)
        .args(["-subj", &subject])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate CSR");
    }

    // Create extensions file for SAN
    // Check if hostname is an IP address
    let is_ip = hostname.parse::<std::net::IpAddr>().is_ok();
    let san = if is_ip {
        format!("IP:{}", hostname)
    } else {
        format!("DNS:{}", hostname)
    };

    let ext_content = format!(
        "subjectAltName = {}\nbasicConstraints = CA:FALSE\nkeyUsage = digitalSignature, keyEncipherment\nextendedKeyUsage = serverAuth",
        san
    );
    std::fs::write(&ext_path, ext_content)?;

    println!("Signing certificate with CA (SAN: {})...", san);

    // Sign with CA including extensions
    let status = Command::new("openssl")
        .args(["x509", "-req", "-in"])
        .arg(&csr_path)
        .args(["-CA"])
        .arg(ca_cert)
        .args(["-CAkey"])
        .arg(ca_key)
        .args(["-CAcreateserial", "-out"])
        .arg(&cert_path)
        .args(["-days"])
        .arg(days.to_string())
        .args(["-sha256", "-extfile"])
        .arg(&ext_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to sign server certificate");
    }

    // Cleanup temp files
    let _ = std::fs::remove_file(&csr_path);
    let _ = std::fs::remove_file(&ext_path);

    println!("Server certificate generated successfully!");
    println!("  Key:  {:?}", key_path);
    println!("  Cert: {:?}", cert_path);

    Ok(())
}

fn generate_client_cert(
    output: &PathBuf,
    ca_cert: &PathBuf,
    ca_key: &PathBuf,
    name: &str,
    days: u32,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(output)?;

    let key_path = output.join("client.key");
    let csr_path = output.join("client.csr");
    let cert_path = output.join("client.crt");

    println!("Generating client private key...");

    // Generate private key
    let status = Command::new("openssl")
        .args(["genrsa", "-out"])
        .arg(&key_path)
        .arg("4096")
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate client private key");
    }

    println!("Generating CSR...");

    // Generate CSR
    let subject = format!("/CN={}", name);
    let status = Command::new("openssl")
        .args(["req", "-new", "-key"])
        .arg(&key_path)
        .args(["-out"])
        .arg(&csr_path)
        .args(["-subj", &subject])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to generate CSR");
    }

    println!("Signing certificate with CA...");

    // Sign with CA
    let status = Command::new("openssl")
        .args(["x509", "-req", "-in"])
        .arg(&csr_path)
        .args(["-CA"])
        .arg(ca_cert)
        .args(["-CAkey"])
        .arg(ca_key)
        .args(["-CAcreateserial", "-out"])
        .arg(&cert_path)
        .args(["-days"])
        .arg(days.to_string())
        .args(["-sha256"])
        .status()?;

    if !status.success() {
        anyhow::bail!("Failed to sign client certificate");
    }

    // Cleanup CSR
    let _ = std::fs::remove_file(&csr_path);

    println!("Client certificate generated successfully!");
    println!("  Key:  {:?}", key_path);
    println!("  Cert: {:?}", cert_path);

    Ok(())
}
