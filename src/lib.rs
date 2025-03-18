use pyo3::prelude::*;

pub mod android;
mod debug_images;
pub mod frame;
pub mod nodetree;
pub mod sample;
pub mod types;

const MAX_STACK_DEPTH: u64 = 128;

/// Formats the sum of two numbers as string.
#[pyfunction]
fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
    Ok((a + b).to_string())
}

/// A Python module implemented in Rust.
#[pymodule]
fn rust_vroom(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;
    Ok(())
}
