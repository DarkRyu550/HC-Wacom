/// Handling of errors from the Wacom STU interface.
mod error;
pub use error::{Exception, Error};

/// Code dealing with the handling of reports from the device.
mod report;
pub use report::{Queue, Event, TryRecvError};

/// Handles to memory managed by the Wacom STU allocator.
mod handle;

use std::collections::HashSet;
use crate::handle::Handle;
use crate::error::{InternalError, ClientError};

use std::sync::{Arc, Mutex};

/// The interface to a Wacom STU tablet.
pub struct Tablet {
	/// The raw handle to the tablet interface.
	raw: Arc<RawTabletConnection>,
	/// The list of reports types supported by this tablet.
	supported_reports: HashSet<stu_sys::tagWacomGSS_ReportId>,
}
impl Tablet {
	/// Create a new Tablet instance from the given RawTablet interface.
	pub(crate) fn wrap(raw: RawTabletConnection) -> Result<Self, Error> {
		let supported_reports = {
			let report_list = unsafe {
				let mut list = std::ptr::null_mut();
				let mut length = 0;

				let result = raw.dispatch(|interface| {
					stu_sys::WacomGSS_Interface_getReportCountLengths(
						interface,
						&mut length,
						&mut list)
				});
				let result = InternalError::from_wacom_stu(result)
					.map_err(InternalError::unwrap_to_general);

				match result {
					Ok(_) => Some(Handle::wrap_slice(list, length as _)),
					Err(what) => {
						log::warn!(
							"tablet does not support getReportCountLengths: {}",
							what);
						None
					}
				}
			};

			let capacity = report_list.as_ref().map(|a| a.len()).unwrap_or(0);
			let mut supported = HashSet::with_capacity(capacity);
			if let Some(report_list) = report_list {
				for i in 0..report_list.len() {
					if report_list[i] != 0 {
						/* Mark this report type as being supported. */
						supported.insert(i as _);
					}
				}
			}

			supported
		};

		Ok(Self {
			raw: Arc::new(raw),
			supported_reports
		})
	}

	/// Checks whether a given Report ID is supported by this device.
	fn check_support(&self, report_id: stu_sys::tagWacomGSS_ReportId)
		-> Result<(), Error> {

		if self.supported_reports.contains(&report_id) {
			Ok(())
		} else {
			Err(Error::ClientError(ClientError::UnsupportedReportId { report_id }))
		}
	}

	/// Clear the screen of the device.
	pub fn clear(&self) -> Result<(), Error> {
		self.check_support(stu_sys::tagWacomGSS_ReportId_WacomGSS_ReportId_ClearScreen)?;

		let result = self.raw.dispatch(|interface| unsafe {
			stu_sys::WacomGSS_Protocol_setClearScreen(interface)
		});
		InternalError::from_wacom_stu(result)
			.map_err(InternalError::unwrap_to_general)
	}

	/// Changes whether inking on the display is enabled or not.
	pub fn inking(&self, enabled: bool) -> Result<(), Error> {
		self.check_support(stu_sys::tagWacomGSS_ReportId_WacomGSS_ReportId_InkingMode)?;

		let mode = if enabled {
			stu_sys::tagWacomGSS_InkingMode_WacomGSS_InkingMode_On
		} else {
			stu_sys::tagWacomGSS_InkingMode_WacomGSS_InkingMode_Off
		};
		let result = self.raw.dispatch(|interface| unsafe {
			stu_sys::WacomGSS_Protocol_setInkingMode(interface, mode as _)
		});
		InternalError::from_wacom_stu(result)
			.map_err(InternalError::unwrap_to_general)
	}

	/// Get information on the layout and the capabilities of the device.
	pub fn capability(&self) -> Result<Capability, Error> {
		self.check_support(stu_sys::tagWacomGSS_ReportId_WacomGSS_ReportId_Capability)?;
		let capability = unsafe {
			let mut capability = std::mem::zeroed();

			let result = self.raw.dispatch(|interface| {
				stu_sys::WacomGSS_Protocol_getCapability(
					interface,
					std::mem::size_of::<stu_sys::WacomGSS_Capability>() as _,
					&mut capability)
			});
			InternalError::from_wacom_stu(result)
				.map_err(InternalError::unwrap_to_general)?;

			Handle::wrap(capability)
		};

		Ok(Capability {
			display_width: u32::from(capability.screenWidth),
			display_height: u32::from(capability.screenHeight),
			input_width: u32::from(capability.tabletMaxX),
			input_height: u32::from(capability.tabletMaxY),
			input_depth: u32::from(capability.tabletMaxPressure)
		})
	}

	/// Opens a queue with which to receive events from the tablet.
	pub fn queue(&self) -> Result<Queue, Error> {
		let caps = self.capability()?;
		Queue::new(self, caps)
	}
}

