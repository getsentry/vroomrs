use nodetree::CallTreeFunction;
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

/// Returns a `ProfileChunk` instance from a json string
///
/// Arguments
/// ---------
/// profile : str
///   A profile serialized as json string
///
///     platform (string): An optional string representing the profile platform.
///         If provided, we can directly deserialize to the right profile chunk
///         more efficiently.
///         If the platform is known at the time this function is invoked, it's
///         recommended to always pass it.
///
/// Returns
/// -------
/// :class:`vroomrs.ProfileChunk`
///   A `ProfileChunk` instance
///
/// Raises
/// -------
/// pyo3.exceptions.PyException
///     If an error occurs during the extraction process.
///
#[pyfunction]
#[pyo3(signature = (profile, platform=None))]
fn profile_chunk_from_json_str(profile: &str, platform: Option<&str>) -> PyResult<ProfileChunk> {
    match platform {
        Some(platform) => ProfileChunk::from_json_vec_and_platform(profile.as_bytes(), platform)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())),
        None => ProfileChunk::from_json_vec(profile.as_bytes())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())),
    }
}

/// Returns a `ProfileChunk` instance from a lz4 encoded profile.
///
/// Arguments
/// ---------
/// profile : bytes
///   A lz4 encoded profile.
///
/// Returns
/// -------
/// :class:`vroomrs.ProfileChunk`
///   A `ProfileChunk` instance
///
/// Raises
/// ------
/// pyo3.exceptions.PyException
///     If an error occurs during the extraction process.
///
/// Example
/// --------
///     >>> with open("profile_compressed.lz4", "rb") as binary_file:
///     ...     profile = vroomrs.decompress_profile_chunk(binary_file.read())
///             # do something with the profile
///
#[pyfunction]
fn decompress_profile_chunk(profile: &[u8]) -> PyResult<ProfileChunk> {
    ProfileChunk::decompress(profile)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
}

#[pymodule]
fn vroomrs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ProfileChunk>()?;
    m.add_class::<CallTreeFunction>()?;
    m.add_function(wrap_pyfunction!(profile_chunk_from_json_str, m)?)?;
    m.add_function(wrap_pyfunction!(decompress_profile_chunk, m)?)?;
    Ok(())
}
