use std::collections::HashSet;

#[derive(Default)]
pub struct ShortCodeGenerator {
    used_codes: HashSet<String>,
}

impl ShortCodeGenerator {
    pub fn new() -> Self {
        Self {
            used_codes: HashSet::new(),
        }
    }
}
