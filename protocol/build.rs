fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Find the workspace root (where tools/protoc is located)
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let workspace_root = std::path::Path::new(&manifest_dir).parent().unwrap();

    // Set PROTOC to use the downloaded protoc binary
    let protoc_path = workspace_root.join("tools/protoc/bin/protoc.exe");
    if protoc_path.exists() {
        std::env::set_var("PROTOC", &protoc_path);
    }

    // Compile the protobuf file
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["smartcard.proto"], &["."])?;

    Ok(())
}
