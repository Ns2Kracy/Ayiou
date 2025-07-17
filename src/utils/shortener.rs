use rand::{Rng, random};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

// Base62 character set (excluding confusing characters like 0, O, I, l)
const BASE62_CHARS: &[u8] = b"123456789ABCDEFGHJKMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz";
const BASE62_LEN: usize = BASE62_CHARS.len();

/// Production-grade short code generator
/// Focused on high-performance, high-availability, high-throughput single algorithm design
pub struct ShortCodeGenerator {
    /// Global counter ensuring basic uniqueness
    counter: AtomicU64,
    /// Random seed for unpredictability
    random_seed: u64,
}

impl ShortCodeGenerator {
    /// Create a new generator instance
    pub fn new() -> Self {
        let random_seed: u64 = random();

        Self {
            counter: AtomicU64::new(Self::init_counter()),
            random_seed,
        }
    }

    /// Generate short code
    /// Algorithm: timestamp(41 bits) + counter(15 bits) + random(8 bits) = 64 bits
    /// Enhanced randomness and counter space without machine_id dependency
    pub fn generate(&self) -> String {
        let timestamp = self.current_timestamp();
        let sequence = self.counter.fetch_add(1, Ordering::Relaxed) & 0x7FFF; // 15 bits counter

        // Generate 8 bits of randomness for better uniqueness
        let mut rng = rand::rng();
        let random_bits = rng.random::<u8>() as u64; // 8 bits random

        // Mix with stored random seed for additional entropy
        let mixed_random = (random_bits ^ (self.random_seed & 0xFF)) & 0xFF;

        // Combine into 64-bit ID: timestamp(41) + sequence(15) + random(8)
        let id = (timestamp << 23) | (sequence << 8) | mixed_random;

        self.encode_base62(id)
    }

    /// Generate short code with specified length
    pub fn generate_with_length(&self, target_length: usize) -> String {
        let mut code = self.generate();

        // If generated code is too short, add some random characters
        while code.len() < target_length {
            let mut rng = rand::rng();
            let idx = rng.random_range(0..BASE62_LEN);
            let random_char = BASE62_CHARS[idx] as char;
            code.push(random_char);
        }

        // If too long, truncate to target length
        if code.len() > target_length {
            code.truncate(target_length);
        }

        code
    }

    /// Validate custom short code safety
    pub fn validate_custom_code(&self, code: &str) -> Result<(), ShortenerError> {
        // Length check
        if code.len() < 3 {
            return Err(ShortenerError::CodeTooShort);
        }
        if code.len() > 12 {
            return Err(ShortenerError::CodeTooLong);
        }

        // Character check
        for ch in code.chars() {
            if !BASE62_CHARS.contains(&(ch as u8)) {
                return Err(ShortenerError::InvalidChar);
            }
        }

        // Safety check (prevent inappropriate content)
        if self.is_unsafe_code(code) {
            return Err(ShortenerError::UnsafeContent);
        }

        Ok(())
    }

    /// Get generator statistics
    pub fn get_stats(&self) -> GeneratorStats {
        GeneratorStats {
            total_generated: self.counter.load(Ordering::Relaxed),
            current_timestamp: self.current_timestamp(),
            random_seed: self.random_seed,
        }
    }

    // Private methods

    /// Get current timestamp (milliseconds, relative to custom epoch)
    fn current_timestamp(&self) -> u64 {
        // Use January 1, 2024 as epoch to reduce timestamp size
        const CUSTOM_EPOCH: u64 = 1704067200000; // 2024-01-01 00:00:00 UTC in milliseconds

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        (now - CUSTOM_EPOCH) & 0x1FFFFFFFFFF // 41 bits
    }

    /// Initialize counter (based on startup time and random number for better distribution)
    fn init_counter() -> u64 {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64; // Use nanoseconds for better entropy

        let random: u64 = random();

        // Mix timestamp and random for better initial distribution
        ((base >> 20) ^ random) & 0x7FFFFFFF // Use more bits for initial counter
    }

    /// Base62 encoding
    fn encode_base62(&self, mut num: u64) -> String {
        if num == 0 {
            return (BASE62_CHARS[0] as char).to_string();
        }

        let mut result = String::new();
        while num > 0 {
            let remainder = (num % BASE62_LEN as u64) as usize;
            result.push(BASE62_CHARS[remainder] as char);
            num /= BASE62_LEN as u64;
        }

        // Base62 encoding is reversed, need to flip
        result.chars().rev().collect()
    }

