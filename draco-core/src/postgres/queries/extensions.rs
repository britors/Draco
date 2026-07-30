use serde::Serialize;
use tokio_postgres::Row;

use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionInfo {
    pub name: String,
    pub default_version: Option<String>,
    pub installed_version: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Extensions {
    pub installed: Vec<ExtensionInfo>,
    pub available: Vec<ExtensionInfo>,
}

pub async fn get_extensions(driver: &PostgresDriver) -> Result<Extensions> {
    let installed = driver
        .query(
            "SELECT name, default_version, installed_version, comment FROM pg_available_extensions \
             WHERE installed_version IS NOT NULL ORDER BY name",
            &[],
        )
        .await?;
    let available = driver
        .query("SELECT name, default_version, comment FROM pg_available_extensions ORDER BY name LIMIT 200", &[])
        .await?;
    let to_ext = |r: &Row, with_installed: bool| ExtensionInfo {
        name: get_str(r, "name"),
        default_version: get_opt_str(r, "default_version"),
        installed_version: if with_installed { get_opt_str(r, "installed_version") } else { None },
        comment: get_opt_str(r, "comment"),
    };
    Ok(Extensions {
        installed: installed.iter().map(|r| to_ext(r, true)).collect(),
        available: available.iter().map(|r| to_ext(r, false)).collect(),
    })
}

pub async fn ext_install(driver: &PostgresDriver, name: &str) -> Result<()> {
    driver.query(&format!("CREATE EXTENSION IF NOT EXISTS {}", quote_ident(name)), &[]).await?;
    Ok(())
}

pub async fn ext_drop(driver: &PostgresDriver, name: &str) -> Result<()> {
    driver.query(&format!("DROP EXTENSION IF EXISTS {}", quote_ident(name)), &[]).await?;
    Ok(())
}
