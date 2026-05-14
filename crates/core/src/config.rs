use std::{fs, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read shop config {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("shop config must contain exactly one non-empty line")]
    InvalidLineCount,
    #[error("shop id must be a positive integer")]
    InvalidShopId,
}

pub fn read_shop_id(path: impl AsRef<Path>) -> Result<i64, ConfigError> {
    let path = path.as_ref();
    let body = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;

    let lines: Vec<_> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() != 1 {
        return Err(ConfigError::InvalidLineCount);
    }

    let shop_id = lines[0]
        .parse::<i64>()
        .map_err(|_| ConfigError::InvalidShopId)?;
    if shop_id <= 0 {
        return Err(ConfigError::InvalidShopId);
    }

    Ok(shop_id)
}
