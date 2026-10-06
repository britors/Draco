//! Parsers for the connection descriptions other PostgreSQL clients already understand: libpq
//! connection URLs (`postgresql://…`), the password file (`~/.pgpass`) and the connection
//! service file (`~/.pg_service.conf`). They only turn text into connection fields; callers
//! decide what to test, save and where a password goes (always the credential store).
//!
//! Draco's TLS switch means "encrypt, accept any certificate", so `sslmode=require`,
//! `verify-ca` and `verify-full` all map to `ssl = true`; the last two also set
//! `ssl_verification_downgraded` so the interface can say the certificate is not verified.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Connection fields read from a URL, a `.pgpass` line or a service. `password` is the only
/// secret and must go straight to the credential store.
#[derive(Clone, PartialEq, Eq)]
pub struct ImportedConnection {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    pub ssl: bool,
    pub ssl_verification_downgraded: bool,
    pub password: Option<String>,
    /// Parameters that were understood syntactically but have no Draco equivalent.
    pub ignored_parameters: Vec<String>,
}

impl std::fmt::Debug for ImportedConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportedConnection")
            .field("label", &self.label)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("ssl", &self.ssl)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish_non_exhaustive()
    }
}

/// Why a URL was rejected. Each variant maps to one interface message; none carries the URL,
/// which may contain a password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlError {
    Scheme,
    Encoding,
    MultipleHosts,
    UnixSocket,
    Port,
    SslMode,
    Malformed,
}

const DEFAULT_PORT: u16 = 5432;
const DEFAULT_HOST: &str = "localhost";
/// Parses a libpq connection URI: `postgresql://[user[:password]@][host][:port][/dbname][?k=v&…]`.
/// Only one host is accepted; IPv6 literals use brackets (`[::1]:5432`); every component is
/// percent-decoded. The user is left empty when absent so the form asks for it.
pub fn parse_connection_url(url: &str) -> Result<ImportedConnection, UrlError> {
    let url = url.trim();
    let rest = strip_prefix_ignore_case(url, "postgresql://")
        .or_else(|| strip_prefix_ignore_case(url, "postgres://"))
        .ok_or(UrlError::Scheme)?;
    let (before_query, query) = match rest.split_once('?') {
        Some((before, query)) => (before, Some(query)),
        None => (rest, None),
    };
    let (authority, path) = match before_query.split_once('/') {
        Some((authority, path)) => (authority, Some(path)),
        None => (before_query, None),
    };
    // The password may contain `@` only when encoded, so the last `@` ends the user part.
    let (userinfo, hostinfo) = match authority.rsplit_once('@') {
        Some((userinfo, hostinfo)) => (Some(userinfo), hostinfo),
        None => (None, authority),
    };

    let mut user = String::new();
    let mut password = None;
    if let Some(userinfo) = userinfo {
        let (raw_user, raw_password) = match userinfo.split_once(':') {
            Some((user, password)) => (user, Some(password)),
            None => (userinfo, None),
        };
        user = percent_decode(raw_user)?;
        password = raw_password.map(percent_decode).transpose()?;
    }

    if hostinfo.contains(',') {
        return Err(UrlError::MultipleHosts);
    }
    let (mut host, mut port) = split_host_port(hostinfo)?;
    let mut database = path.map(percent_decode).transpose()?.unwrap_or_default();
    let mut ssl = false;
    let mut ssl_verification_downgraded = false;
    let mut ignored_parameters = Vec::new();

    for pair in query.unwrap_or_default().split('&') {
        if pair.is_empty() {
            continue;
        }
        let (raw_key, raw_value) = pair.split_once('=').ok_or(UrlError::Malformed)?;
        let key = percent_decode(raw_key)?;
        let value = percent_decode(raw_value)?;
        match key.as_str() {
            "host" | "hostaddr" => {
                if value.contains(',') {
                    return Err(UrlError::MultipleHosts);
                }
                if key == "host" || host.is_empty() {
                    host = value;
                }
            }
            "port" => port = Some(parse_port(&value).ok_or(UrlError::Port)?),
            "dbname" => database = value,
            "user" => user = value,
            "password" => password = Some(value),
            "sslmode" => {
                (ssl, ssl_verification_downgraded) =
                    ssl_from_mode(&value).ok_or(UrlError::SslMode)?;
            }
            other => ignored_parameters.push(other.to_string()),
        }
    }

    if host.starts_with('/') || host.starts_with('@') {
        return Err(UrlError::UnixSocket);
    }
    if host.is_empty() {
        host = DEFAULT_HOST.to_string();
    }
    if database.is_empty() {
        database.clone_from(&user);
    }
    ignored_parameters.sort();
    ignored_parameters.dedup();
    Ok(ImportedConnection {
        label: default_label(&database, &host),
        host,
        port: port.unwrap_or(DEFAULT_PORT),
        database,
        user,
        ssl,
        ssl_verification_downgraded,
        password: password.filter(|password| !password.is_empty()),
        ignored_parameters,
    })
}