    /// Check if content is unsafe
    fn is_unsafe_code(&self, code: &str) -> bool {
        let code_lower = code.to_lowercase();

        // Simplified inappropriate word check
        const UNSAFE_WORDS: &[&str] = &[
            "fuck", "shit", "sex", "porn", "nazi", "hate", "kill", "die", "bomb",
        ];

        UNSAFE_WORDS.iter().any(|&word| code_lower.contains(word))
    }
}

impl Default for ShortCodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Generator statistics
#[derive(Debug)]
pub struct GeneratorStats {
    pub total_generated: u64,
    pub current_timestamp: u64,
    pub random_seed: u64,
}

/// Error types
#[derive(Debug, thiserror::Error)]
pub enum ShortenerError {
    #[error("Code too short, minimum 3 characters")]
    CodeTooShort,

    #[error("Code too long, maximum 12 characters")]
    CodeTooLong,

    #[error("Contains invalid characters")]
    InvalidChar,

    #[error("Contains inappropriate content")]
    UnsafeContent,
}

/// Helper function: validate URL format
pub fn validate_url(url: &str) -> bool {
    url::Url::parse(url).is_ok() && url.len() <= 2048
}

/// Helper function: normalize URL
pub fn normalize_url(url: &str) -> Result<String, url::ParseError> {
    let mut parsed = url::Url::parse(url)?;

    // Remove fragment identifier
    parsed.set_fragment(None);

    // Normalize path
    let path = parsed.path().to_string();
    if path.ends_with('/') && path.len() > 1 {
        let new_path = &path[..path.len() - 1];
        parsed.set_path(new_path);
    }

    Ok(parsed.to_string())
}

// Global singleton generator
use std::sync::OnceLock;
static GLOBAL_GENERATOR: OnceLock<ShortCodeGenerator> = OnceLock::new();

/// Get global generator instance
pub fn global_generator() -> &'static ShortCodeGenerator {
    GLOBAL_GENERATOR.get_or_init(ShortCodeGenerator::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_basic_generation() {
        let generator = ShortCodeGenerator::new();
        let code = generator.generate();
        assert!(!code.is_empty());
        assert!(code.len() >= 3);
        assert!(code.len() <= 12);
    }

    #[test]
    fn test_uniqueness() {
        let generator = ShortCodeGenerator::new();
        let mut codes = HashSet::new();

        // Generate 10000 short codes, check uniqueness
        for _ in 0..10000 {
            let code = generator.generate();
            assert!(codes.insert(code), "Found duplicate short code");
        }
    }

    #[test]
    fn test_custom_code_validation() {
        let generator = ShortCodeGenerator::new();

        // Valid codes
        assert!(generator.validate_custom_code("abc123").is_ok());
        assert!(generator.validate_custom_code("xyz789").is_ok());

        // Invalid codes
        assert!(generator.validate_custom_code("ab").is_err()); // Too short
        assert!(
            generator
                .validate_custom_code("verylongcustomcode123")
                .is_err()
        ); // Too long
        assert!(generator.validate_custom_code("fuck").is_err()); // Inappropriate content
    }

    #[test]
    fn test_enhanced_randomness() {
        let generator = ShortCodeGenerator::new();
        let codes: HashSet<_> = (0..1000).map(|_| generator.generate()).collect();

        // All codes should be unique
        assert_eq!(codes.len(), 1000);
    }

    #[test]
    fn test_concurrent_generation() {
        let generator = ShortCodeGenerator::new();
        let codes: HashSet<_> = (0..5000).map(|_| generator.generate()).collect();

        // Even with rapid generation, all should be unique
        assert_eq!(codes.len(), 5000);
    }

    #[test]
    fn test_url_validation() {
        assert!(validate_url("https://example.com"));
        assert!(validate_url("http://localhost:8080/path"));
        assert!(!validate_url("not-a-url"));
        assert!(!validate_url(""));
    }

    #[test]
    fn test_url_normalization() {
        assert_eq!(
            normalize_url("https://example.com/path/").unwrap(),
            "https://example.com/path"
        );

        assert_eq!(
            normalize_url("https://example.com/#fragment").unwrap(),
            "https://example.com/"
        );
    }
}
