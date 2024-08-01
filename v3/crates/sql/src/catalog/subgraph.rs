use async_trait::async_trait;
use std::{any::Any, sync::Arc};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

mod datafusion {
    pub(super) use datafusion::error::{DataFusionError, Result};
    pub(super) use datafusion::{catalog::schema::SchemaProvider, datasource::TableProvider};
}

use crate::catalog;

use super::model;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Subgraph {
    pub tables: IndexMap<String, Arc<catalog::model::ModelWithPermissions>>,
}

#[async_trait]
impl datafusion::SchemaProvider for catalog::model::WithSession<Subgraph> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn table_names(&self) -> Vec<String> {
        self.value.tables.keys().cloned().collect::<Vec<_>>()
    }

    async fn table(
        &self,
        name: &str,
    ) -> datafusion::Result<Option<Arc<dyn datafusion::TableProvider>>> {
        if let Some(model) = self.value.tables.get(name) {
            let permission = model.permissions.get(&self.session.role).ok_or_else(|| {
                datafusion::DataFusionError::Plan(format!(
                    "role {} does not have select permission for model {}",
                    self.session.role, model.model.name
                ))
            })?;
            let model_source = model.source.as_ref().ok_or_else(|| {
                datafusion::DataFusionError::Plan(format!(
                    "model source should be configured for {}",
                    model.model.name
                ))
            })?;
            let table = model::Table::new_no_args(
                model.model.clone(),
                model_source.clone(),
                permission.clone(),
            );
            Ok(Some(Arc::new(table) as Arc<dyn datafusion::TableProvider>))
        } else {
            Ok(None)
        }
    }

    fn table_exist(&self, name: &str) -> bool {
        self.value.tables.contains_key(name)
    }
}
