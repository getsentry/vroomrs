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
fn profile_chunk_from_json_str(profile: &str) -> PyResult<ProfileChunk> {
    ProfileChunk::from_json_vec(profile.as_bytes())
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
}

#[pyfunction]
fn decompress_profile_chunk(profile: &[u8]) -> PyResult<ProfileChunk> {
    ProfileChunk::decompress(profile)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
}

/// A Python module implemented in Rust.
#[pymodule]
fn vroomrs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(profile_chunk_from_json_str, m)?)?;
    m.add_function(wrap_pyfunction!(decompress_profile_chunk, m)?)?;
    Ok(())
}
