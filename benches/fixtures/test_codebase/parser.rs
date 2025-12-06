//! String parsing and validation utilities

use regex::Regex;
use std::str::FromStr;

/// Generic parser trait
pub trait Parser: Sized {
    type Error;

    fn parse(input: &str) -> Result<Self, Self::Error>;
}

/// Email address parser and validator
#[derive(Debug, Clone, PartialEq)]
pub struct Email(String);

impl Email {
    pub fn new(email: &str) -> Result<Self, ValidationError> {
        validate_email(email)?;
        Ok(Self(email.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Email {
    type Err = ValidationError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// URL parser
#[derive(Debug, Clone)]
pub struct Url {
    scheme: String,
    host: String,
    port: Option<u16>,
    path: String,
}

impl Url {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let re = Regex::new(r"^(https?)://([^:/]+)(:\d+)?(.*)$").unwrap();

        let captures = re.captures(input)
            .ok_or(ParseError::InvalidFormat)?;

        let scheme = captures.get(1).unwrap().as_str().to_string();
        let host = captures.get(2).unwrap().as_str().to_string();
        let port = captures.get(3)
            .map(|m| m.as_str().trim_start_matches(':').parse().ok())
            .flatten();
        let path = captures.get(4)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());

        Ok(Self {
            scheme,
            host,
            port,
            path,
        })
    }
}

/// CSV parser
pub struct CsvParser {
    delimiter: char,
}

impl CsvParser {
    pub fn new(delimiter: char) -> Self {
        Self { delimiter }
    }

    pub fn parse_line(&self, line: &str) -> Vec<String> {
        let mut result = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '"' => {
                    if in_quotes && chars.peek() == Some(&'"') {
                        current.push('"');
                        chars.next();
                    } else {
                        in_quotes = !in_quotes;
                    }
                }
                c if c == self.delimiter && !in_quotes => {
                    result.push(current.trim().to_string());
                    current.clear();
                }
                c => {
                    current.push(c);
                }
            }
        }

        if !current.is_empty() || line.ends_with(self.delimiter) {
            result.push(current.trim().to_string());
        }

        result
    }
}

/// JSON path parser
pub struct JsonPath {
    segments: Vec<PathSegment>,
}

#[derive(Debug)]
enum PathSegment {
    Field(String),
    Index(usize),
}

impl JsonPath {
    pub fn parse(path: &str) -> Result<Self, ParseError> {
        let mut segments = Vec::new();
        let parts = path.split('.');

        for part in parts {
            if part.is_empty() {
                continue;
            }

            if let Some(array_part) = part.strip_suffix(']') {
                if let Some(idx_start) = array_part.rfind('[') {
                    let field = &array_part[..idx_start];
                    let index_str = &array_part[idx_start + 1..];

                    if !field.is_empty() {
                        segments.push(PathSegment::Field(field.to_string()));
                    }

                    let index = index_str.parse::<usize>()
                        .map_err(|_| ParseError::InvalidIndex)?;
                    segments.push(PathSegment::Index(index));
                } else {
                    return Err(ParseError::InvalidFormat);
                }
            } else {
                segments.push(PathSegment::Field(part.to_string()));
            }
        }

        Ok(Self { segments })
    }
}

/// Validation functions
pub fn validate_email(email: &str) -> Result<(), ValidationError> {
    let re = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();

    if !re.is_match(email) {
        return Err(ValidationError::InvalidEmail);
    }

    Ok(())
}

pub fn validate_phone(phone: &str) -> Result<(), ValidationError> {
    let re = Regex::new(r"^\+?[1-9]\d{1,14}$").unwrap();

    if !re.is_match(phone) {
        return Err(ValidationError::InvalidPhone);
    }

    Ok(())
}

pub fn validate_uuid(uuid: &str) -> Result<(), ValidationError> {
    let re = Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$").unwrap();

    if !re.is_match(uuid) {
        return Err(ValidationError::InvalidUuid);
    }

    Ok(())
}

#[derive(Debug)]
pub enum ValidationError {
    InvalidEmail,
    InvalidPhone,
    InvalidUuid,
    InvalidFormat,
}

#[derive(Debug)]
pub enum ParseError {
    InvalidFormat,
    InvalidIndex,
    MissingField,
}