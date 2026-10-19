pub fn original(name: String, active: bool) -> (String, bool) {
    struct User {
        name: String,
        active: bool,
    }
    let user = User {
        name,
        active,
    };
    (user.name, user.active)
}
