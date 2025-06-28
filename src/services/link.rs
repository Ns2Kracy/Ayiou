use std::sync::{Arc, Mutex};

use sqlx::PgPool;

use crate::utils::shortener::ShortCodeGenerator;

pub struct LinkService {
    db: PgPool,
    shortener: Arc<Mutex<ShortCodeGenerator>>,
}

impl LinkService {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            shortener: Arc::new(Mutex::new(ShortCodeGenerator::new())),
        }
    }
}
