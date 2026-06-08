pub mod cache;
pub mod config;
pub mod engine;
pub mod error;
pub mod history;
pub mod index;
pub mod operator;
pub mod repo;
pub mod tag;

#[cfg(feature = "python-bindings")]
mod bindings;

#[cfg(feature = "python-bindings")]
use pyo3::prelude::*;

#[cfg(feature = "python-bindings")]
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<bindings::PyLogRepo>()?;
    m.add_class::<bindings::PyRepoMetadata>()?;
    m.add_class::<bindings::PyOperationRecord>()?;
    m.add_class::<bindings::PyLogStats>()?;
    m.add_class::<bindings::PyWorkspace>()?;
    Ok(())
}