fn strip_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &value[prefix.len()..])
}

fn split_host_port(hostinfo: &str) -> Result<(String, Option<u16>), UrlError> {
    if let Some(bracketed) = hostinfo.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']').ok_or(UrlError::Malformed)?;
        let port = match after {
            "" => None,
            after => {
                let raw = after.strip_prefix(':').ok_or(UrlError::Malformed)?;
                Some(parse_port(raw).ok_or(UrlError::Port)?)
            }
        };
        if host.is_empty() || !host.contains(':') {
            return Err(UrlError::Malformed);
        }
        return Ok((percent_decode(host)?, port));
    }
    match hostinfo.rsplit_once(':') {
        Some((host, raw_port)) => {
            let port = if raw_port.is_empty() {
                None
            } else {
                Some(parse_port(raw_port).ok_or(UrlError::Port)?)
            };
            Ok((percent_decode(host)?, port))
        }
        None => Ok((percent_decode(hostinfo)?, None)),
    }
}

fn parse_port(value: &str) -> Option<u16> {
    value.parse::<u16>().ok().filter(|port| *port > 0)
}

/// Maps `sslmode` to Draco's TLS switch and whether certificate verification was asked for.
fn ssl_from_mode(mode: &str) -> Option<(bool, bool)> {
    match mode {
        "disable" | "allow" | "prefer" => Some((false, false)),
        "require" => Some((true, false)),
        "verify-ca" | "verify-full" => Some((true, true)),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, UrlError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3).ok_or(UrlError::Encoding)?;
            let hex = std::str::from_utf8(hex).map_err(|_| UrlError::Encoding)?;
            decoded.push(u8::from_str_radix(hex, 16).map_err(|_| UrlError::Encoding)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).map_err(|_| UrlError::Encoding)?;
    if decoded.contains('\0') {
        return Err(UrlError::Encoding);
    }
    Ok(decoded)
}

fn default_label(database: &str, host: &str) -> String {
    if database.is_empty() {
        host.to_string()
    } else {
        format!("{database}@{host}")
    }
}

/// One `.pgpass` line. Fields keep the literal `*` wildcard.
#[derive(Clone, PartialEq, Eq)]
pub struct PgpassEntry {
    pub host: String,
    pub port: String,
    pub database: String,
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for PgpassEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PgpassEntry")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database", &self.database)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl PgpassEntry {
    fn matches(&self, host: &str, port: u16, database: &str, user: &str) -> bool {
        let field = |pattern: &str, value: &str| pattern == "*" || pattern == value;
        field(&self.host, host)
            && field(&self.port, &port.to_string())
            && field(&self.database, database)
            && field(&self.user, user)
    }
}

