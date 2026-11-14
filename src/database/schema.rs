use serde::{Deserialize, Deserializer};
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display, Formatter};

pub(crate) const EMBEDDED_SCHEMA: &str = include_str!("../../magia/schema.yaml");

/// An invalid embedded schema or an incompatible existing definition.
#[derive(Debug)]
pub enum SchemaError {
    /// The YAML does not match the supported schema format.
    Parse(serde_yaml::Error),
    /// The parsed schema violates a validation rule.
    Invalid(String),
    /// An existing column cannot be reconciled without altering it.
    IncompatibleColumn {
        /// Name of the table containing the column.
        table: String,
        /// Name of the incompatible column.
        column: String,
        /// Difference between the existing and requested definitions.
        reason: String,
    },
    /// An existing constraint cannot be reconciled without altering it.
    IncompatibleConstraint {
        /// Name of the table containing the constraint.
        table: String,
        /// Name of the incompatible constraint.
        constraint: String,
        /// Difference between the existing and requested definitions.
        reason: String,
    },
}

impl Display for SchemaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "could not parse the embedded schema: {error}"),
            Self::Invalid(reason) => write!(formatter, "invalid schema: {reason}"),
            Self::IncompatibleConstraint {
                table,
                constraint,
                reason,
            } => write!(
                formatter,
                "constraint '{table}.{constraint}' is incompatible with the requested schema: {reason}"
            ),
            Self::IncompatibleColumn {
                table,
                column,
                reason,
            } => write!(
                formatter,
                "column '{table}.{column}' is incompatible with the requested schema: {reason}"
            ),
        }
    }
}

