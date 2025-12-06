//! Authentication and authorization utilities

use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,       // Subject (user ID)
    pub exp: u64,          // Expiry time
    pub iat: u64,          // Issued at
    pub role: String,      // User role
    pub permissions: Vec<String>,
}

/// JWT token manager
pub struct TokenManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    algorithm: Algorithm,
}

impl TokenManager {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            algorithm: Algorithm::HS256,
        }
    }

    /// Generate a new JWT token
    pub fn generate_token(
        &self,
        user_id: &str,
        role: &str,
        permissions: Vec<String>,
        duration_seconds: u64,
    ) -> Result<String, AuthError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError::SystemTime)?
            .as_secs();

        let claims = Claims {
            sub: user_id.to_string(),
            exp: now + duration_seconds,
            iat: now,
            role: role.to_string(),
            permissions,
        };

        encode(&Header::new(self.algorithm), &claims, &self.encoding_key)
            .map_err(|_| AuthError::TokenGeneration)
    }

    /// Validate and decode a JWT token
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let validation = Validation::new(self.algorithm);

        decode::<Claims>(token, &self.decoding_key, &validation)
            .map(|data| data.claims)
            .map_err(|_| AuthError::InvalidToken)
    }
}

/// User authentication service
pub struct AuthService {
    token_manager: TokenManager,
}

impl AuthService {
    pub fn new(secret: &str) -> Self {
        Self {
            token_manager: TokenManager::new(secret),
        }
    }

    /// Authenticate a user with username and password
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AuthResponse, AuthError> {
        // Mock user validation
        if !validate_credentials(username, password).await {
            return Err(AuthError::InvalidCredentials);
        }

        // Get user details (mocked)
        let user = get_user_details(username).await?;

        // Generate token
        let token = self.token_manager.generate_token(
            &user.id,
            &user.role,
            user.permissions.clone(),
            3600, // 1 hour
        )?;

        Ok(AuthResponse {
            token,
            user,
        })
    }

    /// Authorize a user action
    pub fn authorize(&self, token: &str, required_permission: &str) -> Result<(), AuthError> {
        let claims = self.token_manager.validate_token(token)?;

        // Check if token is expired
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AuthError::SystemTime)?
            .as_secs();

        if claims.exp < now {
            return Err(AuthError::TokenExpired);
        }

        // Check permissions
        if !claims.permissions.contains(&required_permission.to_string()) {
            return Err(AuthError::InsufficientPermissions);
        }

        Ok(())
    }
}

/// Password hashing utilities
pub mod password {
    use bcrypt::{hash, verify, DEFAULT_COST};

    pub fn hash_password(password: &str) -> Result<String, super::AuthError> {
        hash(password, DEFAULT_COST).map_err(|_| super::AuthError::PasswordHashing)
    }

    pub fn verify_password(password: &str, hash: &str) -> Result<bool, super::AuthError> {
        verify(password, hash).map_err(|_| super::AuthError::PasswordVerification)
    }
}

/// Mock user structure
#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: User,
}

/// Mock functions for demonstration
async fn validate_credentials(username: &str, password: &str) -> bool {
    // Mock validation
    username == "admin" && password == "password123"
}

async fn get_user_details(username: &str) -> Result<User, AuthError> {
    // Mock user retrieval
    Ok(User {
        id: "user123".to_string(),
        username: username.to_string(),
        role: "admin".to_string(),
        permissions: vec!["read".to_string(), "write".to_string(), "delete".to_string()],
    })
}

#[derive(Debug)]
pub enum AuthError {
    InvalidCredentials,
    InvalidToken,
    TokenExpired,
    TokenGeneration,
    InsufficientPermissions,
    PasswordHashing,
    PasswordVerification,
    SystemTime,
    UserNotFound,
}