//! Database operations and connection management

use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;

/// Database connection pool manager
pub struct Database {
    pool: Arc<Pool<Postgres>>,
}

impl Database {
    /// Create a new database connection pool
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    /// Get a reference to the connection pool
    pub fn connection_pool(&self) -> &Pool<Postgres> {
        &self.pool
    }

    /// Execute a query and return affected rows
    pub async fn execute(&self, query: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(query)
            .execute(&*self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Query for a single row
    pub async fn query_one(&self, query: &str) -> Result<sqlx::postgres::PgRow, sqlx::Error> {
        sqlx::query(query)
            .fetch_one(&*self.pool)
            .await
    }

    /// Query for multiple rows
    pub async fn query_many(&self, query: &str) -> Result<Vec<sqlx::postgres::PgRow>, sqlx::Error> {
        sqlx::query(query)
            .fetch_all(&*self.pool)
            .await
    }

    /// Begin a transaction
    pub async fn begin_transaction(&self) -> Result<sqlx::Transaction<'_, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }
}

/// User repository for database operations
pub struct UserRepository {
    db: Arc<Database>,
}

impl UserRepository {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Find a user by ID
    pub async fn find_by_id(&self, id: &str) -> Result<Option<User>, sqlx::Error> {
        let query = "SELECT id, name, email, created_at FROM users WHERE id = $1";

        match sqlx::query_as::<_, User>(query)
            .bind(id)
            .fetch_optional(self.db.connection_pool())
            .await
        {
            Ok(user) => Ok(user),
            Err(e) => Err(e),
        }
    }

    /// Create a new user
    pub async fn create(&self, user: CreateUser) -> Result<User, sqlx::Error> {
        let query = "
            INSERT INTO users (name, email, password_hash)
            VALUES ($1, $2, $3)
            RETURNING id, name, email, created_at
        ";

        sqlx::query_as::<_, User>(query)
            .bind(&user.name)
            .bind(&user.email)
            .bind(&user.password_hash)
            .fetch_one(self.db.connection_pool())
            .await
    }

    /// Update a user
    pub async fn update(&self, id: &str, update: UpdateUser) -> Result<User, sqlx::Error> {
        let query = "
            UPDATE users
            SET name = COALESCE($2, name),
                email = COALESCE($3, email),
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, name, email, created_at
        ";

        sqlx::query_as::<_, User>(query)
            .bind(id)
            .bind(update.name)
            .bind(update.email)
            .fetch_one(self.db.connection_pool())
            .await
    }

    /// Delete a user
    pub async fn delete(&self, id: &str) -> Result<bool, sqlx::Error> {
        let query = "DELETE FROM users WHERE id = $1";

        let result = sqlx::query(query)
            .bind(id)
            .execute(self.db.connection_pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// List all users with pagination
    pub async fn list(&self, limit: i32, offset: i32) -> Result<Vec<User>, sqlx::Error> {
        let query = "
            SELECT id, name, email, created_at
            FROM users
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
        ";

        sqlx::query_as::<_, User>(query)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.connection_pool())
            .await
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct User {
    pub id: i32,
    pub name: String,
    pub email: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct CreateUser {
    pub name: String,
    pub email: String,
    pub password_hash: String,
}

pub struct UpdateUser {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Migration runner
pub async fn run_migrations(db: &Database) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations")
        .run(db.connection_pool())
        .await?;

    Ok(())
}