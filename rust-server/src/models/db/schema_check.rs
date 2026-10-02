use std::collections::{HashMap, HashSet};

use sqlx::{Pool, Postgres};

/// Table inventory comes from the sqlx baseline (single source of truth).
const BASELINE_SQL: &str = include_str!("../../../migrations/1_baseline.sql");
include!(concat!(env!("OUT_DIR"), "/kysely_migrations.rs"));

const OPTIONAL_TABLES: &[&str] = &["smart_search", "face_search"];
const IGNORED_EXTRA_TABLES: &[&str] = &[
    "kysely_migrations",
    "kysely_migrations_lock",
    "_sqlx_migrations",
    "migration_overrides",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonSchemaVariant {
    Legacy,
    ClusterGroups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStatus {
    Applied,
    Missing,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationCheck {
    pub name: String,
    pub status: MigrationStatus,
}

#[derive(Debug, Default)]
pub struct SchemaCheckReport {
    pub missing_tables: Vec<String>,
    pub extra_tables: Vec<String>,
    pub missing_columns: Vec<String>,
    pub extra_columns: Vec<String>,
    pub drift_lines: Vec<String>,
    pub repair_sql: Vec<String>,
    pub optional_missing: Vec<String>,
    pub vector_extension: bool,
    pub migrations: Option<Vec<MigrationCheck>>,
}

pub fn expected_columns() -> HashMap<String, HashSet<String>> {
    let table_re =
        regex::Regex::new(r#"^CREATE TABLE\s+(?:"([^"]+)"|([a-z_][a-z0-9_]*))\s*\("#).unwrap();
    let column_re = regex::Regex::new(r#"^\s+(?:"([^"]+)"|([a-z_][a-z0-9_]*))\s+\S"#).unwrap();
    let mut tables: HashMap<String, HashSet<String>> = HashMap::new();
    let mut current: Option<String> = None;
    let mut depth = 0i32;

    for line in BASELINE_SQL.lines() {
        let trimmed = line.trim();
        if current.is_none() {
            if let Some(caps) = table_re.captures(trimmed) {
                let name = caps
                    .get(1)
                    .or_else(|| caps.get(2))
                    .map(|item| item.as_str().to_string())
                    .expect("table name");
                depth = paren_delta(trimmed);
                tables.entry(name.clone()).or_default();
                current = Some(name);
            }
            continue;
        }

        depth += paren_delta(trimmed);
        if depth <= 0 {
            current = None;
            continue;
        }

        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("PRIMARY ")
            || upper.starts_with("CONSTRAINT ")
            || upper.starts_with("UNIQUE ")
            || upper.starts_with("CHECK ")
            || upper.starts_with("FOREIGN ")
            || upper.starts_with("EXCLUDE ")
        {
            continue;
        }

        if let Some(caps) = column_re.captures(line) {
            let column = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|item| item.as_str().to_string())
                .expect("column name");
            if let Some(table) = &current {
                tables.get_mut(table).expect("table").insert(column);
            }
        }
    }

    tables
}

fn paren_delta(line: &str) -> i32 {
    let mut delta = 0i32;
    for ch in line.chars() {
        match ch {
            '(' => delta += 1,
            ')' => delta -= 1,
            _ => {}
        }
    }
    delta
}

pub fn expected_tables() -> HashSet<String> {
    let re = regex::Regex::new(r#"CREATE TABLE\s+(?:"([^"]+)"|([a-z_]+))"#).unwrap();
    BASELINE_SQL
        .lines()
        .filter_map(|line| {
            re.captures(line.trim()).map(|caps| {
                caps.get(1)
                    .or_else(|| caps.get(2))
                    .map(|m| m.as_str().to_string())
                    .expect("table name capture")
            })
        })
        .collect()
}

pub fn expected_migration_names() -> &'static [&'static str] {
    KYSELY_MIGRATION_NAMES
}

pub fn compare_migrations(expected: &[&str], applied: &[String]) -> Vec<MigrationCheck> {
    let files_set: HashSet<&str> = expected.iter().copied().collect();
    let rows_set: HashSet<&str> = applied.iter().map(String::as_str).collect();

    let mut combined: Vec<String> = files_set
        .union(&rows_set)
        .map(|name| (*name).to_string())
        .collect();
    combined.sort();

    combined
        .into_iter()
        .map(|name| {
            let in_files = files_set.contains(name.as_str());
            let in_rows = rows_set.contains(name.as_str());
            let status = match (in_files, in_rows) {
                (true, true) => MigrationStatus::Applied,
                (true, false) => MigrationStatus::Missing,
                (false, true) => MigrationStatus::Deleted,
                (false, false) => MigrationStatus::Applied,
            };
            MigrationCheck { name, status }
        })
        .collect()
}

pub async fn detect_person_schema_variant(
    pool: &Pool<Postgres>,
) -> Result<PersonSchemaVariant, sqlx::Error> {
    let has_person_group: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = 'person_group'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    Ok(if has_person_group {
        PersonSchemaVariant::ClusterGroups
    } else {
        PersonSchemaVariant::Legacy
    })
}

pub async fn run(pool: &Pool<Postgres>) -> Result<SchemaCheckReport, sqlx::Error> {
    let expected = expected_tables();
    let actual = list_public_tables(pool).await?;
    let optional: HashSet<&str> = OPTIONAL_TABLES.iter().copied().collect();
    let ignored: HashSet<&str> = IGNORED_EXTRA_TABLES.iter().copied().collect();

    let mut missing_tables: Vec<String> = expected
        .difference(&actual)
        .filter(|table| !optional.contains(table.as_str()))
        .cloned()
        .collect();
    missing_tables.sort();

    let optional_missing: Vec<String> = OPTIONAL_TABLES
        .iter()
        .filter(|table| !actual.contains(**table))
        .map(|table| (*table).to_string())
        .collect();

    let mut extra_tables: Vec<String> = actual
        .difference(&expected)
        .filter(|table| !ignored.contains(table.as_str()))
        .cloned()
        .collect();
    extra_tables.sort();

    let vector_extension = extension_installed(pool, "vector").await?;
    let migrations = kysely_migration_status(pool).await?;
    let (missing_columns, extra_columns, drift_lines, repair_sql) =
        catalog_drift(pool, &actual).await?;

    Ok(SchemaCheckReport {
        missing_tables,
        extra_tables,
        missing_columns,
        extra_columns,
        drift_lines,
        repair_sql,
        optional_missing,
        vector_extension,
        migrations,
    })
}

struct BaselineCatalog {
    column_types: HashMap<String, HashMap<String, String>>,
    indexes: HashMap<String, String>,
    enums: HashMap<String, Vec<String>>,
    functions: HashSet<String>,
    function_sql: HashMap<String, String>,
    function_args: HashMap<String, String>,
    triggers: HashSet<String>,
    trigger_sql: HashMap<String, String>,
}

fn baseline_catalog() -> BaselineCatalog {
    let mut catalog = BaselineCatalog {
        column_types: HashMap::new(),
        indexes: HashMap::new(),
        enums: HashMap::new(),
        functions: HashSet::new(),
        function_sql: HashMap::new(),
        function_args: HashMap::new(),
        triggers: HashSet::new(),
        trigger_sql: HashMap::new(),
    };
    let table_re =
        regex::Regex::new(r#"^CREATE TABLE\s+(?:"([^"]+)"|([a-z_][a-z0-9_]*))\s*\("#).unwrap();
    let column_re = regex::Regex::new(r#"^\s+(?:"([^"]+)"|([a-z_][a-z0-9_]*))\s+(.*)$"#).unwrap();
    let enum_re =
        regex::Regex::new(r#"^CREATE TYPE\s+([a-z0-9_]+)\s+AS\s+ENUM\s*\((.*)\)"#).unwrap();
    let index_re =
        regex::Regex::new(r#"^CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:"([^"]+)"|([a-z0-9_]+))"#).unwrap();
    let function_re =
        regex::Regex::new(r#"^CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([a-z0-9_]+)"#).unwrap();
    let trigger_re =
        regex::Regex::new(r#"^CREATE\s+TRIGGER\s+(?:"([^"]+)"|([a-z0-9_]+))"#).unwrap();

    let lines: Vec<&str> = BASELINE_SQL.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if let Some(caps) = enum_re.captures(trimmed) {
            let name = caps[1].to_string();
            let labels = caps[2]
                .split(',')
                .filter_map(|label| {
                    let label = label.trim().trim_matches('\'').trim();
                    if label.is_empty() {
                        None
                    } else {
                        Some(label.to_string())
                    }
                })
                .collect();
            catalog.enums.insert(name, labels);
        } else if let Some(caps) = function_re.captures(trimmed) {
            let (statement, next) = statement_from(&lines, index);
            catalog.functions.insert(caps[1].to_string());
            catalog
                .function_args
                .insert(caps[1].to_string(), function_identity_args(&statement));
            catalog.function_sql.insert(caps[1].to_string(), statement);
            index = next;
            continue;
        } else if let Some(caps) = index_re.captures(trimmed) {
            let name = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|item| item.as_str().to_string())
                .unwrap_or_default();
            let (statement, next) = statement_from(&lines, index);
            catalog.indexes.insert(name.clone(), statement);
            index = next;
            continue;
        } else if let Some(caps) = trigger_re.captures(trimmed) {
            let name = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|item| item.as_str().to_string())
                .unwrap_or_default();
            let (statement, next) = statement_from(&lines, index);
            catalog.triggers.insert(name.clone());
            catalog.trigger_sql.insert(name, statement);
            index = next;
            continue;
        } else if let Some(caps) = table_re.captures(trimmed) {
            let table = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|item| item.as_str().to_string())
                .unwrap_or_default();
            let mut depth = paren_delta(trimmed);
            index += 1;
            while index < lines.len() && depth > 0 {
                let row = lines[index].trim();
                depth += paren_delta(row);
                if depth > 0 {
                    if let Some(column) = column_re.captures(lines[index]) {
                        let name = column
                            .get(1)
                            .or_else(|| column.get(2))
                            .map(|item| item.as_str().to_string())
                            .unwrap_or_default();
                        let upper = row.to_ascii_uppercase();
                        if !upper.starts_with("PRIMARY ")
                            && !upper.starts_with("CONSTRAINT ")
                            && !upper.starts_with("UNIQUE ")
                            && !upper.starts_with("CHECK ")
                            && !upper.starts_with("FOREIGN ")
                            && !name.is_empty()
                        {
                            let ty = normalize_sql_type(
                                column.get(3).map(|item| item.as_str()).unwrap_or(""),
                            );
                            catalog
                                .column_types
                                .entry(table.clone())
                                .or_default()
                                .insert(name, ty);
                        }
                    }
                }
                index += 1;
            }
            continue;
        }
        index += 1;
    }
    catalog
}

fn statement_from(lines: &[&str], start: usize) -> (String, usize) {
    let mut statement = String::new();
    let mut index = start;
    while index < lines.len() {
        if !statement.is_empty() {
            statement.push('\n');
        }
        statement.push_str(lines[index]);
        let done = !in_dollar_quote(&statement) && lines[index].trim_end().ends_with(';');
        index += 1;
        if done {
            break;
        }
    }
    (statement, index)
}

fn in_dollar_quote(sql: &str) -> bool {
    let markers = regex::Regex::new(r"\$([A-Za-z0-9_]*)\$").unwrap();
    let mut open: Option<String> = None;
    for marker in markers.captures_iter(sql) {
        let tag = marker.get(0).unwrap().as_str().to_string();
        if open.as_deref() == Some(tag.as_str()) {
            open = None;
        } else if open.is_none() {
            open = Some(tag);
        }
    }
    open.is_some()
}

fn normalize_sql_type(raw: &str) -> String {
    let token = raw
        .split_whitespace()
        .take_while(|part| {
            let upper = part.to_ascii_uppercase();
            !matches!(
                upper.as_str(),
                "NOT"
                    | "NULL"
                    | "DEFAULT"
                    | "PRIMARY"
                    | "REFERENCES"
                    | "UNIQUE"
                    | "CHECK"
                    | "CONSTRAINT"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let lower = token.trim().trim_end_matches(',').to_ascii_lowercase();
    let array = lower.ends_with("[]");
    let base = lower.trim_end_matches("[]").trim();
    let normalized = if base.starts_with("varchar") || base.starts_with("character varying") {
        "character varying"
    } else if base == "timestamptz" || base.starts_with("timestamp with time zone") {
        "timestamp with time zone"
    } else if base == "int" || base == "int4" {
        "integer"
    } else if base == "int8" {
        "bigint"
    } else if base == "bool" {
        "boolean"
    } else if base == "float8" || base.starts_with("double precision") {
        "double precision"
    } else if base.starts_with("vector(") || base == "vector" {
        base
    } else {
        base
    };
    if array {
        format!("{normalized}[]")
    } else {
        normalized.to_string()
    }
}

fn function_identity_args(statement: &str) -> String {
    let Some(inner) = first_paren(statement) else {
        return String::new();
    };
    split_top_level(&inner, ',')
        .into_iter()
        .map(|arg| arg_type(&arg))
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn arg_type(arg: &str) -> String {
    let lower = arg.trim().to_ascii_lowercase();
    let without_default = match lower.find(" default ") {
        Some(pos) => &lower[..pos],
        None => lower.as_str(),
    };
    let mut tokens: Vec<&str> = without_default.split_whitespace().collect();
    if tokens
        .first()
        .is_some_and(|token| matches!(*token, "in" | "out" | "inout" | "variadic"))
    {
        tokens.remove(0);
    }
    let text = tokens.join(" ");
    const MULTI: &[&str] = &[
        "timestamp with time zone",
        "timestamp without time zone",
        "time with time zone",
        "time without time zone",
        "double precision",
        "character varying",
    ];
    for ty in MULTI {
        if text == *ty || text.ends_with(&format!(" {ty}")) {
            return normalize_sql_type(ty);
        }
    }
    tokens
        .last()
        .map(|token| normalize_sql_type(token))
        .unwrap_or_default()
}

fn function_args_match(expected: &str, actual: &str) -> bool {
    let expected = split_top_level(expected, ',')
        .into_iter()
        .map(|part| normalize_sql_type(part.trim()))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let actual = split_top_level(actual, ',')
        .into_iter()
        .map(|part| normalize_sql_type(part.trim()))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    actual.len() >= expected.len()
        && expected
            .iter()
            .zip(&actual)
            .all(|(left, right)| left == right)
}

fn normalize_db_type(data_type: &str, udt_name: &str) -> String {
    if data_type == "ARRAY" {
        let base = match udt_name.trim_start_matches('_') {
            "int4" => "integer",
            "int8" => "bigint",
            "varchar" => "character varying",
            "float8" => "double precision",
            "bool" => "boolean",
            other => other,
        };
        return format!("{base}[]");
    }
    if data_type == "USER-DEFINED" {
        if udt_name == "vector" {
            return "vector".to_string();
        }
        return udt_name.to_string();
    }
    data_type.to_string()
}

fn expected_extensions() -> HashSet<String> {
    let re = regex::Regex::new(
        r#"(?i)^CREATE EXTENSION(?:\s+IF\s+NOT\s+EXISTS)?\s+"?([A-Za-z0-9_-]+)"?"#,
    )
    .unwrap();
    let mut names = HashSet::new();
    let mut sql = String::new();
    for line in BASELINE_SQL.lines() {
        sql.push_str(line);
        sql.push('\n');
        if in_dollar_quote(&sql) {
            continue;
        }
        if let Some(caps) = re.captures(line.trim()) {
            names.insert(caps[1].to_string());
        }
    }
    names
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

struct ExpectedConstraint {
    id: String,
    table: String,
    name: String,
    kind: &'static str,
    sql: String,
    check_expr: Option<String>,
}

fn drift_line(kind: &str, text: &str) -> String {
    format!("{kind}: {text}")
}

fn index_table(statement: &str) -> Option<String> {
    relation_after_on(statement)
}

fn trigger_table(statement: &str) -> Option<String> {
    relation_after_on(statement)
}

fn relation_after_on(statement: &str) -> Option<String> {
    let re = regex::Regex::new(r#"(?i)\bON\s+("([^"]+)"|([A-Za-z_][A-Za-z0-9_]*))"#).unwrap();
    re.captures(statement).and_then(|caps| {
        caps.get(2)
            .or_else(|| caps.get(3))
            .map(|item| item.as_str().to_string())
    })
}

fn constraint_kind_name(kind: &str) -> String {
    match kind {
        "p" => "primary-key".to_string(),
        "u" => "unique".to_string(),
        "f" => "foreign-key".to_string(),
        "c" => "check".to_string(),
        other => other.to_string(),
    }
}

fn check_expressions_match(item: &ExpectedConstraint, definition: &str) -> bool {
    let Some(expected) = &item.check_expr else {
        return true;
    };
    normalize_check(expected) == normalize_check(definition)
}

fn normalize_check(sql: &str) -> String {
    sql.chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '"' && *ch != '(' && *ch != ')')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn baseline_constraints() -> Vec<ExpectedConstraint> {
    let mut out = Vec::new();
    for (table, body) in table_bodies() {
        for item in split_top_level(&body, ',') {
            push_table_item(&mut out, &table, &item);
        }
    }
    push_alter_constraints(&mut out);
    out
}

fn table_bodies() -> Vec<(String, String)> {
    let table_re =
        regex::Regex::new(r#"^CREATE TABLE\s+(?:"([^"]+)"|([a-z_][a-z0-9_]*))\s*\("#).unwrap();
    let lines: Vec<&str> = BASELINE_SQL.lines().collect();
    let mut bodies = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if let Some(caps) = table_re.captures(trimmed) {
            let table = caps
                .get(1)
                .or_else(|| caps.get(2))
                .map(|item| item.as_str().to_string())
                .unwrap_or_default();
            let mut depth = paren_delta(trimmed);
            let mut body = String::new();
            if let Some(pos) = trimmed.find('(') {
                body.push_str(&trimmed[pos + 1..]);
                body.push('\n');
            }
            index += 1;
            while index < lines.len() && depth > 0 {
                let row = lines[index].trim();
                let next_depth = depth + paren_delta(row);
                if next_depth <= 0 {
                    let without = row.trim_end_matches(';').trim_end_matches(')').trim();
                    if !without.is_empty() {
                        body.push_str(without);
                        body.push('\n');
                    }
                    index += 1;
                    break;
                }
                body.push_str(row);
                body.push('\n');
                depth = next_depth;
                index += 1;
            }
            bodies.push((table, body));
            continue;
        }
        index += 1;
    }
    bodies
}

fn push_alter_constraints(out: &mut Vec<ExpectedConstraint>) {
    let re =
        regex::Regex::new(r#"(?i)^ALTER TABLE\s+("([^"]+)"|([A-Za-z_][A-Za-z0-9_]*))"#).unwrap();
    let lines: Vec<&str> = BASELINE_SQL.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim();
        if let Some(caps) = re.captures(trimmed) {
            let table = caps
                .get(2)
                .or_else(|| caps.get(3))
                .map(|item| item.as_str().to_string())
                .unwrap_or_default();
            let (statement, next) = statement_from(&lines, index);
            index = next;
            let compact = statement.split_whitespace().collect::<Vec<_>>().join(" ");
            let upper = compact.to_ascii_uppercase();
            if let Some(pos) = upper.find("ADD CONSTRAINT ") {
                let rest = compact[pos + "ADD CONSTRAINT ".len()..]
                    .trim()
                    .trim_end_matches(';');
                push_named_constraint(out, &table, &format!("CONSTRAINT {rest}"));
            }
            continue;
        }
        index += 1;
    }
}

fn push_table_item(out: &mut Vec<ExpectedConstraint>, table: &str, item: &str) {
    let compact = item.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return;
    }
    let upper = compact.to_ascii_uppercase();
    if upper.starts_with("CONSTRAINT ") {
        push_named_constraint(out, table, &compact);
        return;
    }
    if upper.starts_with("PRIMARY KEY") {
        if let Some(cols) = first_paren(&compact) {
            push_pk(out, table, &parse_ident_list(&cols), None);
        }
        return;
    }
    if upper.starts_with("UNIQUE") {
        if let Some(cols) = first_paren(&compact) {
            push_uq(out, table, &parse_ident_list(&cols), None);
        }
        return;
    }
    if upper.starts_with("FOREIGN KEY") {
        push_fk_clause(out, table, &compact, None);
        return;
    }
    if upper.starts_with("CHECK") || upper.starts_with("EXCLUDE") {
        return;
    }

    let Some(column) = compact.split_whitespace().next() else {
        return;
    };
    let column = unquote(column);
    if column.is_empty() {
        return;
    }
    if upper.contains(" PRIMARY KEY") {
        push_pk(out, table, &[column.clone()], None);
    }
    if upper.contains(" UNIQUE") {
        push_uq(out, table, &[column.clone()], None);
    }
    if upper.contains(" REFERENCES ") {
        push_column_fk(out, table, &column, &compact);
    }
}

fn push_named_constraint(out: &mut Vec<ExpectedConstraint>, table: &str, compact: &str) {
    let mut parts = compact.splitn(3, char::is_whitespace);
    let _keyword = parts.next();
    let Some(name_raw) = parts.next() else {
        return;
    };
    let name = unquote(name_raw);
    let rest = parts.next().unwrap_or("").trim().trim_end_matches(',');
    let upper = rest.to_ascii_uppercase();
    if upper.starts_with("UNIQUE") {
        if let Some(cols) = first_paren(rest) {
            push_uq(out, table, &parse_ident_list(&cols), Some(&name));
        }
    } else if upper.starts_with("PRIMARY KEY") {
        if let Some(cols) = first_paren(rest) {
            push_pk(out, table, &parse_ident_list(&cols), Some(&name));
        }
    } else if upper.starts_with("FOREIGN KEY") {
        push_fk_clause(out, table, rest, Some(&name));
    } else if upper.starts_with("CHECK") {
        let id = format!("{table}|ck|{name}");
        let sql = format!(
            "ALTER TABLE {} ADD CONSTRAINT {} {rest};",
            quote_ident(table),
            quote_ident(&name)
        );
        out.push(ExpectedConstraint {
            id,
            table: table.to_string(),
            name: name.clone(),
            kind: "check",
            sql,
            check_expr: Some(rest.to_string()),
        });
    }
}

fn push_pk(out: &mut Vec<ExpectedConstraint>, table: &str, cols: &[String], name: Option<&str>) {
    if cols.is_empty() {
        return;
    }
    let col_sql = quote_list(cols);
    let sql = match name {
        Some(name) => format!(
            "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY ({col_sql});",
            quote_ident(table),
            quote_ident(name)
        ),
        None => format!(
            "ALTER TABLE {} ADD PRIMARY KEY ({col_sql});",
            quote_ident(table)
        ),
    };
    out.push(ExpectedConstraint {
        id: format!("{table}|pk|{}", cols.join(",")),
        table: table.to_string(),
        name: name
            .map(str::to_string)
            .unwrap_or_else(|| format!("{table}_pkey")),
        kind: "primary-key",
        sql,
        check_expr: None,
    });
}

fn push_uq(out: &mut Vec<ExpectedConstraint>, table: &str, cols: &[String], name: Option<&str>) {
    if cols.is_empty() {
        return;
    }
    let col_sql = quote_list(cols);
    let sql = match name {
        Some(name) => format!(
            "ALTER TABLE {} ADD CONSTRAINT {} UNIQUE ({col_sql});",
            quote_ident(table),
            quote_ident(name)
        ),
        None => format!("ALTER TABLE {} ADD UNIQUE ({col_sql});", quote_ident(table)),
    };
    out.push(ExpectedConstraint {
        id: format!("{table}|uq|{}", cols.join(",")),
        table: table.to_string(),
        name: name
            .map(str::to_string)
            .unwrap_or_else(|| format!("{table}_{}_key", cols.join("_"))),
        kind: "unique",
        sql,
        check_expr: None,
    });
}

fn push_fk_clause(
    out: &mut Vec<ExpectedConstraint>,
    table: &str,
    clause: &str,
    name: Option<&str>,
) {
    let Some(local) = first_paren(clause) else {
        return;
    };
    let cols = parse_ident_list(&local);
    let after_local = clause
        .find(')')
        .map(|pos| clause[pos + 1..].trim())
        .unwrap_or("");
    push_fk(out, table, &cols, after_local, name);
}

fn push_column_fk(out: &mut Vec<ExpectedConstraint>, table: &str, column: &str, compact: &str) {
    let upper = compact.to_ascii_uppercase();
    let Some(pos) = upper.find(" REFERENCES ") else {
        return;
    };
    push_fk(
        out,
        table,
        &[column.to_string()],
        compact[pos..].trim(),
        None,
    );
}

fn push_fk(
    out: &mut Vec<ExpectedConstraint>,
    table: &str,
    cols: &[String],
    references_clause: &str,
    name: Option<&str>,
) {
    let re = regex::Regex::new(
        r#"(?i)REFERENCES\s+("([^"]+)"|([A-Za-z_][A-Za-z0-9_]*))\s*\(([^)]*)\)(.*)$"#,
    )
    .unwrap();
    let Some(caps) = re.captures(references_clause) else {
        return;
    };
    let ref_table = caps
        .get(2)
        .or_else(|| caps.get(3))
        .map(|item| item.as_str())
        .unwrap_or("");
    let ref_cols = parse_ident_list(caps.get(4).map(|item| item.as_str()).unwrap_or(""));
    let tail = caps.get(5).map(|item| item.as_str()).unwrap_or("");
    let (update, delete) = referential_actions(tail);
    let col_sql = quote_list(cols);
    let ref_sql = quote_list(&ref_cols);
    let constraint = match name {
        Some(name) => format!("CONSTRAINT {} ", quote_ident(name)),
        None => String::new(),
    };
    out.push(ExpectedConstraint {
        id: format!(
            "{table}|fk|{}->{}|{}|u={update}|d={delete}",
            cols.join(","),
            ref_table,
            ref_cols.join(",")
        ),
        table: table.to_string(),
        name: name.map(str::to_string).unwrap_or_else(|| {
            format!(
                "{table}_{}_fkey",
                cols.first().map(String::as_str).unwrap_or("fkey")
            )
        }),
        kind: "foreign-key",
        sql: format!(
            "ALTER TABLE {} ADD {constraint}FOREIGN KEY ({col_sql}) REFERENCES {} ({ref_sql}) ON UPDATE {update} ON DELETE {delete};",
            quote_ident(table),
            quote_ident(ref_table)
        ),
        check_expr: None,
    });
}

fn referential_actions(tail: &str) -> (String, String) {
    (
        action_value(tail, "UPDATE").unwrap_or_else(|| "NO ACTION".to_string()),
        action_value(tail, "DELETE").unwrap_or_else(|| "NO ACTION".to_string()),
    )
}

fn action_value(tail: &str, which: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(
        r"(?i)ON\s+{which}\s+(CASCADE|RESTRICT|SET\s+NULL|SET\s+DEFAULT|NO\s+ACTION)"
    ))
    .unwrap();
    re.captures(tail).map(|caps| {
        caps[1]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_uppercase()
    })
}

fn split_top_level(input: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut quote = false;
    for ch in input.chars() {
        if ch == '\'' {
            quote = !quote;
            current.push(ch);
            continue;
        }
        if !quote {
            if ch == '(' {
                depth += 1;
            } else if ch == ')' {
                depth -= 1;
            } else if ch == sep && depth == 0 {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
                current.clear();
                continue;
            }
        }
        current.push(ch);
    }
    let trimmed = current.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out
}

fn first_paren(text: &str) -> Option<String> {
    let start = text.find('(')?;
    let mut depth = 0i32;
    for (offset, ch) in text[start..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start + 1..start + offset].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_ident_list(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(|part| unquote(part.trim()))
        .filter(|part| !part.is_empty())
        .collect()
}

fn unquote(token: &str) -> String {
    token.trim().trim_matches('"').trim_matches(',').to_string()
}

fn quote_list(names: &[String]) -> String {
    names
        .iter()
        .map(|name| quote_ident(name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn constraint_id_from_db(table: &str, name: &str, kind: &str, def: &str) -> Option<String> {
    match kind {
        "p" => {
            let cols = parse_ident_list(&first_paren(def)?);
            Some(format!("{table}|pk|{}", cols.join(",")))
        }
        "u" => {
            let cols = parse_ident_list(&first_paren(def)?);
            Some(format!("{table}|uq|{}", cols.join(",")))
        }
        "c" => Some(format!("{table}|ck|{name}")),
        "f" => {
            let local = parse_ident_list(&first_paren(def)?);
            let after = def.find(')').map(|pos| &def[pos + 1..]).unwrap_or("");
            let re = regex::Regex::new(
                r#"(?i)REFERENCES\s+("([^"]+)"|([A-Za-z_][A-Za-z0-9_]*))\s*\(([^)]*)\)(.*)$"#,
            )
            .unwrap();
            let caps = re.captures(after)?;
            let ref_table = caps
                .get(2)
                .or_else(|| caps.get(3))
                .map(|item| item.as_str())
                .unwrap_or("");
            let ref_cols = parse_ident_list(caps.get(4).map(|item| item.as_str()).unwrap_or(""));
            let tail = caps.get(5).map(|item| item.as_str()).unwrap_or("");
            let (update, delete) = referential_actions(tail);
            Some(format!(
                "{table}|fk|{}->{}|{}|u={update}|d={delete}",
                local.join(","),
                ref_table,
                ref_cols.join(",")
            ))
        }
        _ => None,
    }
}

async fn constraint_drift(
    pool: &Pool<Postgres>,
    actual_tables: &HashSet<String>,
) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
    let expected_rows = baseline_constraints();
    let mut expected: HashMap<String, ExpectedConstraint> = HashMap::new();
    for row in expected_rows {
        expected.entry(row.id.clone()).or_insert(row);
    }
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"
        SELECT c.relname, con.conname, con.contype::text, pg_get_constraintdef(con.oid, true)
        FROM pg_constraint con
        JOIN pg_class c ON c.oid = con.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND c.relkind = 'r'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let known_tables = expected_tables();
    let mut actual_ids = HashSet::new();
    let mut lines = Vec::new();
    let mut sql = Vec::new();
    for (table, name, kind, def) in &rows {
        if IGNORED_EXTRA_TABLES.contains(&table.as_str()) || !known_tables.contains(table) {
            continue;
        }
        let Some(id) = constraint_id_from_db(table, name, kind, def) else {
            continue;
        };
        if expected
            .get(&id)
            .is_some_and(|item| check_expressions_match(item, def))
        {
            actual_ids.insert(id);
        } else {
            lines.push(drift_line(
                "ConstraintDrop",
                &format!(
                    "The constraint \"{table}\".\"{name}\" ({}) exists but should be removed",
                    constraint_kind_name(kind)
                ),
            ));
            sql.push(format!(
                "ALTER TABLE {} DROP CONSTRAINT {};",
                quote_ident(table),
                quote_ident(name)
            ));
        }
    }

    for (id, constraint) in &expected {
        if !actual_tables.contains(&constraint.table) || actual_ids.contains(id) {
            continue;
        }
        lines.push(drift_line(
            "ConstraintAdd",
            &format!(
                "The constraint \"{}\".\"{}\" ({}) is missing and needs to be created",
                constraint.table, constraint.name, constraint.kind
            ),
        ));
        sql.push(constraint.sql.clone());
    }
    Ok((lines, sql))
}

async fn catalog_drift(
    pool: &Pool<Postgres>,
    actual_tables: &HashSet<String>,
) -> Result<(Vec<String>, Vec<String>, Vec<String>, Vec<String>), sqlx::Error> {
    let catalog = baseline_catalog();
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"
        SELECT table_name, column_name, data_type, udt_name
        FROM information_schema.columns
        WHERE table_schema = 'public'
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut actual: HashMap<String, HashMap<String, String>> = HashMap::new();
    for (table, column, data_type, udt_name) in rows {
        actual
            .entry(table)
            .or_default()
            .insert(column, normalize_db_type(&data_type, &udt_name));
    }
    let vector_rows: Vec<(String, String, i32)> = sqlx::query_as(
        r#"
        SELECT c.relname, a.attname, a.atttypmod
        FROM pg_attribute a
        JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        WHERE n.nspname = 'public'
          AND t.typname = 'vector'
          AND a.attnum > 0
          AND NOT a.attisdropped
          AND a.atttypmod > 0
        "#,
    )
    .fetch_all(pool)
    .await?;
    for (table, column, typmod) in vector_rows {
        if let Some(columns) = actual.get_mut(&table) {
            if columns.get(&column).is_some_and(|ty| ty == "vector") {
                columns.insert(column, format!("vector({typmod})"));
            }
        }
    }

    let mut missing = Vec::new();
    let mut extra = Vec::new();
    let mut lines = Vec::new();
    let mut sql = Vec::new();
    for (table, columns) in &catalog.column_types {
        if !actual_tables.contains(table) {
            continue;
        }
        let present = actual.get(table).cloned().unwrap_or_default();
        for (column, expected_type) in columns {
            match present.get(column) {
                None => {
                    missing.push(format!("{table}.{column}"));
                    sql.push(format!(
                        "ALTER TABLE {} ADD COLUMN {} {expected_type};",
                        quote_ident(table),
                        quote_ident(column)
                    ));
                }
                Some(actual_type) if actual_type != expected_type => {
                    lines.push(drift_line(
                        "ColumnDrop",
                        &format!(
                            "The column \"{table}\".\"{column}\" exists but should be removed"
                        ),
                    ));
                    lines.push(drift_line(
                        "ColumnAdd",
                        &format!(
                            "The column \"{table}\".\"{column}\" is missing and needs to be created"
                        ),
                    ));
                    sql.push(format!(
                        "ALTER TABLE {} DROP COLUMN {};",
                        quote_ident(table),
                        quote_ident(column)
                    ));
                    sql.push(format!(
                        "ALTER TABLE {} ADD {} {expected_type};",
                        quote_ident(table),
                        quote_ident(column)
                    ));
                }
                Some(_) => {}
            }
        }
        for column in present.keys() {
            if !columns.contains_key(column) {
                extra.push(format!("{table}.{column}"));
            }
        }
    }

    let index_rows: Vec<(String,)> =
        sqlx::query_as(r#"SELECT indexname FROM pg_indexes WHERE schemaname = 'public'"#)
            .fetch_all(pool)
            .await?;
    let index_names: HashSet<String> = index_rows.into_iter().map(|(name,)| name).collect();
    for (name, statement) in &catalog.indexes {
        if !index_names.contains(name) {
            let table = index_table(statement).unwrap_or_else(|| name.clone());
            lines.push(drift_line(
                "IndexCreate",
                &format!("The index \"{table}\".\"{name}\" is missing and needs to be created"),
            ));
            sql.push(statement.clone());
        }
    }

    let enum_rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT t.typname, e.enumlabel
        FROM pg_type t
        JOIN pg_enum e ON e.enumtypid = t.oid
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE n.nspname = 'public'
        ORDER BY t.typname, e.enumsortorder
        "#,
    )
    .fetch_all(pool)
    .await?;
    let mut actual_enums: HashMap<String, Vec<String>> = HashMap::new();
    for (name, label) in enum_rows {
        actual_enums.entry(name).or_default().push(label);
    }
    for (name, labels) in &catalog.enums {
        match actual_enums.get(name) {
            None => {
                let quoted = labels
                    .iter()
                    .map(|label| format!("'{label}'"))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(drift_line(
                    "EnumCreate",
                    &format!("The enum \"{name}\" is missing and needs to be created"),
                ));
                sql.push(format!("CREATE TYPE {name} AS ENUM ({quoted});"));
            }
            Some(actual_labels) => {
                for label in labels {
                    if !actual_labels.contains(label) {
                        lines.push(drift_line(
                            "EnumDrop",
                            &format!("The enum \"{name}\" exists but is no longer needed"),
                        ));
                        lines.push(drift_line(
                            "EnumCreate",
                            &format!("The enum \"{name}\" is missing and needs to be created"),
                        ));
                        sql.push(format!("DROP TYPE {};", quote_ident(name)));
                        sql.push(format!(
                            "CREATE TYPE {name} AS ENUM ({quoted_labels});",
                            quoted_labels = labels
                                .iter()
                                .map(|label| format!("'{label}'"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
            }
        }
    }

    let function_rows: Vec<(String, String)> = sqlx::query_as(
        r#"
        SELECT p.proname, pg_get_function_identity_arguments(p.oid)
        FROM pg_proc p
        JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE n.nspname = 'public'
          AND p.prokind = 'f'
          AND NOT EXISTS (
            SELECT 1 FROM pg_depend d WHERE d.objid = p.oid AND d.deptype = 'e'
          )
        "#,
    )
    .fetch_all(pool)
    .await?;
    for name in &catalog.functions {
        let expected = catalog
            .function_args
            .get(name)
            .map(String::as_str)
            .unwrap_or("");
        let matched = function_rows
            .iter()
            .any(|(actual_name, args)| actual_name == name && function_args_match(expected, args));
        if !matched {
            lines.push(drift_line(
                "FunctionCreate",
                &format!("The function \"{name}\" is missing and needs to be created"),
            ));
            if let Some(statement) = catalog.function_sql.get(name) {
                sql.push(statement.clone());
            }
        }
    }
    for (name, args) in &function_rows {
        let expected = catalog.function_args.get(name).map(String::as_str);
        let known = expected.is_some_and(|expected| function_args_match(expected, args));
        if !known {
            lines.push(drift_line(
                "FunctionDrop",
                &format!("The function \"{name}\" exists but should be removed"),
            ));
            sql.push(format!(
                "DROP FUNCTION IF EXISTS {}({args});",
                quote_ident(name)
            ));
        }
    }

    let trigger_rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT t.tgname
        FROM pg_trigger t
        JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
          AND NOT t.tgisinternal
        "#,
    )
    .fetch_all(pool)
    .await?;
    let trigger_names: HashSet<String> = trigger_rows.into_iter().map(|(name,)| name).collect();
    for name in &catalog.triggers {
        if !trigger_names.contains(name) {
            let statement = catalog
                .trigger_sql
                .get(name)
                .map(String::as_str)
                .unwrap_or("");
            let table = trigger_table(statement).unwrap_or_else(|| name.clone());
            lines.push(drift_line(
                "TriggerCreate",
                &format!("The trigger \"{table}\".\"{name}\" is missing and needs to be created"),
            ));
            if !statement.is_empty() {
                sql.push(statement.to_string());
            }
        }
    }

    let (constraint_lines, constraint_sql) = constraint_drift(pool, actual_tables).await?;
    lines.extend(constraint_lines);
    sql.extend(constraint_sql);

    let installed: Vec<(String,)> = sqlx::query_as(r#"SELECT extname FROM pg_extension"#)
        .fetch_all(pool)
        .await?;
    let installed: HashSet<String> = installed.into_iter().map(|(name,)| name).collect();
    for name in expected_extensions() {
        if !installed.contains(&name) {
            lines.push(drift_line(
                "ExtensionCreate",
                &format!("The extension \"{name}\" is missing and needs to be created"),
            ));
            sql.push(format!("CREATE EXTENSION IF NOT EXISTS \"{name}\";"));
        }
    }

    missing.sort();
    extra.sort();
    lines.sort();
    Ok((missing, extra, lines, sql))
}

async fn list_public_tables(pool: &Pool<Postgres>) -> Result<HashSet<String>, sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(name,)| name).collect())
}

async fn extension_installed(pool: &Pool<Postgres>, name: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM pg_extension WHERE extname = $1
        )
        "#,
    )
    .bind(name)
    .fetch_one(pool)
    .await
}

async fn kysely_migration_status(
    pool: &Pool<Postgres>,
) -> Result<Option<Vec<MigrationCheck>>, sqlx::Error> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM information_schema.tables
            WHERE table_schema = 'public'
              AND table_name = 'kysely_migrations'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    if !exists {
        return Ok(None);
    }

    let applied: Vec<(String,)> =
        sqlx::query_as(r#"SELECT name FROM kysely_migrations ORDER BY name ASC"#)
            .fetch_all(pool)
            .await?;

    let applied_names = applied.into_iter().map(|(name,)| name).collect::<Vec<_>>();
    Ok(Some(compare_migrations(
        expected_migration_names(),
        &applied_names,
    )))
}

pub fn print_report(report: &SchemaCheckReport) -> bool {
    let mut ok = true;

    match &report.migrations {
        Some(migrations)
            if migrations
                .iter()
                .all(|m| m.status == MigrationStatus::Applied) =>
        {
            println!("Migrations are up to date");
        }
        Some(migrations) => {
            ok = false;
            println!("Migration issues detected:");
            for migration in migrations {
                match migration.status {
                    MigrationStatus::Applied => {}
                    MigrationStatus::Missing => {
                        println!(
                            "  - {} exists on disk, but has not been applied to the database",
                            migration.name
                        );
                    }
                    MigrationStatus::Deleted => {
                        println!(
                            "  - {} was applied, but the file no longer exists on disk",
                            migration.name
                        );
                    }
                }
            }
        }
        None => {
            println!("No kysely_migrations table (sqlx-baseline / fresh database)");
            if !expected_migration_names().is_empty() {
                println!(
                    "  Lock tracks {} upstream Kysely name(s) fused into sqlx baseline",
                    expected_migration_names().len()
                );
            }
        }
    }

    let mut lines = Vec::new();
    for table in &report.missing_tables {
        lines.push(drift_line(
            "TableCreate",
            &format!("The table \"{table}\" is missing and needs to be created"),
        ));
    }
    for column in &report.missing_columns {
        let (table, name) = column.split_once('.').unwrap_or((column.as_str(), ""));
        lines.push(drift_line(
            "ColumnAdd",
            &format!("The column \"{table}\".\"{name}\" is missing and needs to be created"),
        ));
    }
    lines.extend(report.drift_lines.iter().cloned());
    if lines.is_empty() {
        println!("\nNo schema drift detected");
    } else {
        ok = false;
        println!(
            "\nDetected schema drift. For more information, see https://docs.immich.app/errors#schema-drift"
        );
        for line in &lines {
            println!("  - {line}");
        }
        if !report.repair_sql.is_empty() {
            println!(
                "\nThe below SQL is automatically generated and may be helpful for resolving drift. ** Use at your own risk! **\n"
            );
            println!("```sql");
            for statement in &report.repair_sql {
                println!("{statement}");
            }
            println!("```");
        }
    }

    if report.optional_missing.is_empty() {
        println!("\nOptional pgvector tables: present");
    } else {
        println!(
            "\nOptional pgvector tables missing: {}",
            report.optional_missing.join(", ")
        );
        if !report.vector_extension {
            println!("  (pgvector extension is not installed)");
        }
    }

    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_expected_tables_from_baseline_sql() {
        let tables = expected_tables();
        assert!(tables.contains("user"));
        assert!(tables.contains("asset"));
        assert!(tables.contains("cluster_group"));
        assert!(tables.contains("person_group"));
        assert!(tables.contains("workflow_log"));
        assert!(tables.contains("smart_search"));
        assert!(tables.contains("face_search"));
        assert!(tables.len() >= 60);
    }

    #[test]
    fn parses_album_types_indexes_and_enums() {
        let catalog = baseline_catalog();
        let album = catalog.column_types.get("album").expect("album");
        assert_eq!(
            album.get("albumName").map(String::as_str),
            Some("character varying")
        );
        assert_eq!(
            catalog
                .column_types
                .get("smart_search")
                .and_then(|columns| columns.get("embedding"))
                .map(String::as_str),
            Some("vector(512)")
        );
        assert!(catalog.indexes.contains_key("user_updated_at_id_idx"));
        assert_eq!(
            catalog.enums.get("album_user_role_enum").map(Vec::as_slice),
            Some(
                [
                    "owner".to_string(),
                    "editor".to_string(),
                    "viewer".to_string()
                ]
                .as_slice()
            )
        );
        assert_eq!(
            catalog
                .function_args
                .get("immich_uuid_v7")
                .map(String::as_str),
            Some("timestamp with time zone")
        );
        assert_eq!(
            catalog.function_args.get("f_concat_ws").map(String::as_str),
            Some("text, text[]")
        );
        assert_eq!(
            catalog
                .function_args
                .get("ll_to_earth_public")
                .map(String::as_str),
            Some("double precision, double precision")
        );
        assert_eq!(
            catalog.function_args.get("updated_at").map(String::as_str),
            Some("")
        );
        assert!(
            catalog
                .function_sql
                .get("updated_at")
                .is_some_and(|sql| sql.contains("RETURN NEW"))
        );
        assert!(catalog.triggers.contains("cluster_group_updatedAt"));
        let extensions = expected_extensions();
        assert!(extensions.contains("uuid-ossp"));
        assert!(extensions.contains("pg_trgm"));
        assert!(!extensions.contains("vector"));
        assert!(!extensions.contains("vchord"));
    }

    #[test]
    fn parses_constraints_from_baseline() {
        let constraints = baseline_constraints();
        let ids: HashSet<_> = constraints.iter().map(|item| item.id.as_str()).collect();
        assert!(ids.contains("activity|ck|activity_like_check"));
        assert!(ids.contains("person|ck|person_birth_date_chk"));
        assert!(ids.contains("album_user|pk|albumId,userId"));
        assert!(ids.contains("user|uq|email"));
        assert!(ids.contains("user|fk|clusterGroupId->cluster_group|id|u=CASCADE|d=NO ACTION"));
        assert!(ids.contains("asset_face|fk|personGroupId->person_group|id|u=CASCADE|d=SET NULL"));
    }

    #[test]
    fn pg_constraint_def_matches_baseline_identity() {
        let fk = constraint_id_from_db(
            "user",
            "user_clusterGroupId_fkey",
            "f",
            r#"FOREIGN KEY ("clusterGroupId") REFERENCES cluster_group(id) ON UPDATE CASCADE"#,
        )
        .unwrap();
        assert_eq!(
            fk,
            "user|fk|clusterGroupId->cluster_group|id|u=CASCADE|d=NO ACTION"
        );
        let pk = constraint_id_from_db(
            "album_user",
            "album_user_pkey",
            "p",
            r#"PRIMARY KEY ("albumId", "userId")"#,
        )
        .unwrap();
        assert_eq!(pk, "album_user|pk|albumId,userId");
    }

    #[test]
    fn parses_album_columns_from_baseline_sql() {
        let columns = expected_columns();
        let album = columns.get("album").expect("album");
        assert!(album.contains("albumName"));
        assert!(album.contains("ownerId") || album.contains("description"));
        assert!(!album.contains("PRIMARY"));
    }

    #[test]
    fn loads_kysely_migration_names() {
        assert!(!expected_migration_names().is_empty());
        assert!(
            expected_migration_names()
                .iter()
                .any(|name| name.contains("InitialMigration"))
        );
    }

    #[test]
    fn compare_migrations_detects_missing_and_deleted() {
        let expected = ["100-First", "200-Second"];
        let applied = vec!["100-First".to_string(), "300-Old".to_string()];
        let result = compare_migrations(&expected, &applied);

        assert_eq!(
            result
                .iter()
                .find(|item| item.name == "200-Second")
                .map(|item| item.status),
            Some(MigrationStatus::Missing)
        );
        assert_eq!(
            result
                .iter()
                .find(|item| item.name == "300-Old")
                .map(|item| item.status),
            Some(MigrationStatus::Deleted)
        );
        assert_eq!(
            result
                .iter()
                .find(|item| item.name == "100-First")
                .map(|item| item.status),
            Some(MigrationStatus::Applied)
        );
    }

    #[test]
    fn bookkeeping_tables_are_ignored_as_extra() {
        assert!(IGNORED_EXTRA_TABLES.contains(&"_sqlx_migrations"));
        assert!(IGNORED_EXTRA_TABLES.contains(&"migration_overrides"));
    }
}
