//! The contracts panel's sort criterion, shared by the quest model's sorting and the panel's
//! header selection. The retired retail UI's `panels::contracts` and `pyreal_client_model::quests`
//! re-export it.

/// How the contracts panel orders its rows: by name (`0`, the default) or by status (`1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContractSort {
    #[default]
    Name = 0,
    Status = 1,
}
