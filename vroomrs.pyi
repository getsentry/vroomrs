from typing import List, Optional, Union

class Profile:
    """
    This is a Profile class
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

    def get_profile_id(self) -> str:
        """
        Returns the profile ID.

        Returns:
            str: The profile ID of the profile.
        """
        ...

    def set_profile_id(self, profile_id: str) -> None:
        """
        Sets the profile ID.

        This method updates the profile's unique identifier.

        Args:
            profile_id (str): The new profile ID to set.

        Example:
            >>> profile.set_profile_id("06ccc59502e64154a352e25cb59ccf08")
        """
        ...

    def get_retention_days(self) -> int:
        """
        Returns the retention days.

        Returns:
            int: The retention days.
        """
        ...
    
    def duration_ns(self) -> int:
        """
        Returns the duration of the profile in ns.

        Returns:
            int: The duration of the profile in ns.
        """
        ...

    def get_timestamp(self) -> float:
        """
        Returns the timestamp of the profile.

        The timestamp is a Unix timestamp in seconds
        with millisecond precision.

        Returns:
            float: The timestamp of the profile.
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
        Extracts function metrics from the profile.

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
            >>> metrics = profile.extract_functions_metrics(min_depth=2, filter_system_frames=True, max_unique_functions=10)
            >>> for function_metric in metrics:
            ...     do_something(function_metric)
        """
        ...

    def find_occurrences(self) -> "Occurrences":
        """
        Finds performance issues (occurrences) in the profile.

        This method analyzes the call tree to detect various performance issues such as:
        - Frame drops caused by main thread blocking
        - Slow operations on the main thread (e.g., I/O, compression, database operations)
        - SwiftUI performance issues (view inflation, layout, rendering)
        - Machine learning model operations
        - And other platform-specific performance patterns

        Returns:
            Occurrences: An Occurrences object, a wrapper containing a list of Occurrences, each representing a detected performance issue.

        Raises:
            Exception: If an error occurs during the detection process.
        """
        ...

    def is_sampled(self) -> bool:
        """
        Returns whether the profile is sampled.

        Returns:
            bool: True if the profile is sampled, False otherwise.
        """
        ...

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
        Extracts function metrics from the profile chunk.

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
            >>> metrics = profile_chunk.extract_functions_metrics(min_depth=2, filter_system_frames=True, max_unique_functions=10)
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

class Occurrence:
    """
    Represents a detected performance issue (occurrence) in a profile.

    An occurrence is a specific instance of a performance problem detected through
    profile analysis. It contains detailed information about the issue, including
    the problematic function, stack trace, evidence, and metadata for issue tracking.
    """
    
    def get_culprit(self) -> str:
        """
        Returns the culprit (transaction name) where the issue occurred.

        Returns:
            str: The name of the transaction or main operation where the issue occurred.
        """
        ...

    def get_detection_time(self) -> str:
        """
        Returns the detection time as an RFC 3339 formatted string.

        Returns:
            str: The detection time in RFC 3339 format.
        """
        ...

    def get_event(self) -> "Event":
        """
        Returns the event data.

        Returns:
            Event: Event data including platform, stack trace, and debug information.
        """
        ...

    def get_evidence_data(self) -> "EvidenceData":
        """
        Returns the evidence data.

        Returns:
            EvidenceData: Structured data about the performance issue.
        """
        ...

    def get_evidence_display(self) -> List["Evidence"]:
        """
        Returns the evidence display list.

        Returns:
            list[Evidence]: Human-readable evidence for displaying the issue.
        """
        ...

    def get_fingerprint(self) -> List[str]:
        """
        Returns the fingerprint list.

        Returns:
            list[str]: Unique identifiers for grouping similar issues.
        """
        ...

    def get_id(self) -> str:
        """
        Returns the occurrence ID.

        Returns:
            str: Unique identifier for this specific occurrence.
        """
        ...

    def get_issue_title(self) -> str:
        """
        Returns the issue title.

        Returns:
            str: Human-readable title describing the type of issue.
        """
        ...

    def get_level(self) -> str:
        """
        Returns the severity level.

        Returns:
            str: Severity level of the issue (e.g., "info", "warning", "error").
        """
        ...

    def get_payload_type(self) -> str:
        """
        Returns the payload type.

        Returns:
            str: Type of payload, typically "occurrence".
        """
        ...

    def get_project_id(self) -> int:
        """
        Returns the project ID.

        Returns:
            int: ID of the project where the issue was detected.
        """
        ...

    def get_resource_id(self) -> Optional[str]:
        """
        Returns the resource ID.

        Returns:
            str: Optional resource identifier, or None if not available.
        """
        ...

    def get_subtitle(self) -> str:
        """
        Returns the subtitle.

        Returns:
            str: Brief description, usually the function name where the issue occurred.
        """
        ...

    def get_type(self) -> int:
        """
        Returns the issue type.

        Returns:
            int: Numeric type identifier for the issue category.
        """
        ...

    def get_category(self) -> str:
        """
        Returns the category name.

        Returns:
            str: Category name for the performance issue.
        """
        ...

    def get_duration_ns(self) -> int:
        """
        Returns the duration in nanoseconds.

        Returns:
            int: Duration of the problematic operation in nanoseconds.
        """
        ...

    def get_sample_count(self) -> int:
        """
        Returns the sample count.

        Returns:
            int: Number of samples where this issue was detected.
        """
        ...

class Occurrences:
    """
    A wrapper class containing a list of Occurrence objects.
    
    This class wraps the results of occurrence detection, providing access to
    the detected performance issues through the occurrences attribute.
    """
    occurrences: List[Occurrence]
    
    def to_json_str(self) -> str:
        """
        Serializes the occurrences to a JSON string.

        Returns:
            str: A JSON string representation of the occurrences list.

        Raises:
            ValueError: If the serialization fails due to invalid data.

        Example:
            >>> occurrences = profile.find_occurrences()
            >>> json_str = occurrences.to_json_str()
            >>> print(json_str)
        """
        ...

    def filter_none_type_issues(self) -> None:
        """
        Filters occurrences to remove those with NONE_TYPE.

        This method removes all occurrences that have a type of NONE_TYPE,
        keeping only meaningful performance issues in the collection.

        Example:
            >>> occurrences = profile.find_occurrences()
            >>> occurrences.filter_none_type_issues()
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

def profile_from_json_str(profile: str, platform: Optional[str] = None) -> Profile:
    """
    Returns a `Profile` instance from a json string

    Arguments
    ---------
    profile : str
       A profile serialized as json string

    platform : Optional[str]
       An optional string representing the profile platform.
       If provided, we can directly deserialize to the right profile more 
       efficiently.
       If the platform is known at the time this function is invoked, it's
       recommended to always pass it.

    Returns
    -------
    Profile
      A `Profile` instance

    Raises
    ------
    Exception
        If an error occurs during the extraction process.
    """
    ...

def decompress_profile(profile: bytes) -> Profile:
    """
    Returns a `Profile` instance from a lz4 encoded profile.

    Arguments
    ---------
    profile : bytes
      A lz4 encoded profile.

    Returns
    -------
    Profile
      A `Profile` instance

    Raises
    ------
    Exception
        If an error occurs during the extraction process.

    Example
    -------
        >>> with open("profile_compressed.lz4", "rb") as binary_file:
        ...     profile = vroomrs.decompress_profile(binary_file.read())
                # do something with the profile
    """
    ...