/// The set of capabilities reported by the device.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct Capability {
	/// Width of the display screen, in pixels.
	display_width: u32,
	/// Height of the display screen, in pixels.
	display_height: u32,
	/// The width of the input polling grid.
	input_width: u32,
	/// The height of the input polling grid.
	input_height: u32,
	/// The depth (of pressures) of the input polling grid.
	input_depth: u32,
}
impl Capability {
	/// Width of the display screen, in pixels.
	pub fn width(&self) -> u32 {
		self.display_width
	}

	/// Height of the display screen, in pixels.
	pub fn height(&self) -> u32 {
		self.display_height
	}

	/// The width of the input grid.
	///
	/// The input grid is the grid whose cells are the smallest possible for
	/// detecting and tracking the position of the stylus. This is the maximum
	/// value the position of the pen will assume.
	pub fn input_grid_width(&self) -> u32 {
		self.input_width
	}

	/// The height of the input grid.
	///
	/// The input grid is the grid whose cells are the smallest possible for
	/// detecting and tracking the position of the stylus. This is the maximum
	/// value the position of the pen will assume.
	pub fn input_grid_height(&self) -> u32 {
		self.input_height
	}

	/// The maximum pressure value on the input grid.
	///
	/// This value can be thought of as a third dimension to the pen input grid,
	/// indicating how many cells deep the input grid is.
	pub fn input_grid_pressure(&self) -> u32 {
		self.input_depth
	}
}

/// A wrapper around a a handle to an interface.
struct RawTabletConnection {
	interface: Mutex<stu_sys::WacomGSS_Interface>,
}
impl RawTabletConnection {
	/// Dispatch the given functor with a raw handle to the interface.
	fn dispatch<F, T>(
		&self,
		fun: F) -> T
		where F: FnOnce(stu_sys::WacomGSS_Interface) -> T {

		let interface = self.interface.lock().unwrap();
		fun(*interface)
	}
}
impl Drop for RawTabletConnection {
	fn drop(&mut self) {
		self.dispatch(|interface| unsafe {
			let _ = stu_sys::WacomGSS_Interface_disconnect(interface);
			let _ = stu_sys::WacomGSS_Interface_free(interface);
		});
	}
}

/// The structure containing information about a device.
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct Information {
	id_vendor: u16,
	id_product: u16,
	bcd_device: u16,
}
impl Information {
	/// Vendor identification number of this device.
	pub fn vendor(&self) -> u16 {
		self.id_vendor
	}

	/// Product identification number of this device.
	pub fn product(&self) -> u16 {
		self.id_product
	}

	/// The device identification number.
	pub fn device(&self) -> u16 { self.bcd_device }
}

/// A connector to a tablet device.
///
/// This structure has no functionality for direct communication with a tablet,
/// and, instead, only serves to initiate a connection to a device attached to
/// the system. This structure also provides a means to identify the device
/// before a connection is established.
pub struct Connector {
	device: stu_sys::WacomGSS_UsbDevice,
}
impl Connector {
	/// Get the information about the device this connector is targeting.
	pub fn info(&self) -> Information {
		Information {
			id_vendor: self.device.usbDevice.idVendor,
			id_product: self.device.usbDevice.idProduct,
			bcd_device: self.device.usbDevice.bcdDevice
		}
	}

	/// Try to connect to the device this connector is targeting.
	pub fn connect(self) -> Result<Tablet, Error> {
		let interface = unsafe {
			let mut interface = std::mem::zeroed();
			InternalError::from_wacom_stu({
				stu_sys::WacomGSS_UsbInterface_create_1(
					std::mem::size_of::<stu_sys::WacomGSS_UsbDevice>() as _,
					&self.device,
					true as _,
					&mut interface)
			}).map_err(InternalError::unwrap_to_general)?;

			interface
		};

		Tablet::wrap(RawTabletConnection {
			interface: Mutex::new(interface)
		})
	}
}

/// An iterator over the [connectors] currently available to the application.
///
/// This structure is obtained from the [`list_devices()`] function in this
/// crate.
///
/// [connectors]: Connector
/// [`list_devices()`]: list_devices
pub struct Connectors {
	values: Handle<[stu_sys::WacomGSS_UsbDevice]>,
	index: usize,
}
impl Iterator for Connectors {
	type Item = Connector;
	fn next(&mut self) -> Option<Self::Item> {
		let val = self.values.get(self.index)
			.map(|device| Connector {
				device: *device
			});
		self.index = self.index.saturating_add(1);

		val
	}
}

/// List all of the currently available devices.
///
/// # Panic
/// This function panics if USB devices are not supported by the system.
pub fn list_devices() -> Connectors {
	let devices = unsafe {
		let mut count = 0;
		let mut devices = std::ptr::null_mut();
		InternalError::from_wacom_stu({
			stu_sys::WacomGSS_getUsbDevices(
				std::mem::size_of::<stu_sys::WacomGSS_UsbDevice>() as _,
				&mut count,
				&mut devices)
		}).unwrap();

		Handle::wrap_slice(devices, count as _)
	};

	Connectors {
		values: devices,
		index: 0
	}
}