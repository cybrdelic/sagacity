#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    User,
    AI,
}

/// Represents a chat message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub sender: Sender,
    pub content: String,
}
