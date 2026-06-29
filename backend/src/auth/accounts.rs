use tracing::error;

use crate::{
    auth::{User, oauth::ProviderUserInfo, session},
    database::Database,
};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityPublic {
    pub provider: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug)]
pub struct LoginOutcome {
    pub user_id: String,
    pub is_new_user: bool,
}

#[derive(Debug)]
pub enum LinkOutcome {
    Linked,
    AlreadyLinked,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("this {provider} account is already linked to another user")]
    AlreadyLinkedToAnother {
        provider: String,
        /// The account the identity currently belongs to (the source to transfer from).
        source_user_id: String,
    },
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug)]
pub enum TransferResult {
    /// The identity was moved to the target account.
    Transferred,
    /// The identity no longer belongs to the expected source (state changed).
    NotFound,
    /// The identity is already on the target account (nothing to do).
    NotApplicable,
}

#[derive(Debug)]
pub enum UnlinkResult {
    Unlinked,
    NotFound,
    LastIdentity,
}

fn now_iso() -> String {
    jiff::Timestamp::now().to_string()
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}

struct Identity {
    id: String,
    user_id: String,
}

async fn find_identity(
    tx: &mut sqlx::sqlite::SqliteConnection,
    provider_id: &str,
    subject: &str,
) -> Result<Option<Identity>, sqlx::Error> {
    let row = sqlx::query!(
        "
        SELECT id, user_id
        FROM user_oauth_identities
        WHERE provider = ? AND provider_subject = ?
        ",
        provider_id,
        subject,
    )
    .fetch_optional(tx)
    .await?;

    Ok(row.map(|r| Identity {
        id: r.id,
        user_id: r.user_id,
    }))
}

async fn refresh_identity_fields(
    tx: &mut sqlx::sqlite::SqliteConnection,
    identity_id: &str,
    info: &ProviderUserInfo,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "
        UPDATE user_oauth_identities
        SET provider_email        = ?,
            provider_display_name = ?,
            provider_avatar_url   = ?,
            updated_at            = ?
        WHERE id = ?
        ",
        info.email,
        info.name,
        info.picture,
        now,
        identity_id,
    )
    .execute(tx)
    .await?;

    Ok(())
}

async fn insert_identity(
    tx: &mut sqlx::sqlite::SqliteConnection,
    user_id: &str,
    provider_id: &str,
    info: &ProviderUserInfo,
    now: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "
        INSERT INTO user_oauth_identities
            ( id
            , user_id
            , provider
            , provider_subject
            , provider_email
            , provider_display_name
            , provider_avatar_url
            , created_at
            , updated_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ",
        ulid::Ulid::new().to_string(),
        user_id,
        provider_id,
        info.subject,
        info.email,
        info.name,
        info.picture,
        now,
        now,
    )
    .execute(tx)
    .await?;

    Ok(())
}

pub async fn login(
    provider_id: &str,
    info: &ProviderUserInfo,
) -> Result<LoginOutcome, sqlx::Error> {
    let now = now_iso();
    let mut tx = Database::pool().begin().await?;

    if let Some(identity) = find_identity(&mut tx, provider_id, &info.subject).await? {
        refresh_identity_fields(&mut tx, &identity.id, info, &now).await?;
        tx.commit().await?;
        return Ok(LoginOutcome {
            user_id: identity.user_id,
            is_new_user: false,
        });
    }

    let user_id = ulid::Ulid::new().to_string();
    sqlx::query!(
        "
        INSERT INTO users
            ( id
            , display_name
            , email
            , avatar_url
            , created_at
            , updated_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ",
        user_id,
        info.name,
        info.email,
        info.picture,
        now,
        now,
    )
    .execute(&mut *tx)
    .await?;

    match insert_identity(&mut tx, &user_id, provider_id, info, &now).await {
        Ok(()) => {
            tx.commit().await?;
            Ok(LoginOutcome {
                user_id,
                is_new_user: true,
            })
        }
        Err(e) if is_unique_violation(&e) => {
            // Race lost: roll back the whole transaction (incl. the new user)
            // and re-fetch the winning identity in a fresh transaction.
            let _ = tx.rollback().await;
            let mut tx = Database::pool().begin().await?;
            let identity = find_identity(&mut tx, provider_id, &info.subject)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
            refresh_identity_fields(&mut tx, &identity.id, info, &now).await?;
            tx.commit().await?;
            Ok(LoginOutcome {
                user_id: identity.user_id,
                is_new_user: false,
            })
        }
        Err(e) => Err(e),
    }
}

pub async fn link(
    provider_id: &str,
    info: &ProviderUserInfo,
    current_user_id: &str,
) -> Result<LinkOutcome, LinkError> {
    let now = now_iso();
    let mut tx = Database::pool().begin().await?;

    let existing = find_identity(&mut tx, provider_id, &info.subject).await?;

    let outcome = match existing {
        Some(identity) if identity.user_id == current_user_id => {
            refresh_identity_fields(&mut tx, &identity.id, info, &now).await?;
            LinkOutcome::AlreadyLinked
        }
        Some(other) => {
            drop(tx);
            return Err(LinkError::AlreadyLinkedToAnother {
                provider: provider_id.to_string(),
                source_user_id: other.user_id,
            });
        }
        None => match insert_identity(&mut tx, current_user_id, provider_id, info, &now).await {
            Ok(()) => LinkOutcome::Linked,
            Err(e) if is_unique_violation(&e) => {
                let identity = find_identity(&mut tx, provider_id, &info.subject)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound)?;
                if identity.user_id == current_user_id {
                    refresh_identity_fields(&mut tx, &identity.id, info, &now).await?;
                    LinkOutcome::AlreadyLinked
                } else {
                    drop(tx);
                    return Err(LinkError::AlreadyLinkedToAnother {
                        provider: provider_id.to_string(),
                        source_user_id: identity.user_id,
                    });
                }
            }
            Err(e) => return Err(LinkError::Database(e)),
        },
    };

    tx.commit().await?;
    Ok(outcome)
}

