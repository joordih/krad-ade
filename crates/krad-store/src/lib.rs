mod chat_log;
mod config_dir;
mod gateway_key;
mod github_login;
mod gitlab_login;
mod layout;
mod settings;
mod vault;

pub use chat_log::{read_latest_chat, ChatLogWriter, StoredChat};
pub use config_dir::{config_dir, opencode_config_dir, APP_NAME};
pub use gateway_key::{delete_gateway_key, find_gateway_key, write_gateway_key};
pub use github_login::{
    delete_github_login, find_github_login, read_github_login, write_github_login,
};
pub use gitlab_login::{
    delete_gitlab_login, find_gitlab_login, read_gitlab_login, write_gitlab_login,
};
pub use layout::{read_layout_snapshot, LayoutWriter};
pub use settings::{read_settings, write_settings};
pub use vault::{LoginPlace, LoginStoreError};
