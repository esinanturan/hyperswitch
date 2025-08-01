use common_utils::types::user::ThemeLineage;
use diesel_models::user::theme::{self as storage, ThemeUpdate};
use error_stack::report;

use super::MockDb;
use crate::{
    connection,
    core::errors::{self, CustomResult},
    services::Store,
};

#[async_trait::async_trait]
pub trait ThemeInterface {
    async fn insert_theme(
        &self,
        theme: storage::ThemeNew,
    ) -> CustomResult<storage::Theme, errors::StorageError>;

    async fn find_theme_by_theme_id(
        &self,
        theme_id: String,
    ) -> CustomResult<storage::Theme, errors::StorageError>;

    async fn find_most_specific_theme_in_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<storage::Theme, errors::StorageError>;

    async fn find_theme_by_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<storage::Theme, errors::StorageError>;

    async fn update_theme_by_theme_id(
        &self,
        theme_id: String,
        theme_update: ThemeUpdate,
    ) -> CustomResult<storage::Theme, errors::StorageError>;

    async fn delete_theme_by_theme_id(
        &self,
        theme_id: String,
    ) -> CustomResult<storage::Theme, errors::StorageError>;

    async fn list_themes_at_and_under_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<Vec<storage::Theme>, errors::StorageError>;
}

#[async_trait::async_trait]
impl ThemeInterface for Store {
    async fn insert_theme(
        &self,
        theme: storage::ThemeNew,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let conn = connection::pg_connection_write(self).await?;
        theme
            .insert(&conn)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    async fn find_theme_by_theme_id(
        &self,
        theme_id: String,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let conn = connection::pg_connection_read(self).await?;
        storage::Theme::find_by_theme_id(&conn, theme_id)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    async fn find_most_specific_theme_in_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let conn = connection::pg_connection_read(self).await?;
        storage::Theme::find_most_specific_theme_in_lineage(&conn, lineage)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    async fn find_theme_by_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let conn = connection::pg_connection_read(self).await?;
        storage::Theme::find_by_lineage(&conn, lineage)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    async fn update_theme_by_theme_id(
        &self,
        theme_id: String,
        theme_update: ThemeUpdate,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let conn = connection::pg_connection_write(self).await?;
        storage::Theme::update_by_theme_id(&conn, theme_id, theme_update)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    async fn delete_theme_by_theme_id(
        &self,
        theme_id: String,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let conn = connection::pg_connection_write(self).await?;
        storage::Theme::delete_by_theme_id(&conn, theme_id)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }
    async fn list_themes_at_and_under_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<Vec<storage::Theme>, errors::StorageError> {
        let conn = connection::pg_connection_read(self).await?;
        storage::Theme::find_all_by_lineage_hierarchy(&conn, lineage)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }
}

fn check_theme_with_lineage(theme: &storage::Theme, lineage: &ThemeLineage) -> bool {
    match lineage {
        ThemeLineage::Tenant { tenant_id } => {
            &theme.tenant_id == tenant_id
                && theme.org_id.is_none()
                && theme.merchant_id.is_none()
                && theme.profile_id.is_none()
        }
        ThemeLineage::Organization { tenant_id, org_id } => {
            &theme.tenant_id == tenant_id
                && theme
                    .org_id
                    .as_ref()
                    .is_some_and(|org_id_inner| org_id_inner == org_id)
                && theme.merchant_id.is_none()
                && theme.profile_id.is_none()
        }
        ThemeLineage::Merchant {
            tenant_id,
            org_id,
            merchant_id,
        } => {
            &theme.tenant_id == tenant_id
                && theme
                    .org_id
                    .as_ref()
                    .is_some_and(|org_id_inner| org_id_inner == org_id)
                && theme
                    .merchant_id
                    .as_ref()
                    .is_some_and(|merchant_id_inner| merchant_id_inner == merchant_id)
                && theme.profile_id.is_none()
        }
        ThemeLineage::Profile {
            tenant_id,
            org_id,
            merchant_id,
            profile_id,
        } => {
            &theme.tenant_id == tenant_id
                && theme
                    .org_id
                    .as_ref()
                    .is_some_and(|org_id_inner| org_id_inner == org_id)
                && theme
                    .merchant_id
                    .as_ref()
                    .is_some_and(|merchant_id_inner| merchant_id_inner == merchant_id)
                && theme
                    .profile_id
                    .as_ref()
                    .is_some_and(|profile_id_inner| profile_id_inner == profile_id)
        }
    }
}

fn check_theme_belongs_to_lineage_hierarchy(
    theme: &storage::Theme,
    lineage: &ThemeLineage,
) -> bool {
    match lineage {
        ThemeLineage::Tenant { tenant_id } => &theme.tenant_id == tenant_id,
        ThemeLineage::Organization { tenant_id, org_id } => {
            &theme.tenant_id == tenant_id
                && theme
                    .org_id
                    .as_ref()
                    .is_some_and(|org_id_inner| org_id_inner == org_id)
        }
        ThemeLineage::Merchant {
            tenant_id,
            org_id,
            merchant_id,
        } => {
            &theme.tenant_id == tenant_id
                && theme
                    .org_id
                    .as_ref()
                    .is_some_and(|org_id_inner| org_id_inner == org_id)
                && theme
                    .merchant_id
                    .as_ref()
                    .is_some_and(|merchant_id_inner| merchant_id_inner == merchant_id)
        }
        ThemeLineage::Profile {
            tenant_id,
            org_id,
            merchant_id,
            profile_id,
        } => {
            &theme.tenant_id == tenant_id
                && theme
                    .org_id
                    .as_ref()
                    .is_some_and(|org_id_inner| org_id_inner == org_id)
                && theme
                    .merchant_id
                    .as_ref()
                    .is_some_and(|merchant_id_inner| merchant_id_inner == merchant_id)
                && theme
                    .profile_id
                    .as_ref()
                    .is_some_and(|profile_id_inner| profile_id_inner == profile_id)
        }
    }
}

#[async_trait::async_trait]
impl ThemeInterface for MockDb {
    async fn insert_theme(
        &self,
        new_theme: storage::ThemeNew,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let mut themes = self.themes.lock().await;
        for theme in themes.iter() {
            if new_theme.theme_id == theme.theme_id {
                return Err(errors::StorageError::DuplicateValue {
                    entity: "theme_id",
                    key: None,
                }
                .into());
            }

            if new_theme.tenant_id == theme.tenant_id
                && new_theme.org_id == theme.org_id
                && new_theme.merchant_id == theme.merchant_id
                && new_theme.profile_id == theme.profile_id
            {
                return Err(errors::StorageError::DuplicateValue {
                    entity: "lineage",
                    key: None,
                }
                .into());
            }
        }

        let theme = storage::Theme {
            theme_id: new_theme.theme_id,
            tenant_id: new_theme.tenant_id,
            org_id: new_theme.org_id,
            merchant_id: new_theme.merchant_id,
            profile_id: new_theme.profile_id,
            created_at: new_theme.created_at,
            last_modified_at: new_theme.last_modified_at,
            entity_type: new_theme.entity_type,
            theme_name: new_theme.theme_name,
            email_primary_color: new_theme.email_primary_color,
            email_foreground_color: new_theme.email_foreground_color,
            email_background_color: new_theme.email_background_color,
            email_entity_name: new_theme.email_entity_name,
            email_entity_logo_url: new_theme.email_entity_logo_url,
        };
        themes.push(theme.clone());

        Ok(theme)
    }

    async fn find_theme_by_theme_id(
        &self,
        theme_id: String,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let themes = self.themes.lock().await;
        themes
            .iter()
            .find(|theme| theme.theme_id == theme_id)
            .cloned()
            .ok_or(
                errors::StorageError::ValueNotFound(format!("Theme with id {theme_id} not found"))
                    .into(),
            )
    }

    async fn find_most_specific_theme_in_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let themes = self.themes.lock().await;
        let lineages = lineage.get_same_and_higher_lineages();

        themes
            .iter()
            .filter(|theme| {
                lineages
                    .iter()
                    .any(|lineage| check_theme_with_lineage(theme, lineage))
            })
            .min_by_key(|theme| theme.entity_type)
            .ok_or(
                errors::StorageError::ValueNotFound("No theme found in lineage".to_string()).into(),
            )
            .cloned()
    }

    async fn find_theme_by_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let themes = self.themes.lock().await;
        themes
            .iter()
            .find(|theme| check_theme_with_lineage(theme, &lineage))
            .cloned()
            .ok_or(
                errors::StorageError::ValueNotFound(format!(
                    "Theme with lineage {lineage:?} not found",
                ))
                .into(),
            )
    }

    async fn update_theme_by_theme_id(
        &self,
        theme_id: String,
        theme_update: ThemeUpdate,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let mut themes = self.themes.lock().await;
        themes
            .iter_mut()
            .find(|theme| theme.theme_id == theme_id)
            .map(|theme| {
                match theme_update {
                    ThemeUpdate::EmailConfig { email_config } => {
                        theme.email_primary_color = email_config.primary_color;
                        theme.email_foreground_color = email_config.foreground_color;
                        theme.email_background_color = email_config.background_color;
                        theme.email_entity_name = email_config.entity_name;
                        theme.email_entity_logo_url = email_config.entity_logo_url;
                    }
                }
                theme.clone()
            })
            .ok_or_else(|| {
                report!(errors::StorageError::ValueNotFound(format!(
                    "Theme with id {theme_id} not found",
                )))
            })
    }

    async fn delete_theme_by_theme_id(
        &self,
        theme_id: String,
    ) -> CustomResult<storage::Theme, errors::StorageError> {
        let mut themes = self.themes.lock().await;
        let index = themes
            .iter()
            .position(|theme| theme.theme_id == theme_id)
            .ok_or(errors::StorageError::ValueNotFound(format!(
                "Theme with id {theme_id} not found"
            )))?;

        let theme = themes.remove(index);
        Ok(theme)
    }
    async fn list_themes_at_and_under_lineage(
        &self,
        lineage: ThemeLineage,
    ) -> CustomResult<Vec<storage::Theme>, errors::StorageError> {
        let themes = self.themes.lock().await;
        let matching_themes: Vec<storage::Theme> = themes
            .iter()
            .filter(|theme| check_theme_belongs_to_lineage_hierarchy(theme, &lineage))
            .cloned()
            .collect();

        Ok(matching_themes)
    }
}
