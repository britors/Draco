//! Schema comparison between two connections, or two schemas of one connection. Both sides are
//! read-only snapshots; the generated script is returned as text and only runs if the user opens
//! it in the SQL editor and executes it there, under the editor's usual confirmations.

use draco_core::postgres::queries;
pub use draco_core::schema_diff::{DiffObjectKind, DiffStatus, ObjectDiff};
use serde::{Deserialize, Serialize};

use crate::{validate_schema_object_name, Application, ApplicationError, Result, Validation};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDiffInput {
    pub source_id: String,
    pub source_schema: String,
    pub target_id: String,
    pub target_schema: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaDiffView {
    pub source_label: String,
    pub target_label: String,
    pub source_schema: String,
    pub target_schema: String,
    pub objects: Vec<ObjectDiff>,
    /// SQL that makes the target match the source, for review in the SQL editor.
    pub script: String,
    pub destructive: bool,
}

impl Application {
    pub async fn schema_diff(&self, input: SchemaDiffInput) -> Result<SchemaDiffView> {
        validate_schema_object_name(&input.source_schema, "Schema")?;
        validate_schema_object_name(&input.target_schema, "Schema")?;
        if input.source_id == input.target_id && input.source_schema == input.target_schema {
            return Err(ApplicationError::InvalidInput(Validation::new(
                "validation.schemaDiffSameSide",
                "Choose two different schemas or connections to compare",
            )));
        }
        let (source_driver, source_label) = self.connected_driver(&input.source_id).await?;
        let (target_driver, target_label) = self.connected_driver(&input.target_id).await?;
        let (source, target) = tokio::join!(
            queries::get_schema_snapshot(&source_driver, &input.source_schema),
            queries::get_schema_snapshot(&target_driver, &input.target_schema),
        );
        let diff = draco_core::schema_diff::diff_schemas(&source?, &target?);
        Ok(SchemaDiffView {
            source_label,
            target_label,
            source_schema: input.source_schema,
            target_schema: input.target_schema,
            objects: diff.objects,
            script: diff.script,
            destructive: diff.destructive,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn comparing_a_schema_with_itself_is_rejected() {
        let app = Application::new();
        let error = app
            .schema_diff(SchemaDiffInput {
                source_id: "a".into(),
                source_schema: "public".into(),
                target_id: "a".into(),
                target_schema: "public".into(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            ApplicationError::InvalidInput(Validation {
                key: "validation.schemaDiffSameSide",
                ..
            })
        ));
    }
}
