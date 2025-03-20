use profile::ProfileChunk;
use pyo3::prelude::*;

mod android;
mod debug_images;
mod frame;
mod nodetree;
mod profile;
mod sample;
mod types;

const MAX_STACK_DEPTH: u64 = 128;

#[pyfunction]
fn parse_profile_chunk(profile: &str) -> PyResult<ProfileChunk> {
    ProfileChunk::from_json_string(profile)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
}

/// A Python module implemented in Rust.
#[pymodule]
fn rust_vroom(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_profile_chunk, m)?)?;
    Ok(())
}
