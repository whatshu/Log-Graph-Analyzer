//! Operation executor modules.
//!
//! Each submodule handles one [`PendingOp`](super::super::state::PendingOp) variant.
//! The `apply_pending` method in [`App`](super::super::app::App) dispatches to these
//! functions, keeping the dispatch logic thin and each operation focused.

pub mod apply_from;
pub mod apply_operation;
pub mod checkout;
pub mod export;
pub mod merge;
pub mod replay;
pub mod soft_delete;
pub mod subtract;
pub mod undo;
