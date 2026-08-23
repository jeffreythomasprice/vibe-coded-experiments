//! CRUD on user-authored theme documents.
//!
//! Owns the two validations the database can't express — a name may not
//! collide with a built-in's label, and every color field must be
//! `#rrggbb` — using `shared::theme`'s own helpers rather than duplicating
//! the rules here. The database still owns uniqueness *among user themes*
//! (`lib::db::themes`'s `themes_name` index and pre-check).

use shared::ids::ThemeId;
use shared::theme::{built_in_name_conflict, parse_hex_rgb, ThemeField, UserTheme, UserThemeInput};

use crate::db;

use super::{Service, ServiceError};

impl Service {
    pub async fn list_themes(&self) -> Result<Vec<UserTheme>, ServiceError> {
        Ok(db::themes::list(&self.db).await?)
    }

    pub async fn create_theme(&self, input: UserThemeInput) -> Result<UserTheme, ServiceError> {
        validate_theme_input(&input)?;
        Ok(db::themes::create(&self.db, &input).await?)
    }

    pub async fn update_theme(&self, id: ThemeId, input: UserThemeInput) -> Result<UserTheme, ServiceError> {
        validate_theme_input(&input)?;
        Ok(db::themes::update(&self.db, id, &input).await?)
    }

    pub async fn delete_theme(&self, id: ThemeId) -> Result<(), ServiceError> {
        Ok(db::themes::delete(&self.db, id).await?)
    }
}

/// The two rules the database can't express: a name may not equal a
/// built-in's label (built-ins never have a `themes` row for the unique
/// index to see), and every color must parse as `#rrggbb`.
fn validate_theme_input(input: &UserThemeInput) -> Result<(), ServiceError> {
    if input.name.trim().is_empty() {
        return Err(ServiceError::EmptyThemeName);
    }
    if built_in_name_conflict(&input.name).is_some() {
        return Err(ServiceError::ThemeNameConflict { name: input.name.clone() });
    }
    for field in ThemeField::ALL {
        let value = field.get(&input.theme);
        if parse_hex_rgb(value).is_none() {
            return Err(ServiceError::InvalidThemeColor {
                field: field.label(),
                value: value.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::llm::router::Router;
    use shared::theme::BuiltIn;
    use std::sync::Arc;

    async fn empty_service() -> Service {
        let db = Db::in_memory().await.unwrap();
        Service::new(db, Arc::new(Router::new()), Arc::new(crate::agent::ToolRegistry::new()))
    }

    fn input(name: &str) -> UserThemeInput {
        UserThemeInput {
            name: name.to_string(),
            theme: BuiltIn::Dracula.theme(),
        }
    }

    #[tokio::test]
    async fn create_then_list_round_trips() {
        let service = empty_service().await;
        let created = service.create_theme(input("My Theme")).await.unwrap();
        assert_eq!(service.list_themes().await.unwrap(), vec![created]);
    }

    #[tokio::test]
    async fn create_rejects_a_name_matching_a_built_in_label() {
        let service = empty_service().await;
        let err = service.create_theme(input("Dracula")).await.unwrap_err();
        assert!(matches!(err, ServiceError::ThemeNameConflict { name } if name == "Dracula"));
    }

    #[tokio::test]
    async fn create_rejects_a_name_matching_a_built_in_label_case_insensitively() {
        let service = empty_service().await;
        let err = service.create_theme(input("dracula")).await.unwrap_err();
        assert!(matches!(err, ServiceError::ThemeNameConflict { .. }));
    }

    #[tokio::test]
    async fn create_rejects_a_blank_name() {
        let service = empty_service().await;
        let err = service.create_theme(input("   ")).await.unwrap_err();
        assert!(matches!(err, ServiceError::EmptyThemeName));
    }

    #[tokio::test]
    async fn create_rejects_an_invalid_color() {
        let service = empty_service().await;
        let mut bad = input("My Theme");
        bad.theme.accent = "not-a-color".to_string();
        let err = service.create_theme(bad).await.unwrap_err();
        assert!(matches!(err, ServiceError::InvalidThemeColor { .. }));
    }

    #[tokio::test]
    async fn update_revalidates_the_input() {
        let service = empty_service().await;
        let created = service.create_theme(input("My Theme")).await.unwrap();
        let err = service.update_theme(created.id, input("Dracula")).await.unwrap_err();
        assert!(matches!(err, ServiceError::ThemeNameConflict { .. }));
    }

    #[tokio::test]
    async fn delete_theme_removes_it() {
        let service = empty_service().await;
        let created = service.create_theme(input("My Theme")).await.unwrap();
        service.delete_theme(created.id).await.unwrap();
        assert!(service.list_themes().await.unwrap().is_empty());
    }
}
