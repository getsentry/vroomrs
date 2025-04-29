from typing import List, Optional, Union

class ProfileChunk:
    """
    This is a ProfileChunk class
    """
    def normalize(self) -> None:
        """
        Applies the various normalization steps,
        depending on the profile's platform.
        """
        ...
    
    def get_environment(self) -> Optional[str]:
        """
        Returns the environment.

        Returns:
            str: The environment, or None, if release is not available.
        """
        ...
    
    def get_chunk_id(self) -> str:
        """
        Returns the profile chunk ID.

        Returns:
            str: The profile chunk ID.
        """
        ...
    
    def get_organization_id(self) -> int:
        """
        Returns the organization ID.

        Returns:
            int: The organization ID to which the profile belongs.
        """
        ...
    
    def get_platform(self) -> str:
        """
        Returns the profile platform.

        Returns:
            str: The profile's platform. One of the following values:
                * android
                * cocoa
                * java
                * javascript
                * php
                * node
                * python
                * rust
                * none
        """
        ...
    
    def get_profiler_id(self) -> str:
        """
        Returns the profiler ID.

        Returns:
            str: The profile ID of the profile chunk.
        """
        ...
    
    def get_project_id(self) -> int:
        """
        Returns the project ID.

        Returns:
            int: The project ID to which the profile belongs.
        """
        ...
    
    def get_received(self) -> float:
        """
        Returns the received timestamp.

        Returns:
            float: The received timestamp.
        """
        ...
    
    def get_release(self) -> Optional[str]:
        """
        Returns the release.

        Returns:
            str: The release of the SDK used to collect this profile,
                or None, if release is not available.
        """
        ...
    
    def get_retention_days(self) -> int:
        """
        Returns the retention days.

        Returns:
            int: The retention days.
        """
        ...
    
    def duration_ms(self) -> int:
        """
        Returns the duration of the profile in ms.

        Returns:
            int: The duration of the profile in ms.
        """
        ...
    
    def start_timestamp(self) -> float:
        """
        Returns the start timestamp of the profile.

        The timestamp is a Unix timestamp in seconds
        with millisecond precision.

        Returns:
            float: The start timestamp of the profile.
        """
        ...
    
    def end_timestamp(self) -> float:
        """
        Returns the end timestamp of the profile.

        The timestamp is a Unix timestamp in seconds
        with millisecond precision.

        Returns:
            float: The end timestamp of the profile.
        """
        ...
    
    def sdk_name(self) -> Optional[str]:
        """
        Returns the SDK name.

        Returns:
            str: The name of the SDK used to collect this profile,
                or None, if version is not available.
        """
        ...
    
    def sdk_version(self) -> Optional[str]:
        """
        Returns the SDK version.

        Returns:
            str: The version of the SDK used to collect this profile,
                or None, if version is not available.
        """
        ...
    
    def storage_path(self) -> str:
        """
        Returns the storage path of the profile.

        Returns:
            str: The storage path of the profile.
        """
        ...
    
    def compress(self) -> bytes:
        """
        Compresses the profile with lz4.

        This method serializes the profile to json and then compresses it with lz4,
        returning the bytes representing the lz4 encoded profile.

        Returns:
            bytes: A bytes object representing the lz4 encoded profile.

        Raises:
            Exception: If an error occurs during the extraction process.

        Example:
            >>> compressed_profile = profile.compress()
            >>> with open("profile_compressed.lz4", "wb+") as binary_file:
            ...     binary_file.write(compressed_profile)
        """
        ...
    
    def extract_functions_metrics(self, min_depth: int, filter_system_frames: bool, max_unique_functions: Optional[int] = None) -> List["CallTreeFunction"]:
        """
        Extracts function metrics from the call tree.

        This method analyzes the call tree and extracts metrics for each function,
        returning a list of `CallTreeFunction` objects.

        Args:
            min_depth (int): The minimum depth of the node in the call tree.
                When computing slowest functions, ignore frames/node whose depth in the callTree
                is less than min_depth (i.e. if min_depth=1, we'll ignore root frames).
            filter_system_frames (bool): If `True`, system frames (e.g., standard library calls) will be filtered out.
            max_unique_functions (int, optional): An optional maximum number of unique functions to extract.
                If provided, only the top `max_unique_functions` slowest functions will be returned.
                If `None`, all functions will be returned.

        Returns:
            list[CallTreeFunction]: A list of CallTreeFunction objects, each containing metrics for a function in the call tree.

        Raises:
            Exception: If an error occurs during the extraction process.

        Example:
            >>> metrics = call_tree.extract_functions_metrics(min_depth=2, filter_system_frames=True, max_unique_functions=10)
            >>> for function_metric in metrics:
            ...     do_something(function_metric)
        """
        ...

class CallTreeFunction:
    """
    Represents function metrics from a call tree
    """
    def get_fingerprint(self) -> int:
        """
        Returns the function fingerprint.

        Returns:
            int: The function fingerprint.
        """
        ...
    
    def get_function(self) -> str:
        """
        Returns the function name.

        Returns:
            str: The function name.
        """
        ...
    
    def get_package(self) -> str:
        """
        Returns the package name.

        Returns:
            str: The package name.
        """
        ...
    
    def get_in_app(self) -> bool:
        """
        Returns whether the function is in an app or system one.

        Returns:
            bool: True if the function is an app one, False otherwise.
        """
        ...
    
    def get_self_times_ns(self) -> List[int]:
        """
        Returns the self times in nanoseconds.

        Returns:
            list[int]: The self times in nanoseconds.
        """
        ...
    
    def get_sum_self_time_ns(self) -> int:
        """
        Returns the sum of self times in nanoseconds.

        Returns:
            int: The sum of self times in nanoseconds.
        """
        ...
    
    def get_sample_count(self) -> int:
        """
        Returns the sample count.

        Returns:
            int: The sample count.
        """
        ...
    
    def get_thread_id(self) -> str:
        """
        Returns the thread ID.

        Returns:
            str: The thread ID.
        """
        ...
    
    def get_max_duration(self) -> int:
        """
        Returns the maximum duration in nanoseconds.

        Returns:
            int: The maximum duration in nanoseconds.
        """
        ...

def profile_chunk_from_json_str(profile: str, platform: Optional[str] = None) -> ProfileChunk:
    """
    Returns a `ProfileChunk` instance from a json string

    Arguments
    ---------
    profile : str
       A profile serialized as json string

    platform : Optional[str]
       An optional string representing the profile platform.
       If provided, we can directly deserialize to the right profile chunk
       more efficiently.
       If the platform is known at the time this function is invoked, it's
       recommended to always pass it.

    Returns
    -------
    ProfileChunk
      A `ProfileChunk` instance

    Raises
    ------
    Exception
        If an error occurs during the extraction process.
    """
    ...

def decompress_profile_chunk(profile: bytes) -> ProfileChunk:
    """
    Returns a `ProfileChunk` instance from a lz4 encoded profile.

    Arguments
    ---------
    profile : bytes
      A lz4 encoded profile.

    Returns
    -------
    ProfileChunk
      A `ProfileChunk` instance

    Raises
    ------
    Exception
        If an error occurs during the extraction process.

    Example
    -------
        >>> with open("profile_compressed.lz4", "rb") as binary_file:
        ...     profile = vroomrs.decompress_profile_chunk(binary_file.read())
                # do something with the profile
    """
    ...
