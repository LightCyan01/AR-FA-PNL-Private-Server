// Account/session result types shared by the state service.

#[derive(Debug, Clone)]
pub struct SignInResult {
    pub session_token: String,
    pub user_id: i64,
    pub language: i32,
}
