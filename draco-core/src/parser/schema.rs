//! Parser for `schema.draco` files (a Prisma-like schema DSL used to describe the data model
//! alongside a connection). Independent of any database driver.

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DracoDataSource {
    pub provider: String,
    pub url: String,
    pub is_postgres: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DracoField {
    pub name: String,
    pub draco_type: String,
    /// Actual DB column name (from `@map` or the field name).
    pub column_name: String,
    pub is_optional: bool,
    pub is_id: bool,
    /// Array fields (relations or native arrays).
    pub is_list: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DracoModel {
    pub name: String,
    /// From `@@map("...")` or the model name.
    pub table_name: String,
    pub fields: Vec<DracoField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedDracoSchema {
    pub file_path: String,
    pub datasource: Option<DracoDataSource>,
    pub models: Vec<DracoModel>,
}

pub fn parse_draco_schema(file_path: &str, content: &str) -> ParsedDracoSchema {
    let clean = strip_comments(content);
    ParsedDracoSchema {
        file_path: file_path.to_string(),
        datasource: parse_datasource(&clean),
        models: parse_models(&clean),
    }
}

fn strip_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            out.push(chars[i]);
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    out.push(chars[i]);
                    i += 1;
                    if i >= chars.len() {
                        break;
                    }
                }
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
        } else if chars[i] == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i = (i + 2).min(chars.len());
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn parse_datasource(content: &str) -> Option<DracoDataSource> {
    let block_re = Regex::new(r"(?s)datasource\s+\w+\s*\{(.*?)\}").unwrap();
    let block = block_re.captures(content)?.get(1)?.as_str();

    let provider_re = Regex::new(r#"\bprovider\s*=\s*"([^"]+)""#).unwrap();
    let url_re = Regex::new(r"\burl\s*=\s*(.+)").unwrap();

    let provider = provider_re
        .captures(block)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    let url = url_re
        .captures(block)
        .map(|c| c[1].trim().to_string())
        .unwrap_or_default();
    let is_postgres = provider.to_lowercase().contains("postgres");

    Some(DracoDataSource { provider, url, is_postgres })
}

fn parse_models(content: &str) -> Vec<DracoModel> {
    let model_start_re = Regex::new(r"^\s*model\s+(\w+)\s*\{").unwrap();
    let map_re = Regex::new(r#"@@map\s*\(\s*"([^"]+)"\s*\)"#).unwrap();

    let lines: Vec<&str> = content.lines().collect();
    let mut models = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(caps) = model_start_re.captures(lines[i]) {
            let name = caps[1].to_string();
            let mut depth = 1i32;
            let mut body_lines = Vec::new();
            i += 1;
            while i < lines.len() && depth > 0 {
                let line = lines[i];
                for ch in line.chars() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                if depth > 0 {
                    body_lines.push(line);
                }
                i += 1;
            }
            let body = body_lines.join("\n");
            let table_name = map_re
                .captures(&body)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| name.clone());
            models.push(DracoModel { name, table_name, fields: parse_fields(&body) });
        } else {
            i += 1;
        }
    }
    models
}

fn parse_fields(body: &str) -> Vec<DracoField> {
    let field_re = Regex::new(r"^(\w+)\s+([\w\[\]?]+)\s*(.*)?$").unwrap();
    let map_re = Regex::new(r#"@map\s*\(\s*"([^"]+)"\s*\)"#).unwrap();
    let id_re = Regex::new(r"@id\b").unwrap();

    let mut fields = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("@@") || trimmed.starts_with("//") {
            continue;
        }
        let Some(caps) = field_re.captures(trimmed) else { continue };
        let field_name = caps[1].to_string();
        let type_raw = caps[2].to_string();
        let attrs = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        let is_list = type_raw.contains('[');
        let is_optional = type_raw.ends_with('?');
        let draco_type = type_raw.replace(['[', ']', '?'], "");
        let column_name = map_re
            .captures(attrs)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| field_name.clone());

        fields.push(DracoField {
            name: field_name,
            draco_type,
            column_name,
            is_optional,
            is_id: id_re.is_match(attrs),
            is_list,
        });
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "/workspace/schema.draco";

    fn parse(content: &str) -> ParsedDracoSchema {
        parse_draco_schema(FILE, content)
    }

    #[test]
    fn empty_content_has_no_datasource() {
        assert_eq!(parse("").datasource, None);
    }

    #[test]
    fn parses_postgresql_provider_as_is_postgres() {
        let schema = parse(
            "datasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}",
        );
        let ds = schema.datasource.unwrap();
        assert_eq!(ds.provider, "postgresql");
        assert!(ds.is_postgres);
    }

    #[test]
    fn parses_postgres_provider_as_is_postgres() {
        let schema = parse(
            "datasource db {\n  provider = \"postgres\"\n  url      = \"postgresql://localhost\"\n}",
        );
        assert!(schema.datasource.unwrap().is_postgres);
    }

    #[test]
    fn marks_non_postgres_providers_as_not_postgres() {
        let schema =
            parse("datasource db {\n  provider = \"mysql\"\n  url      = \"mysql://localhost\"\n}");
        let ds = schema.datasource.unwrap();
        assert!(!ds.is_postgres);
    }

    #[test]
    fn stores_the_raw_url_string() {
        let schema = parse(
            "datasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}",
        );
        assert!(schema.datasource.unwrap().url.contains("DATABASE_URL"));
    }

    #[test]
    fn empty_content_has_no_models() {
        assert!(parse("").models.is_empty());
    }

    #[test]
    fn parses_a_simple_model() {
        let schema = parse("model User {\n  id   Int    @id\n  name String\n}");
        assert_eq!(schema.models.len(), 1);
        assert_eq!(schema.models[0].name, "User");
        assert_eq!(schema.models[0].table_name, "User");
    }

    #[test]
    fn uses_at_at_map_for_table_name() {
        let schema = parse("model User {\n  id Int @id\n  @@map(\"users\")\n}");
        assert_eq!(schema.models[0].table_name, "users");
    }

    #[test]
    fn uses_model_name_when_map_absent() {
        let schema = parse("model Post {\n  id Int @id\n}");
        assert_eq!(schema.models[0].table_name, "Post");
    }

    #[test]
    fn parses_multiple_models() {
        let schema = parse(
            "model User {\n  id Int @id\n}\nmodel Post {\n  id Int @id\n}\nmodel Comment {\n  id Int @id\n}\n",
        );
        assert_eq!(schema.models.len(), 3);
        let names: Vec<&str> = schema.models.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["User", "Post", "Comment"]);
    }

    #[test]
    fn parses_id_field_attribute() {
        let schema = parse("model User {\n  id Int @id\n}");
        let id = schema.models[0].fields.iter().find(|f| f.name == "id").unwrap();
        assert!(id.is_id);
    }

    #[test]
    fn parses_optional_field() {
        let schema = parse("model User {\n  id  Int     @id\n  bio String?\n}");
        let bio = schema.models[0].fields.iter().find(|f| f.name == "bio").unwrap();
        assert!(bio.is_optional);
    }

    #[test]
    fn parses_list_field() {
        let schema = parse("model User {\n  id   Int      @id\n  tags String[]\n}");
        let tags = schema.models[0].fields.iter().find(|f| f.name == "tags").unwrap();
        assert!(tags.is_list);
    }

    #[test]
    fn parses_map_column_alias() {
        let schema =
            parse("model User {\n  id        Int      @id\n  createdAt DateTime @map(\"created_at\")\n}");
        let f = schema.models[0].fields.iter().find(|f| f.name == "createdAt").unwrap();
        assert_eq!(f.column_name, "created_at");
    }

    #[test]
    fn uses_field_name_as_column_name_when_map_absent() {
        let schema = parse("model User {\n  id   Int    @id\n  name String\n}");
        let f = schema.models[0].fields.iter().find(|f| f.name == "name").unwrap();
        assert_eq!(f.column_name, "name");
    }

    #[test]
    fn strips_single_line_comments_before_parsing() {
        let schema = parse("// This is a comment\nmodel User {\n  // another comment\n  id Int @id // inline\n}");
        assert_eq!(schema.models.len(), 1);
        assert!(schema.models[0].fields.iter().any(|f| f.name == "id"));
    }

    #[test]
    fn strips_block_comments_before_parsing() {
        let schema = parse("/* block comment */\nmodel Post {\n  id Int @id\n}");
        assert_eq!(schema.models.len(), 1);
        assert_eq!(schema.models[0].name, "Post");
    }

    #[test]
    fn ignores_at_at_level_attributes_as_fields() {
        let schema = parse("model User {\n  id  Int    @id\n  @@index([id])\n  @@unique([id])\n}");
        let field_names: Vec<&str> = schema.models[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert!(!field_names.contains(&"@@index"));
        assert!(!field_names.contains(&"@@unique"));
    }

    #[test]
    fn stores_the_file_path_unchanged() {
        let schema = parse("model Foo { id Int @id }");
        assert_eq!(schema.file_path, FILE);
    }
}
