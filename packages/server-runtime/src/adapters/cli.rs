use std::net::SocketAddr;

pub fn validate_foundation_address(address: &str) -> Result<SocketAddr, String> {
    let address: SocketAddr = address
        .parse()
        .map_err(|_| "EXPLICIT_LOOPBACK_ADDRESS_REQUIRED")?;
    if !address.ip().is_loopback()
        || address.port() == 0
        || matches!(
            address.port(),
            3000 | 3001 | 3100 | 3101 | 3103 | 5432 | 5433 | 5435 | 8080 | 3200..=3299
        )
    {
        return Err("DISPOSABLE_FOUNDATION_PORT_REQUIRED".to_owned());
    }
    Ok(address)
}