pub async fn unlink(user_id: &str, provider_id: &str) -> Result<UnlinkResult, sqlx::Error> {
    let mut tx = Database::pool().begin().await?;

    let res = sqlx::query!(
        "
        DELETE FROM user_oauth_identities
        WHERE user_id = ? AND provider = ?
          AND (SELECT COUNT(*) FROM user_oauth_identities WHERE user_id = ?) > 1
        ",
        user_id,
        provider_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;

    if res.rows_affected() > 0 {
        tx.commit().await?;
        return Ok(UnlinkResult::Unlinked);
    }

    let has_provider = sqlx::query_scalar!(
        "
        SELECT
            COUNT(*) AS \"count!: i64\"
        FROM user_oauth_identities
        WHERE   user_id = ?
            AND provider = ?
        ",
        user_id,
        provider_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    if has_provider > 0 {
        Ok(UnlinkResult::LastIdentity)
    } else {
        Ok(UnlinkResult::NotFound)
    }
}

pub async fn transfer(
    target_user_id: &str,
    provider_id: &str,
    provider_subject: &str,
    source_user_id: &str,
) -> Result<TransferResult, sqlx::Error> {
    let now = now_iso();
    let mut tx = Database::pool().begin().await?;

    let identity = sqlx::query!(
        "
        SELECT id, user_id
        FROM user_oauth_identities
        WHERE provider = ? AND provider_subject = ?
        ",
        provider_id,
        provider_subject,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(identity) = identity else {
        return Ok(TransferResult::NotFound);
    };
    if identity.user_id == target_user_id {
        return Ok(TransferResult::NotApplicable);
    }
    if identity.user_id != source_user_id {
        // State changed between the collision and the confirm.
        return Ok(TransferResult::NotFound);
    }

    // Move the identity to the target.
    sqlx::query!(
        "UPDATE user_oauth_identities SET user_id = ?, updated_at = ? WHERE id = ?",
        target_user_id,
        now,
        identity.id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        "
        INSERT INTO user_settings (user_id, settings, updated_at)
        SELECT ?, settings, updated_at
        FROM user_settings
        WHERE user_id = ?
          AND NOT EXISTS (SELECT 1 FROM user_settings WHERE user_id = ?)
        ",
        target_user_id,
        source_user_id,
        target_user_id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE feedback SET user_id = ? WHERE user_id = ?",
        target_user_id,
        source_user_id,
    )
    .execute(&mut *tx)
    .await?;

    let remaining = sqlx::query_scalar!(
        "SELECT COUNT(*) AS \"count!: i64\" FROM user_oauth_identities WHERE user_id = ?",
        source_user_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if remaining == 0 {
        sqlx::query!("DELETE FROM users WHERE id = ?", source_user_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(TransferResult::Transferred)
}

pub async fn identities_for_user(user_id: &str) -> Result<Vec<IdentityPublic>, sqlx::Error> {
    let rows = sqlx::query!(
        "
        SELECT provider,
               provider_email,
               provider_display_name,
               provider_avatar_url
        FROM user_oauth_identities
        WHERE user_id = ?
        ORDER BY created_at
        ",
        user_id,
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| IdentityPublic {
            provider: r.provider,
            email: r.provider_email,
            display_name: r.provider_display_name,
            avatar_url: r.provider_avatar_url,
        })
        .collect())
}

pub async fn user_by_id(id: &str) -> Option<User> {
    match sqlx::query!(
        "
        SELECT
              id
            , display_name
            , email
            , avatar_url
        FROM users
        WHERE id = ?
        ",
        id,
    )
    .fetch_optional(&Database::pool())
    .await
    {
        Ok(Some(row)) => Some(User {
            id: row.id,
            display_name: row.display_name,
            email: row.email,
            avatar_url: row.avatar_url,
        }),
        Ok(None) => None,
        Err(e) => {
            error!(error = %e, "Failed to fetch user by id");
            None
        }
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSummary {
    pub id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub providers: Vec<String>,
}

#[derive(Debug)]
pub struct DeleteUserResult {
    pub deleted: bool,
    pub session_ids: Vec<String>,
}

/// Permanently remove an account and all associated data (cascade). Feedback
/// rows keep their content but lose the user association (`ON DELETE SET NULL`).
pub async fn delete_user(user_id: &str) -> Result<DeleteUserResult, sqlx::Error> {
    let sessions = session::list_sessions_for_user(user_id).await?;
    let session_ids: Vec<String> = sessions.into_iter().map(|s| s.id).collect();

    let res = sqlx::query!("DELETE FROM users WHERE id = ?", user_id)
        .execute(&Database::pool())
        .await?;

    Ok(DeleteUserResult {
        deleted: res.rows_affected() > 0,
        session_ids,
    })
}

pub async fn list_users() -> Result<Vec<UserSummary>, sqlx::Error> {
    let rows = sqlx::query!(
        "
        SELECT u.id,
               u.display_name,
               u.email,
               COALESCE(
                 (SELECT GROUP_CONCAT(provider, ',' ORDER BY created_at)
                  FROM user_oauth_identities WHERE user_id = u.id),
                 ''
               ) AS \"providers!: String\"
        FROM users u
        ORDER BY u.created_at DESC
        "
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|u| UserSummary {
            id: u.id,
            display_name: u.display_name,
            email: u.email,
            providers: if u.providers.is_empty() {
                Vec::new()
            } else {
                u.providers.split(',').map(String::from).collect()
            },
        })
        .collect())
}