/// Parses the password file format `hostname:port:database:username:password`. `\` escapes
/// `:` and `\`; blank lines and lines starting with `#` are skipped, as are lines with fewer
/// than five fields.
pub fn parse_pgpass(content: &str) -> Vec<PgpassEntry> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let mut fields = Vec::with_capacity(5);
            let mut current = String::new();
            let mut chars = line.trim_end_matches('\r').chars();
            while let Some(character) = chars.next() {
                match character {
                    '\\' => {
                        if let Some(next) = chars.next() {
                            current.push(next);
                        }
                    }
                    // The password is the rest of the line, so only the first four separators split.
                    ':' if fields.len() < 4 => fields.push(std::mem::take(&mut current)),
                    character => current.push(character),
                }
            }
            fields.push(current);
            let [host, port, database, user, password]: [String; 5] = fields.try_into().ok()?;
            Some(PgpassEntry {
                host,
                port,
                database,
                user,
                password,
            })
        })
        .collect()
}

/// The first `.pgpass` password matching these fields, as libpq looks it up.
pub fn pgpass_password<'a>(
    entries: &'a [PgpassEntry],
    host: &str,
    port: u16,
    database: &str,
    user: &str,
) -> Option<&'a str> {
    entries
        .iter()
        .find(|entry| entry.matches(host, port, database, user))
        .map(|entry| entry.password.as_str())
}

/// Turns `.pgpass` lines into connections. A wildcard host or user cannot name a connection and
/// is skipped; a wildcard port means 5432 and a wildcard database means `postgres`. Repeated
/// targets keep only the first line, which is the one libpq would use.
pub fn pgpass_connections(entries: &[PgpassEntry]) -> Vec<ImportedConnection> {
    let mut seen = std::collections::HashSet::new();
    let mut connections = Vec::new();
    for entry in entries {
        if entry.host == "*" || entry.user == "*" || entry.host.is_empty() {
            continue;
        }
        if entry.host.starts_with('/') {
            continue;
        }
        let port = if entry.port == "*" {
            DEFAULT_PORT
        } else {
            match parse_port(&entry.port) {
                Some(port) => port,
                None => continue,
            }
        };
        let database = if entry.database == "*" {
            "postgres".to_string()
        } else {
            entry.database.clone()
        };
        if !seen.insert((
            entry.host.clone(),
            port,
            database.clone(),
            entry.user.clone(),
        )) {
            continue;
        }
        connections.push(ImportedConnection {
            label: default_label(&database, &entry.host),
            host: entry.host.clone(),
            port,
            database,
            user: entry.user.clone(),
            ssl: false,
            ssl_verification_downgraded: false,
            password: Some(entry.password.clone()).filter(|password| !password.is_empty()),
            ignored_parameters: Vec::new(),
        });
    }
    connections
}

/// One `[name]` section of a service file with its `key=value` lines.
#[derive(Clone, PartialEq, Eq)]
pub struct ServiceEntry {
    pub name: String,
    pub parameters: BTreeMap<String, String>,
}

impl std::fmt::Debug for ServiceEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys = self.parameters.keys().collect::<Vec<_>>();
        formatter
            .debug_struct("ServiceEntry")
            .field("name", &self.name)
            .field("keys", &keys)
            .finish()
    }
}

/// Parses the INI-like connection service file. Lines before the first section and malformed
/// lines are skipped; a repeated key keeps its last value.
pub fn parse_pg_service(content: &str) -> Vec<ServiceEntry> {
    let mut services: Vec<ServiceEntry> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            services.push(ServiceEntry {
                name: name.trim().to_string(),
                parameters: BTreeMap::new(),
            });
        } else if let (Some(service), Some((key, value))) =
            (services.last_mut(), line.split_once('='))
        {
            service
                .parameters
                .insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    services.retain(|service| !service.name.is_empty());
    services
}

