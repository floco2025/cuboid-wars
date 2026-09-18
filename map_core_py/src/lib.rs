//! Python adapter; map behavior belongs to map_core.
use pyo3::{exceptions::PyValueError, prelude::*};

#[pyfunction]
fn call(operation: &str, input: &str) -> PyResult<String> {
    let args = serde_json::from_str(input).map_err(|error| PyValueError::new_err(error.to_string()))?;
    let result = map_core::dispatch(operation, &args).map_err(|error| PyValueError::new_err(format!("{error:#}")))?;
    serde_json::to_string(&result).map_err(|error| PyValueError::new_err(error.to_string()))
}

#[pymodule]
fn _map_core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(call, module)?)
}
