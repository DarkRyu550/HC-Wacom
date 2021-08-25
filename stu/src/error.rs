use crate::handle::Handle;
use std::ffi::CStr;

/// An error originating from the Wacom STU API.
#[derive(Debug)]
pub struct Error {
	/// The exception that originated this error.
	exception: Exception,
	/// A handle to the string data describing this error.
	data: Handle<[std::os::raw::c_char]>,
	/// The integer code, as given by the Wacom STU API.
	stu_code: std::os::raw::c_int,
}
impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.exception)?;

		let message = unsafe { CStr::from_ptr(self.data.as_ptr() as _) }.to_string_lossy();
		write!(f, ": {}", message)?;

		Ok(())
	}
}
impl std::error::Error for Error { }

/// An exception thrown by the Wacom STU API.
///
/// An exception originates from C++, and is then translated into a Rust
/// enumeration. This means that the variants of this structure express all of
/// the error conditions given out by the API in a centralized way.
#[derive(Debug, thiserror::Error)]
pub enum Exception {
	#[error("write operations are not supported")]
	WriteNotSupported,
	#[error("the system reported an error")]
	SystemError,
	#[error("the target device is not connected")]
	NotConnected,
	#[error("the device has been removed")]
	DeviceRemoved,
	#[error("the operation has timed out")]
	TimedOut,
	#[error("an input/output error has occurred")]
	InputOutput,
	#[error("an unknown error has occurred")]
	Other,
}

/// Error type corresponding exactly to the type given to us by the C FFI.
///
/// This type must not be exposed to clients of this library, seeing as it is
/// intended for internal use. This error type encodes both errors originating
/// from the library and those originating from invalid user input. Therefore,
/// before passing an error on to the user, a function must check whether of the
/// two types of errors a given instance of this type is, and act accordingly.
///
/// # Conversion to a General Error Type
/// To assist in the process of sifting for these errors, functions may use the
/// functions [`internal_misbehavior()`], which indicates whether a particular
/// variant is generally a product of crate misbehavior, and
/// [`unwrap_to_general()`], which raises a panic in case of a crate
/// misbehavior.
///
/// Keep in mind that both of these functions are approximations. They may be
/// true for in most instances, but definitely they won't be for all. One must,
/// therefore, use them with caution, always being sure of the local
/// circumstances of every FFI call.
///
/// [`internal_misbehavior()`]: InternalError::internal_misbehavior
/// [`unwrap_to_general()`]: InternalError::unwrap_to_general
///
#[derive(Debug, thiserror::Error)]
#[error("{code}")]
pub(crate) struct InternalError {
	/// The code indicating what led to this error.
	code: InternalErrorCode,
	/// A handle to the string data describing this error.
	data: Handle<[std::os::raw::c_char]>,
	/// The integer code, as given by the Wacom STU API.
	stu_code: std::os::raw::c_int,
}
impl InternalError {
	/// Whether this error indicates a misbehavior in this crate.
	pub fn internal_misbehavior(&self) -> bool {
		match &self.code {
			InternalErrorCode::Exception(_) => false,
			_ => true
		}
	}

	/// Unwraps this error to an exception, if possible.
	pub fn unwrap_to_general(self) -> Error {
		let exception = match self.code {
			InternalErrorCode::Exception(exception) => exception,
			other => panic!(
				"Tried to unwrap non-exception to an exception: {}",
				other)
		};

		Error {
			exception,
			data: self.data,
			stu_code: self.stu_code
		}
	}

	/// Tries to create a wrapper around the error value from the Wacom STU API.
	pub fn from_wacom_stu(what: std::os::raw::c_int) -> Result<(), Self> {
		let code = match InternalErrorCode::from_wacom_stu(what) {
			Ok(_) => return Ok(()),
			Err(what) => what
		};
		let (data, stu_code) = unsafe {
			let mut stu_code = 0;
			let mut data = std::ptr::null_mut();
			let mut length = 0;

			InternalErrorCode::from_wacom_stu({
				stu_sys::WacomGSS_getException(
					&mut stu_code,
					&mut length,
					&mut data)
			}).expect("Could not get exception name data from the Wacom STU API");

			let data = Handle::wrap_slice(data, length as _);
			(data, stu_code)
		};

		Err(Self { code, data, stu_code })
	}
}

/// An enumeration of the valid types of internal errors.
#[derive(Debug, thiserror::Error)]
pub(crate) enum InternalErrorCode {
	#[error("unspecified error")]
	Unspecified,
	#[error("a handle given to the ffi function is invalid")]
	InvalidHandle,
	#[error("a parameter given to the ffi function is invalid")]
	InvalidParameter,
	#[error("a parameter had the wrong size of value")]
	InvalidSizeOf,
	#[error("the operation is unsupported")]
	Unsupported,
	#[error("{0}")]
	Exception(Exception),
}
impl InternalErrorCode {
	/// Generate our wrapper equivalent error type to the Wacom STU error.
	pub fn from_wacom_stu(what: std::os::raw::c_int) -> Result<(), Self> {
		match what {
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Success => Ok(()),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Unspecified => Err(Self::Unspecified),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_InvalidHandle => Err(Self::InvalidHandle),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_InvalidParameter => Err(Self::InvalidParameter),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_InvalidParameterNullPointer => Err(Self::InvalidParameter),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Unsupported => Err(Self::Unsupported),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Error => Err(Self::Unspecified),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_ErrorSizeof => Err(Self::InvalidSizeOf),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_Unknown => Err(Self::Exception(Exception::Other)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_std => Err(Self::Exception(Exception::Other)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_system_error => Err(Self::Exception(Exception::SystemError)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_not_connected => Err(Self::Exception(Exception::NotConnected)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_device_removed => Err(Self::Exception(Exception::DeviceRemoved)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_write_not_supported => Err(Self::Exception(Exception::WriteNotSupported)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_io => Err(Self::Exception(Exception::InputOutput)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_timeout => Err(Self::Exception(Exception::TimedOut)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_set => Err(Self::Exception(Exception::Other)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_ReportHandler => Err(Self::Exception(Exception::Other)),
			stu_sys::tagWacomGSS_Return_WacomGSS_Return_Exception_EncryptionHandler => Err(Self::Exception(Exception::Other)),
			val => panic!("Invalid return value from the Wacom STU API: {}", val)
		}
	}
}