impl std::error::Error for SchemaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_)
            | Self::IncompatibleColumn { .. }
            | Self::IncompatibleConstraint { .. } => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Schema {
    schema_version: u32,
    name: String,
    description: String,
    conventions: Conventions,
    tables: Vec<Table>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Conventions {
    naming: NamingConvention,
    identity_generation: IdentityConvention,
    default_nullable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NamingConvention {
    SnakeCase,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum IdentityConvention {
    Database,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Table {
    name: Identifier,
    columns: Vec<Column>,
    primary_key: Vec<Identifier>,
    #[serde(default)]
    constraints: Vec<ForeignKey>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Column {
    name: Identifier,
    #[serde(rename = "type")]
    data_type: DataType,
    #[serde(default)]
    generated: Option<Generated>,
    #[serde(default)]
    nullable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Generated {
    Identity,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DataType {
    BigInteger,
    Text,
    Binary,
}

impl DataType {
    fn sql(self) -> &'static str {
        match self {
            Self::BigInteger => "bigint",
            Self::Text => "text",
            Self::Binary => "bytea",
        }
    }

    fn postgres_name(self) -> &'static str {
        match self {
            Self::BigInteger => "bigint",
            Self::Text => "text",
            Self::Binary => "bytea",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ForeignKey {
    name: Identifier,
    #[serde(rename = "type")]
    kind: ForeignKeyKind,
    columns: Vec<Identifier>,
    references: Reference,
    on_delete: ReferentialAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ForeignKeyKind {
    ForeignKey,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    table: Identifier,
    columns: Vec<Identifier>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReferentialAction {
    Restrict,
    Cascade,
}

impl Display for ReferentialAction {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Restrict => formatter.write_str("RESTRICT"),
            Self::Cascade => formatter.write_str("CASCADE"),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct Identifier(String);

impl Identifier {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, SchemaError> {
        let value = value.into();
        let mut characters = value.chars();
        let has_valid_start = characters
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character == '_');
        let has_valid_characters = characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        });

        if !has_valid_start || !has_valid_characters || value.len() > 63 {
            return Err(SchemaError::Invalid(format!(
                "identifier '{value}' must be a lowercase PostgreSQL identifier of at most 63 bytes"
            )));
        }

        Ok(Self(value))
    }

    pub(crate) fn quoted(&self) -> String {
        format!("\"{}\"", self.0)
    }
}

impl Display for Identifier {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug)]
pub(crate) struct ColumnState {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) is_nullable: bool,
    pub(crate) identity_generation: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ConstraintState {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) columns: Vec<String>,
    pub(crate) referenced_table: String,
    pub(crate) referenced_columns: Vec<String>,
    pub(crate) on_delete: String,
    pub(crate) validated: bool,
}

#[derive(Debug, Default)]
pub(crate) struct DatabaseState {
    pub(crate) tables: HashMap<String, Vec<ColumnState>>,
    pub(crate) constraints: HashMap<String, Vec<ConstraintState>>,
}

impl Schema {
    pub(crate) fn parse(yaml: &str) -> Result<Self, SchemaError> {
        let schema: Self = serde_yaml::from_str(yaml).map_err(SchemaError::Parse)?;
        schema.validate()?;
        Ok(schema)
    }

    pub(crate) fn version(&self) -> Result<i32, SchemaError> {
        i32::try_from(self.schema_version).map_err(|_| {
            SchemaError::Invalid("schema_version exceeds PostgreSQL integer range".to_owned())
        })
    }

    fn validate(&self) -> Result<(), SchemaError> {
        Identifier::new(self.name.clone())?;
        if self.description.trim().is_empty() {
            return Err(SchemaError::Invalid(
                "schema description cannot be empty".to_owned(),
            ));
        }
        if self.schema_version == 0 {
            return Err(SchemaError::Invalid(
                "schema_version must be greater than zero".to_owned(),
            ));
        }

        match self.conventions.naming {
            NamingConvention::SnakeCase => {}
        }
        match self.conventions.identity_generation {
            IdentityConvention::Database => {}
        }

        let mut table_names = HashSet::new();
        for table in &self.tables {
            if !table_names.insert(&table.name) {
                return Err(SchemaError::Invalid(format!(
                    "schema contains duplicate table '{}'",
                    table.name
                )));
            }
            validate_table(table, self.conventions.default_nullable)?;
        }

        for table in &self.tables {
            for foreign_key in &table.constraints {
                let referenced_table = self
                    .tables
                    .iter()
                    .find(|candidate| candidate.name == foreign_key.references.table)
                    .ok_or_else(|| {
                        SchemaError::Invalid(format!(
                            "foreign key '{}' refers to unknown table '{}'",
                            foreign_key.name, foreign_key.references.table
                        ))
                    })?;
                for column in &foreign_key.references.columns {
                    if !referenced_table
                        .columns
                        .iter()
                        .any(|candidate| candidate.name == *column)
                    {
                        return Err(SchemaError::Invalid(format!(
                            "foreign key '{}' refers to unknown column '{}.{column}'",
                            foreign_key.name, referenced_table.name
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) fn migration_statements(
        &self,
        state: &DatabaseState,
    ) -> Result<Vec<String>, SchemaError> {
        let mut statements = Vec::new();

        for table in &self.tables {
            if let Some(columns) = state.tables.get(table.name.as_str()) {
                statements
                    .extend(table.add_missing_columns(columns, self.conventions.default_nullable)?);
                statements.extend(table.add_missing_constraints(state)?);
            } else {
                statements.push(table.create(self.conventions.default_nullable)?);
            }
        }

        Ok(statements)
    }
}

impl Identifier {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_table(table: &Table, default_nullable: bool) -> Result<(), SchemaError> {
    if table.columns.is_empty() {
        return Err(SchemaError::Invalid(format!(
            "table '{}' must contain at least one column",
            table.name
        )));
    }

    let column_names: HashSet<_> = table.columns.iter().map(|column| &column.name).collect();
    if column_names.len() != table.columns.len() {
        return Err(SchemaError::Invalid(format!(
            "table '{}' contains duplicate columns",
            table.name
        )));
    }

    for column in &table.columns {
        if matches!(column.generated, Some(Generated::Identity))
            && !matches!(column.data_type, DataType::BigInteger)
        {
            return Err(SchemaError::Invalid(format!(
                "identity column '{}.{}' must use big_integer",
                table.name, column.name
            )));
        }
    }

    for column in &table.primary_key {
        if !column_names.contains(column) {
            return Err(SchemaError::Invalid(format!(
                "primary key of '{}' refers to unknown column '{column}'",
                table.name
            )));
        }
        let definition = table
            .columns
            .iter()
            .find(|candidate| &candidate.name == column)
            .expect("primary key membership was checked");
        if definition.nullable.unwrap_or(default_nullable) {
            return Err(SchemaError::Invalid(format!(
                "primary key column '{}.{column}' cannot be nullable",
                table.name
            )));
        }
    }

    let mut constraint_names = HashSet::new();
    for foreign_key in &table.constraints {
        if !constraint_names.insert(&foreign_key.name) {
            return Err(SchemaError::Invalid(format!(
                "table '{}' contains duplicate constraint '{}'",
                table.name, foreign_key.name
            )));
        }
        if foreign_key.columns.is_empty()
            || foreign_key.columns.len() != foreign_key.references.columns.len()
        {
            return Err(SchemaError::Invalid(format!(
                "foreign key '{}' must reference the same number of columns",
                foreign_key.name
            )));
        }
        if foreign_key
            .columns
            .iter()
            .any(|column| !column_names.contains(column))
        {
            return Err(SchemaError::Invalid(format!(
                "foreign key '{}' refers to an unknown local column",
                foreign_key.name
            )));
        }
        match foreign_key.kind {
            ForeignKeyKind::ForeignKey => {}
        }
    }

    Ok(())
}

impl Table {
    fn qualified_name(&self) -> String {
        format!("public.{}", self.name.quoted())
    }

    fn primary_key_name(&self) -> Result<Identifier, SchemaError> {
        Identifier::new(format!("{}_primary_key", self.name.as_str()))
    }

    fn create(&self, default_nullable: bool) -> Result<String, SchemaError> {
        let mut definitions = self
            .columns
            .iter()
            .map(|column| column.definition(default_nullable))
            .collect::<Vec<_>>();

        if !self.primary_key.is_empty() {
            definitions.push(format!(
                "CONSTRAINT {} PRIMARY KEY ({})",
                self.primary_key_name()?.quoted(),
                quoted_identifiers(&self.primary_key)
            ));
        }
        definitions.extend(self.constraints.iter().map(ForeignKey::definition));

        Ok(format!(
            "CREATE TABLE {} ({})",
            self.qualified_name(),
            definitions.join(", ")
        ))
    }

    fn add_missing_columns(
        &self,
        existing: &[ColumnState],
        default_nullable: bool,
    ) -> Result<Vec<String>, SchemaError> {
        let mut statements = Vec::new();

        for column in &self.columns {
            if let Some(current) = existing
                .iter()
                .find(|current| current.name == column.name.as_str())
            {
                column.ensure_compatible(&self.name, current, default_nullable)?;
            } else {
                statements.push(format!(
                    "ALTER TABLE {} ADD COLUMN {}",
                    self.qualified_name(),
                    column.definition(default_nullable)
                ));
            }
        }

        Ok(statements)
    }

    fn add_missing_constraints(&self, state: &DatabaseState) -> Result<Vec<String>, SchemaError> {
        let existing = state
            .constraints
            .get(self.name.as_str())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut statements = Vec::new();

        if !self.primary_key.is_empty() {
            let name = self.primary_key_name()?;
            if let Some(current) = existing
                .iter()
                .find(|constraint| constraint.name == name.as_str())
            {
                self.ensure_compatible_primary_key(&name, current)?;
            } else {
                if existing.iter().any(|constraint| constraint.kind == "p") {
                    return Err(incompatible_constraint(
                        &self.name,
                        &name,
                        "a primary key with a different name already exists",
                    ));
                }
                statements.push(format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY ({})",
                    self.qualified_name(),
                    name.quoted(),
                    quoted_identifiers(&self.primary_key)
                ));
            }
        }

        for foreign_key in &self.constraints {
            if let Some(current) = existing
                .iter()
                .find(|constraint| constraint.name == foreign_key.name.as_str())
            {
                foreign_key.ensure_compatible(&self.name, current)?;
            } else {
                statements.push(format!(
                    "ALTER TABLE {} ADD {}",
                    self.qualified_name(),
                    foreign_key.definition()
                ));
            }
        }

        Ok(statements)
    }

    fn ensure_compatible_primary_key(
        &self,
        name: &Identifier,
        current: &ConstraintState,
    ) -> Result<(), SchemaError> {
        let expected_columns = identifier_strings(&self.primary_key);
        if current.kind != "p" || current.columns != expected_columns || !current.validated {
            return Err(incompatible_constraint(
                &self.name,
                name,
                "expected a validated primary key over the declared columns",
            ));
        }
        Ok(())
    }
}

impl Column {
    fn definition(&self, default_nullable: bool) -> String {
        let mut definition = format!("{} {}", self.name.quoted(), self.data_type.sql());
        if matches!(self.generated, Some(Generated::Identity)) {
            definition.push_str(" GENERATED BY DEFAULT AS IDENTITY");
        }
        if !self.nullable.unwrap_or(default_nullable) {
            definition.push_str(" NOT NULL");
        }
        definition
    }

    fn ensure_compatible(
        &self,
        table: &Identifier,
        current: &ColumnState,
        default_nullable: bool,
    ) -> Result<(), SchemaError> {
        let expected_identity = matches!(self.generated, Some(Generated::Identity));
        let expected_generation = expected_identity.then_some("BY DEFAULT");
        let expected_nullable = self.nullable.unwrap_or(default_nullable);
        let reason = if current.data_type != self.data_type.postgres_name() {
            Some(format!(
                "expected type '{}', found '{}'",
                self.data_type.postgres_name(),
                current.data_type
            ))
        } else if current.identity_generation.as_deref() != expected_generation {
            Some("identity generation differs".to_owned())
        } else if current.is_nullable != expected_nullable {
            Some("nullability differs".to_owned())
        } else {
            None
        };

        if let Some(reason) = reason {
            return Err(SchemaError::IncompatibleColumn {
                table: table.to_string(),
                column: self.name.to_string(),
                reason,
            });
        }
        Ok(())
    }
}

impl ForeignKey {
    fn definition(&self) -> String {
        format!(
            "CONSTRAINT {} FOREIGN KEY ({}) REFERENCES public.{} ({}) ON DELETE {}",
            self.name.quoted(),
            quoted_identifiers(&self.columns),
            self.references.table.quoted(),
            quoted_identifiers(&self.references.columns),
            self.on_delete
        )
    }

    fn ensure_compatible(
        &self,
        table: &Identifier,
        current: &ConstraintState,
    ) -> Result<(), SchemaError> {
        let expected_columns = identifier_strings(&self.columns);
        let expected_references = identifier_strings(&self.references.columns);
        let expected_table = format!("public.{}", self.references.table.as_str());
        if current.kind != "f"
            || current.columns != expected_columns
            || current.referenced_table != expected_table
            || current.referenced_columns != expected_references
            || current.on_delete != self.on_delete.postgres_code()
            || !current.validated
        {
            return Err(incompatible_constraint(
                table,
                &self.name,
                "expected the declared validated foreign key definition",
            ));
        }
        Ok(())
    }
}

impl ReferentialAction {
    fn postgres_code(self) -> &'static str {
        match self {
            Self::Restrict => "r",
            Self::Cascade => "c",
        }
    }
}

fn identifier_strings(identifiers: &[Identifier]) -> Vec<&str> {
    identifiers.iter().map(Identifier::as_str).collect()
}

fn incompatible_constraint(
    table: &Identifier,
    constraint: &Identifier,
    reason: &str,
) -> SchemaError {
    SchemaError::IncompatibleConstraint {
        table: table.to_string(),
        constraint: constraint.to_string(),
        reason: reason.to_owned(),
    }
}

fn quoted_identifiers(identifiers: &[Identifier]) -> String {
    identifiers
        .iter()
        .map(Identifier::quoted)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_schema_has_the_expected_version_and_tables() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");

        assert_eq!(schema.schema_version, 1);
        assert_eq!(schema.tables.len(), 2);
        assert_eq!(schema.tables[0].name.as_str(), "files");
        assert_eq!(schema.tables[1].name.as_str(), "paths");
    }

    #[test]
    fn creates_tables_with_identity_primary_keys_and_foreign_keys() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");

        let statements = schema
            .migration_statements(&DatabaseState::default())
            .expect("empty database should accept the schema");

        assert_eq!(statements.len(), 2);
        assert!(statements[0].contains(
            "CREATE TABLE public.\"files\" (\"id\" bigint GENERATED BY DEFAULT AS IDENTITY NOT NULL"
        ));
        assert!(statements[0].contains("CONSTRAINT \"files_primary_key\" PRIMARY KEY (\"id\")"));
        assert!(statements[1].contains(
            "CONSTRAINT \"paths_file_foreign_key\" FOREIGN KEY (\"file_id\") REFERENCES public.\"files\" (\"id\") ON DELETE RESTRICT"
        ));
    }

    #[test]
    fn adds_only_missing_columns_to_an_existing_table() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");
        let state = DatabaseState {
            tables: HashMap::from([(
                "files".to_owned(),
                vec![
                    column_state("id", "bigint", false, true),
                    column_state("name", "text", false, false),
                ],
            )]),
            constraints: HashMap::from([(
                "files".to_owned(),
                vec![primary_key_state("files_primary_key", &["id"])],
            )]),
        };

        let statements = schema
            .migration_statements(&state)
            .expect("compatible columns should accept additive changes");

        assert!(statements.contains(
            &("ALTER TABLE public.\"files\" ADD COLUMN \"content\" bytea NOT NULL".to_owned())
        ));
        assert!(
            !statements
                .iter()
                .any(|statement| statement.contains("ADD COLUMN \"name\""))
        );
    }

    #[test]
    fn rejects_an_existing_column_with_a_different_type() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");
        let state = DatabaseState {
            tables: HashMap::from([(
                "files".to_owned(),
                vec![column_state("id", "text", false, true)],
            )]),
            constraints: HashMap::new(),
        };

        let error = schema
            .migration_statements(&state)
            .expect_err("an incompatible column must stop migration");

        assert!(matches!(error, SchemaError::IncompatibleColumn { .. }));
    }

    #[test]
    fn rejects_an_existing_identity_column_with_always_generation() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");
        let mut id = column_state("id", "bigint", false, true);
        id.identity_generation = Some("ALWAYS".to_owned());
        let state = DatabaseState {
            tables: HashMap::from([("files".to_owned(), vec![id])]),
            constraints: HashMap::new(),
        };

        let error = schema
            .migration_statements(&state)
            .expect_err("a different identity mode must stop migration");

        assert!(matches!(error, SchemaError::IncompatibleColumn { .. }));
    }

    #[test]
    fn rejects_an_existing_column_with_different_nullability() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");
        let state = DatabaseState {
            tables: HashMap::from([(
                "files".to_owned(),
                vec![column_state("id", "bigint", true, true)],
            )]),
            constraints: HashMap::new(),
        };

        let error = schema
            .migration_statements(&state)
            .expect_err("different nullability must stop migration");

        assert!(matches!(error, SchemaError::IncompatibleColumn { .. }));
    }

    #[test]
    fn accepts_existing_primary_and_foreign_keys_with_matching_definitions() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");

        let statements = schema
            .migration_statements(&complete_database_state())
            .expect("matching constraints should be compatible");

        assert!(statements.is_empty());
    }

    #[test]
    fn rejects_a_foreign_key_that_references_another_schema() {
        let schema = Schema::parse(EMBEDDED_SCHEMA).expect("embedded schema should be valid");
        let mut state = complete_database_state();
        state
            .constraints
            .get_mut("paths")
            .expect("paths constraints should exist")
            .iter_mut()
            .find(|constraint| constraint.kind == "f")
            .expect("foreign key should exist")
            .referenced_table = "other.files".to_owned();

        let error = schema
            .migration_statements(&state)
            .expect_err("a foreign key to another schema must stop migration");

        assert!(matches!(error, SchemaError::IncompatibleConstraint { .. }));
    }

    #[test]
    fn rejects_schema_version_zero() {
        let yaml = EMBEDDED_SCHEMA.replace("schema_version: 1", "schema_version: 0");

        let error = Schema::parse(&yaml).expect_err("version zero should be invalid");

        assert!(matches!(error, SchemaError::Invalid(_)));
    }

    #[test]
    fn rejects_unknown_schema_fields_instead_of_silently_ignoring_them() {
        let yaml = EMBEDDED_SCHEMA.replace(
            "        generated: identity",
            "        generated: identity\n        default: 1",
        );

        let error = Schema::parse(&yaml).expect_err("unsupported defaults should be rejected");

        assert!(matches!(error, SchemaError::Parse(_)));
    }

    #[test]
    fn rejects_unsafe_identifiers() {
        let error = Identifier::new("files\"; DROP TABLE files; --")
            .expect_err("SQL metacharacters should be rejected");

        assert!(matches!(error, SchemaError::Invalid(_)));
    }

    fn complete_database_state() -> DatabaseState {
        DatabaseState {
            tables: HashMap::from([
                (
                    "files".to_owned(),
                    vec![
                        column_state("id", "bigint", false, true),
                        column_state("name", "text", false, false),
                        column_state("content", "bytea", false, false),
                    ],
                ),
                (
                    "paths".to_owned(),
                    vec![
                        column_state("id", "bigint", false, true),
                        column_state("path", "text", false, false),
                        column_state("file_id", "bigint", false, false),
                    ],
                ),
            ]),
            constraints: HashMap::from([
                (
                    "files".to_owned(),
                    vec![primary_key_state("files_primary_key", &["id"])],
                ),
                (
                    "paths".to_owned(),
                    vec![
                        primary_key_state("paths_primary_key", &["id"]),
                        ConstraintState {
                            name: "paths_file_foreign_key".to_owned(),
                            kind: "f".to_owned(),
                            columns: vec!["file_id".to_owned()],
                            referenced_table: "public.files".to_owned(),
                            referenced_columns: vec!["id".to_owned()],
                            on_delete: "r".to_owned(),
                            validated: true,
                        },
                    ],
                ),
            ]),
        }
    }

    fn column_state(
        name: &str,
        data_type: &str,
        is_nullable: bool,
        is_identity: bool,
    ) -> ColumnState {
        ColumnState {
            name: name.to_owned(),
            data_type: data_type.to_owned(),
            is_nullable,
            identity_generation: is_identity.then(|| "BY DEFAULT".to_owned()),
        }
    }

    fn primary_key_state(name: &str, columns: &[&str]) -> ConstraintState {
        ConstraintState {
            name: name.to_owned(),
            kind: "p".to_owned(),
            columns: columns.iter().map(|column| (*column).to_owned()).collect(),
            referenced_table: String::new(),
            referenced_columns: Vec::new(),
            on_delete: String::new(),
            validated: true,
        }
    }
}
