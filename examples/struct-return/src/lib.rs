pub mod models {
    pub struct User {
        pub name: String,
        pub active: bool,
    }
}

pub mod directory {
    pub use super::models::{self as schema, User as Member};
}

use crate::models::User as Account;

pub fn demo() -> Account {
    let name = String::from("Alice");
    let active = true;
    let user = Account {
        name,
        active,
    };
    user
}

pub fn count(values: &[i32]) -> usize {
    let count = values.len();
    count
}
