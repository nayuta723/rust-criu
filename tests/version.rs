fn version_test(criu_bin_path: &str) {
    let mut criu = rust_criu::Criu::new().unwrap();
    match criu.get_criu_version() {
        Ok(version) => println!("Version from CRIU found in $PATH: {}", version),
        Err(e) => println!("{:#?}", e),
    };

    criu = rust_criu::Criu::new_with_criu_path(criu_bin_path.to_string()).unwrap();
    match criu.get_criu_version() {
        Ok(version) => println!("Version from {}: {}", criu_bin_path, version),
        Err(e) => println!("{:#?}", e),
    };
}

#[test]
fn test() {
    let Some(criu_bin_path) = std::env::var("CRIU_BINARY").ok() else {
        eprintln!("skip: CRIU_BINARY not set");
        return;
    };
    version_test(&criu_bin_path);
}