/// Turns services into connections, taking a missing password from `.pgpass` like libpq does.
/// `default_user` stands in for libpq's "operating system user" default. Services pointing at a
/// Unix socket, with several hosts, a bad port or an unknown `sslmode` are skipped.
pub fn service_connections(
    services: &[ServiceEntry],
    pgpass: &[PgpassEntry],
    default_user: Option<&str>,
) -> Vec<ImportedConnection> {
    services
        .iter()
        .filter_map(|service| {
            let parameter = |key: &str| {
                service
                    .parameters
                    .get(key)
                    .map(String::as_str)
                    .filter(|value| !value.is_empty())
            };
            let host = parameter("host")
                .or_else(|| parameter("hostaddr"))
                .unwrap_or(DEFAULT_HOST);
            if host.contains(',') || host.starts_with('/') || host.starts_with('@') {
                return None;
            }
            let port = match parameter("port") {
                Some(port) => parse_port(port)?,
                None => DEFAULT_PORT,
            };
            let user = parameter("user").or(default_user)?.to_string();
            let database = parameter("dbname").unwrap_or(&user).to_string();
            let (ssl, ssl_verification_downgraded) = match parameter("sslmode") {
                Some(mode) => ssl_from_mode(mode)?,
                None => (false, false),
            };
            let password = parameter("password")
                .or_else(|| pgpass_password(pgpass, host, port, &database, &user))
                .filter(|password| !password.is_empty())
                .map(str::to_string);
            let mut ignored_parameters = service
                .parameters
                .keys()
                .filter(|key| {
                    !matches!(
                        key.as_str(),
                        "host" | "hostaddr" | "port" | "user" | "dbname" | "sslmode" | "password"
                    )
                })
                .cloned()
                .collect::<Vec<_>>();
            ignored_parameters.sort();
            Some(ImportedConnection {
                label: service.name.clone(),
                host: host.to_string(),
                port,
                database,
                user,
                ssl,
                ssl_verification_downgraded,
                password,
                ignored_parameters,
            })
        })
        .collect()
}

/// Where libpq looks for the password file: `PGPASSFILE`, then `~/.pgpass`
/// (`%APPDATA%\postgresql\pgpass.conf` on Windows).
pub fn default_pgpass_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PGPASSFILE").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|appdata| {
            PathBuf::from(appdata)
                .join("postgresql")
                .join("pgpass.conf")
        })
    } else {
        home_dir().map(|home| home.join(".pgpass"))
    }
}

/// Where libpq looks for the per-user service file: `PGSERVICEFILE`, then `~/.pg_service.conf`
/// (`%APPDATA%\postgresql\.pg_service.conf` on Windows).
pub fn default_service_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PGSERVICEFILE").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|appdata| {
            PathBuf::from(appdata)
                .join("postgresql")
                .join(".pg_service.conf")
        })
    } else {
        home_dir().map(|home| home.join(".pg_service.conf"))
    }
}

/// The operating system user name, libpq's default for `user`.
pub fn os_user() -> Option<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|user| !user.is_empty())
}

fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_full_url() {
        let parsed = parse_connection_url(
            "postgresql://alice:s3cret@db.example.com:6543/sales?sslmode=require",
        )
        .unwrap();
        assert_eq!(parsed.host, "db.example.com");
        assert_eq!(parsed.port, 6543);
        assert_eq!(parsed.database, "sales");
        assert_eq!(parsed.user, "alice");
        assert_eq!(parsed.password.as_deref(), Some("s3cret"));
        assert!(parsed.ssl);
        assert!(!parsed.ssl_verification_downgraded);
        assert_eq!(parsed.label, "sales@db.example.com");
    }

    #[test]
    fn accepts_the_short_scheme_and_defaults() {
        let parsed = parse_connection_url("postgres://").unwrap();
        assert_eq!(parsed.host, "localhost");
        assert_eq!(parsed.port, 5432);
        assert_eq!(parsed.user, "");
        assert_eq!(parsed.database, "");
        assert_eq!(parsed.password, None);
        let parsed = parse_connection_url("POSTGRESQL://bob@localhost").unwrap();
        assert_eq!(parsed.user, "bob");
        assert_eq!(parsed.database, "bob", "dbname defaults to the user");
    }

    #[test]
    fn decodes_percent_encoded_components() {
        let parsed = parse_connection_url(
            "postgresql://us%40er:p%3Aa%2Fss%40w%25rd@host/my%20db?application_name=x%26y",
        )
        .unwrap();
        assert_eq!(parsed.user, "us@er");
        assert_eq!(parsed.password.as_deref(), Some("p:a/ss@w%rd"));
        assert_eq!(parsed.database, "my db");
        assert_eq!(parsed.ignored_parameters, vec!["application_name"]);
        let parsed = parse_connection_url("postgresql://jos%C3%A9@host/caf%C3%A9").unwrap();
        assert_eq!(parsed.user, "josé");
        assert_eq!(parsed.database, "café");
    }

    #[test]
    fn a_raw_at_sign_in_the_password_still_splits_on_the_last_one() {
        let parsed = parse_connection_url("postgresql://u:p@ss@host/db").unwrap();
        assert_eq!(parsed.user, "u");
        assert_eq!(parsed.password.as_deref(), Some("p@ss"));
        assert_eq!(parsed.host, "host");
    }

    #[test]
    fn parses_ipv6_hosts() {
        let parsed = parse_connection_url("postgresql://u@[::1]:5433/db").unwrap();
        assert_eq!(parsed.host, "::1");
        assert_eq!(parsed.port, 5433);
        let parsed = parse_connection_url("postgresql://[2001:db8::1234]/db").unwrap();
        assert_eq!(parsed.host, "2001:db8::1234");
        assert_eq!(parsed.port, 5432);
        let parsed = parse_connection_url("postgresql://[fe80::1%25eth0]:5432/db").unwrap();
        assert_eq!(parsed.host, "fe80::1%eth0");
        assert_eq!(
            parse_connection_url("postgresql://[::1/db").unwrap_err(),
            UrlError::Malformed
        );
        assert_eq!(
            parse_connection_url("postgresql://[::1]5432/db").unwrap_err(),
            UrlError::Malformed
        );
    }

    #[test]
    fn query_parameters_override_the_authority() {
        let parsed = parse_connection_url(
            "postgresql:///?host=db.internal&port=5440&dbname=app&user=svc&password=pw&sslmode=verify-full",
        )
        .unwrap();
        assert_eq!(parsed.host, "db.internal");
        assert_eq!(parsed.port, 5440);
        assert_eq!(parsed.database, "app");
        assert_eq!(parsed.user, "svc");
        assert_eq!(parsed.password.as_deref(), Some("pw"));
        assert!(parsed.ssl && parsed.ssl_verification_downgraded);
        let parsed = parse_connection_url("postgresql://u@h/d?sslmode=prefer").unwrap();
        assert!(!parsed.ssl);
    }

    #[test]
    fn rejects_unsupported_urls() {
        assert_eq!(
            parse_connection_url("mysql://h/db").unwrap_err(),
            UrlError::Scheme
        );
        assert_eq!(
            parse_connection_url("postgresql://h1,h2/db").unwrap_err(),
            UrlError::MultipleHosts
        );
        assert_eq!(
            parse_connection_url("postgresql://%2Fvar%2Frun%2Fpostgresql/db").unwrap_err(),
            UrlError::UnixSocket
        );
        assert_eq!(
            parse_connection_url("postgresql:///db?host=/tmp").unwrap_err(),
            UrlError::UnixSocket
        );
        assert_eq!(
            parse_connection_url("postgresql://h:99999/db").unwrap_err(),
            UrlError::Port
        );
        assert_eq!(
            parse_connection_url("postgresql://h:abc/db").unwrap_err(),
            UrlError::Port
        );
        assert_eq!(
            parse_connection_url("postgresql://h/db?sslmode=maybe").unwrap_err(),
            UrlError::SslMode
        );
        assert_eq!(
            parse_connection_url("postgresql://h/d%zz").unwrap_err(),
            UrlError::Encoding
        );
        assert_eq!(
            parse_connection_url("postgresql://h/d%ff").unwrap_err(),
            UrlError::Encoding,
            "invalid UTF-8"
        );
        assert_eq!(
            parse_connection_url("postgresql://h/d%00").unwrap_err(),
            UrlError::Encoding
        );
        assert_eq!(
            parse_connection_url("postgresql://h/d?sslmode").unwrap_err(),
            UrlError::Malformed
        );
    }

    #[test]
    fn debug_output_redacts_the_password() {
        let parsed = parse_connection_url("postgresql://u:topsecret@h/d").unwrap();
        assert!(!format!("{parsed:?}").contains("topsecret"));
        let entries = parse_pgpass("h:5432:d:u:topsecret");
        assert!(!format!("{entries:?}").contains("topsecret"));
        let services = parse_pg_service("[s]\npassword=topsecret");
        assert!(!format!("{services:?}").contains("topsecret"));
    }

    #[test]
    fn parses_pgpass_with_escapes_comments_and_wildcards() {
        let entries = parse_pgpass(
            "# comment\n\nlocalhost:5432:app:alice:pa\\:ss\\\\word\n*:*:*:bob:x:y\r\nshort:line\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].password, "pa:ss\\word");
        assert_eq!(entries[1].host, "*");
        assert_eq!(
            entries[1].password, "x:y",
            "the password is the rest of the line"
        );
        assert_eq!(
            pgpass_password(&entries, "db", 6000, "anything", "bob"),
            Some("x:y")
        );
        assert_eq!(
            pgpass_password(&entries, "localhost", 5432, "app", "alice"),
            Some("pa:ss\\word")
        );
        assert_eq!(
            pgpass_password(&entries, "localhost", 5433, "app", "alice"),
            None
        );
    }

    #[test]
    fn pgpass_connections_skip_wildcard_targets_and_duplicates() {
        let entries = parse_pgpass(
            "db.example.com:*:*:alice:one\n\
             db.example.com:5432:postgres:alice:two\n\
             *:5432:app:bob:three\n\
             db:5432:app:*:four\n\
             db:bad:app:carol:five\n\
             /var/run/postgresql:5432:app:dave:six\n\
             ::1:5433:app:erin:seven\n",
        );
        let connections = pgpass_connections(&entries);
        // Duplicates, wildcard hosts/users, bad ports, sockets and an unescaped IPv6 host (whose
        // colons split the line, so libpq would not match it either) are all skipped.
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].port, 5432);
        assert_eq!(connections[0].database, "postgres");
        assert_eq!(connections[0].password.as_deref(), Some("one"));
        assert_eq!(connections[0].label, "postgres@db.example.com");
        let escaped = pgpass_connections(&parse_pgpass("\\:\\:1:5433:app:erin:seven"));
        assert_eq!(escaped[0].host, "::1");
    }

    #[test]
    fn parses_services_and_fills_passwords_from_pgpass() {
        let services = parse_pg_service(
            "ignored=before\n# c\n[prod]\nhost = db.example.com\nport=6432\ndbname=sales\nuser=app\nsslmode=verify-ca\napplication_name=x\n\
             [local]\ndbname=dev\n[socket]\nhost=/tmp\n[broken]\nport=0\n[]\nhost=x\n",
        );
        assert_eq!(
            services.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            vec!["prod", "local", "socket", "broken"]
        );
        let pgpass = parse_pgpass("db.example.com:6432:sales:app:from-pgpass\n");
        let connections = service_connections(&services, &pgpass, Some("osuser"));
        assert_eq!(connections.len(), 2);
        let prod = &connections[0];
        assert_eq!(prod.label, "prod");
        assert_eq!(prod.host, "db.example.com");
        assert_eq!(prod.port, 6432);
        assert!(prod.ssl && prod.ssl_verification_downgraded);
        assert_eq!(prod.password.as_deref(), Some("from-pgpass"));
        assert_eq!(prod.ignored_parameters, vec!["application_name"]);
        let local = &connections[1];
        assert_eq!(local.host, "localhost");
        assert_eq!(local.user, "osuser");
        assert_eq!(local.database, "dev");
        assert_eq!(local.password, None);
        assert!(service_connections(&services[1..2], &[], None).is_empty());
    }
}
