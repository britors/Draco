use serde::Serialize;
use crate::error::Result;
use crate::postgres::pool::PostgresDriver;
use super::helpers::*;

#[derive(Debug, Clone, Serialize)]
pub struct RoleInfo {
    pub rolname: String,
    pub rolsuper: bool,
    pub rolcreatedb: bool,
    pub rolcreaterole: bool,
    pub rolcanlogin: bool,
    pub rolconnlimit: i32,
    pub rolvaliduntil: Option<String>,
}

pub async fn get_roles(driver: &PostgresDriver) -> Result<Vec<RoleInfo>> {
    let rows = driver
        .query(
            "SELECT rolname, rolsuper, rolcreatedb, rolcreaterole, rolcanlogin, rolconnlimit, \
                    to_char(rolvaliduntil, 'YYYY-MM-DD') AS rolvaliduntil \
             FROM pg_roles ORDER BY rolname",
            &[],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| RoleInfo {
            rolname: get_str(r, "rolname"),
            rolsuper: get_bool(r, "rolsuper"),
            rolcreatedb: get_bool(r, "rolcreatedb"),
            rolcreaterole: get_bool(r, "rolcreaterole"),
            rolcanlogin: get_bool(r, "rolcanlogin"),
            rolconnlimit: get_i32(r, "rolconnlimit"),
            rolvaliduntil: get_opt_str(r, "rolvaliduntil"),
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct NewRole {
    pub name: String,
    pub password: Option<String>,
    pub login: bool,
    pub createdb: bool,
    pub createrole: bool,
    pub superuser: bool,
    pub conn_limit: i32,
    pub valid_until: Option<String>,
}

pub async fn create_role(driver: &PostgresDriver, role: &NewRole) -> Result<()> {
    let safe_name = quote_ident(&role.name);
    let mut parts = role_attributes_sql(role.login, role.createdb, role.createrole, role.superuser, role.conn_limit, role.valid_until.as_deref());
    if let Some(pw) = &role.password {
        if !pw.is_empty() {
            parts.push(format!("PASSWORD '{}'", pw.replace('\'', "''")));
        }
    }
    driver.query(&format!("CREATE ROLE {safe_name} {}", parts.join(" ")), &[]).await?;
    Ok(())
}

fn role_attributes_sql(login: bool, createdb: bool, createrole: bool, superuser: bool, conn_limit: i32, valid_until: Option<&str>) -> Vec<String> {
    let mut parts = vec![
        if login { "LOGIN" } else { "NOLOGIN" }.to_string(),
        if createdb { "CREATEDB" } else { "NOCREATEDB" }.to_string(),
        if createrole { "CREATEROLE" } else { "NOCREATEROLE" }.to_string(),
        if superuser { "SUPERUSER" } else { "NOSUPERUSER" }.to_string(),
        format!("CONNECTION LIMIT {}", conn_limit.max(-1)),
    ];
    let valid_until = valid_until.filter(|value| !value.trim().is_empty()).unwrap_or("infinity");
    parts.push(format!("VALID UNTIL '{}'", valid_until.replace('\'', "''")));
    parts
}

#[allow(clippy::too_many_arguments)]
pub async fn update_role(
    driver: &PostgresDriver,
    name: &str,
    login: bool,
    createdb: bool,
    createrole: bool,
    superuser: bool,
    conn_limit: i32,
    valid_until: Option<&str>,
    password: Option<&str>,
) -> Result<()> {
    let safe_name = quote_ident(name);
    let mut parts = role_attributes_sql(login, createdb, createrole, superuser, conn_limit, valid_until);
    if let Some(password) = password.filter(|value| !value.is_empty()) {
        parts.push(format!("PASSWORD '{}'", password.replace('\'', "''")));
    }
    driver.query(&format!("ALTER ROLE {safe_name} WITH {}", parts.join(" ")), &[]).await?;
    Ok(())
}

pub async fn drop_role(driver: &PostgresDriver, name: &str) -> Result<()> {
    let safe_name = quote_ident(name);
    driver.query(&format!("DROP ROLE IF EXISTS {safe_name}"), &[]).await?;
    Ok(())
}
