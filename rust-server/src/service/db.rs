use sqlx::PgPool;

#[derive(Clone)]
pub struct DbService {
    pub pool: PgPool,
}

impl DbService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
