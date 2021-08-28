use std::rc::Rc;
use std::cell::RefCell;
use nwg::NativeUi;
use std::borrow::BorrowMut;
use crate::path::EventCanvas;
use std::sync::mpsc::SendError;

/// Initialize globals required by the windowing interface.
pub fn init() {
	nwg::init().expect("Could not initialize Win32 UI framework.");
	nwg::Font::set_global_family("Segoe UI").unwrap();

	let mut font = Default::default();
	nwg::Font::builder()
		.family("Segoe UI")
		.size(16)
		.build(&mut font)
		.unwrap();
	nwg::Font::set_global_default(Some(font)).unwrap();
}

/// Prompt the user to pick a tablet device to connect to.
pub fn pick_tablet() -> Result<stu::Information, NoTabletConnector> {
	let mut devices = stu::list_devices()
		.map(|connector| connector.info())
		.collect::<Vec<_>>();
	if devices.len() == 0 {
		return Err(NoTabletConnector::NoDevicesAvailable)
	}

	let mut channel = Rc::new(RefCell::new(None));
	let _ = {
		let selection = DeviceSelection::new(devices, channel.clone());
		let _selection = NativeUi::build_ui(selection)
			.expect("Could not build device selection window.");
		nwg::dispatch_thread_events();
	};

	let connector = channel.borrow_mut().take();
	match connector {
		Some(connector) => Ok(connector),
		None => Err(NoTabletConnector::Cancelled)
	}
}

/// Error type enumerating all of the reasons for which no tablet connector may
/// be available after a call to [`pick_tablet_connector()`].
///
/// [`pick_tablet_connector()`]: pick_tablet_connector
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, thiserror::Error)]
pub enum NoTabletConnector {
	/// This variant indicates that are no available devices.
	#[error("there are no available tablet devices")]
	NoDevicesAvailable,
	/// The user has cancelled the operation.
	#[error("the operation was cancelled")]
	Cancelled
}

/// A modal message window containing a device selection drop down menu.
#[derive(nwd::NwgUi)]
pub struct DeviceSelection {
	/// The icon we're gonna be using for the window.
	#[nwg_resource(source_system: Some(nwg::OemIcon::Information))]
	icon: nwg::Icon,

	/// The top level window this controller is contained in.
	#[nwg_control(
		title: "Tablet",
		flags: "WINDOW",
		center: true,
		icon: Some(&data.icon),
		size: (400, 100)
	)]
	#[nwg_events(
		OnInit: [Self::init],
		OnWindowClose: [Self::on_cancel]
	)]
	window: nwg::Window,

	/// The description of what should be done.
	#[nwg_control(
		text: "Select the tablet device you would like to connect to.",
		size: (380, 20),
		position: (10, 10)
	)]
	description: nwg::Label,

	/// The device connector selection box.
	#[nwg_control(
		size: (380, 40),
		position: (10, 30)
	)]
	selection: nwg::ComboBox<ConnectorDisplay>,

	/// The cancel button.
	///
	/// Having this button be clicked indicates that the user does not wish to
	/// connect to any devices and that the operation should be aborted.
	#[nwg_control(
		text: "Cancel",
		position: (290, 65)
	)]
	#[nwg_events(
		OnButtonClick: [Self::on_cancel]
	)]
	cancel: nwg::Button,

	/// The accept button.
	///
	/// Having this button be clicked indicates that the user wishes to connect
	/// to the device that is currently selected in the selection box.
	#[nwg_control(
		text: "Connect",
		position: (180, 65)
	)]
	#[nwg_events(
		OnButtonClick: [Self::on_accept]
	)]
	accept: nwg::Button,

	/// The list of table devices currently available to us.
	devices: RefCell<Vec<stu::Information>>,

	/// The channel through which we will provide our answer.
	channel: Rc<RefCell<Option<stu::Information>>>
}
impl DeviceSelection {
	/// Create a new device selection structure for the given connectors.
	fn new(
		devices: Vec<stu::Information>,
		channel: Rc<RefCell<Option<stu::Information>>>) -> Self {
		assert_ne!(
			devices.len(),
			0,
			"window::DeviceSelection controls must be initialized with device \
			lists with at least one element.");

		Self {
			icon: Default::default(),
			window: Default::default(),
			description: Default::default(),
			cancel: Default::default(),
			accept: Default::default(),
			selection: Default::default(),
			devices: RefCell::new(devices),
			channel
		}
	}

	/// Populates the data in the window controls.
	fn init(&self) {
		for device in self.devices.borrow_mut().drain(..) {
			self.selection
				.collection_mut()
				.push(ConnectorDisplay(Some(device)));
		}
		self.selection.sync();
		self.selection.set_selection(Some(0));
		self.selection.set_visible(true);

		self.window.set_visible(true);
		self.window.set_focus();
	}

	/// A source of cancellation intent has been fired.
	fn on_cancel(&self) {
		nwg::stop_thread_dispatch();
	}

	/// A source of acceptance intent has been fired.
	fn on_accept(&self) {
		let selection = self.selection.selection().unwrap();
		let selection = self.selection.collection_mut().swap_remove(selection);

		*(&(*self.channel)).borrow_mut() = Some(selection.0.unwrap());
		nwg::stop_thread_dispatch();
	}
}

/// A structure that wraps a connector and provides a display implementation.
#[derive(Default)]
struct ConnectorDisplay(Option<stu::Information>);
impl std::fmt::Display for ConnectorDisplay {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let info = self.0.as_ref().unwrap();

		write!(f, "{:04x}:{:04x}", info.vendor(), info.product())
	}
}

///
#[derive(nwd::NwgUi)]
pub struct ManagementWindow {
	/// The icon we're gonna be using for the window.
	#[nwg_resource(source_system: Some(nwg::OemIcon::Information))]
	icon: nwg::Icon,

	/// The top level window this controller is contained in.
	#[nwg_control(
		title: "Tablet",
		flags: "WINDOW",
		center: true,
		icon: Some(&data.icon),
		size: (800, 600)
	)]
	#[nwg_events(
		OnInit: [Self::init],
		OnWindowClose: [Self::on_exit]
	)]
	window: nwg::Window,

	#[nwg_control(
		background_color: Some([255, 255, 255]),
		position: (10, 30)
	)]
	/// The controller managing the display of the pen bitmap.
	display: nwg::ImageFrame,
	/// Label for the display update.
	#[nwg_control(
		text: "Screen Preview",
		position: (10, 10),
		size: (100, 20)
	)]
	display_label: nwg::Label,

	#[nwg_control()]
	#[nwg_events(
		OnNotice: [Self::on_display_update]
	)]
	/// The notice object associated with the updating of the display bitmap.
	display_update: nwg::Notice,

	/// The channel through which we pull updates to the display bitmap.
	display_blobs: std::sync::mpsc::Receiver<(u32, u32, Box<[u8]>)>,
	/// A copy of the sender side of the display bitmap channel.
	display_blobs_tx: std::sync::mpsc::Sender<(u32, u32, Box<[u8]>)>
}
impl ManagementWindow {
	/// Create a new instance of the controller structure.
	pub fn controller(&self) -> ManagementWindowController {
		ManagementWindowController {
			display_update: self.display_update.sender(),
			display_blobs: self.display_blobs_tx.clone(),
		}
	}

	/// Populates the data in the window controls.
	fn init(&self) {
		self.window.set_visible(true);
		self.window.set_focus();
	}

	/// Called when an update to the pen display preview has been requested.
	fn on_display_update(&self) {
		let (width, height, blob) = self.display_blobs
			.try_recv()
			.unwrap();
		let bitmap = nwg::Bitmap::from_bin(&blob[..]).unwrap();

		self.display.set_size(width, height);
		self.display.set_bitmap(Some(&bitmap));
	}

	/// Called when the window has been told to close.
	fn on_exit(&self) {
		nwg::stop_thread_dispatch();
	}
}
impl Default for ManagementWindow {
	fn default() -> Self {
		let (tx, rx) = std::sync::mpsc::channel();

		Self {
			icon: Default::default(),
			window: Default::default(),
			display: Default::default(),
			display_label: Default::default(),
			display_update: Default::default(),
			display_blobs: rx,
			display_blobs_tx: tx,
		}
	}
}

/// A structure for sending control and update events to the main window
#[derive(Clone)]
pub struct ManagementWindowController {
	/// The mechanism through which we notify the window of a preview update.
	display_update: nwg::NoticeSender,
	/// The mechanism through which we send bitmap blobs of the preview.
	display_blobs: std::sync::mpsc::Sender<(u32, u32, Box<[u8]>)>,
}
impl ManagementWindowController {
	/// Updates the preview image being displayed on this window so that it
	/// corresponds to the data in the given [canvas].
	///
	/// [canvas]: crate::path::EventCanvas
	pub fn update_preview(&self, source: &EventCanvas)
		-> Result<(), ManagementWindowDisconnected> {

		let bitmap = source.to_bitmap();
		self.display_blobs.send((source.width(), source.height(), bitmap))
			.map_err(|_| ManagementWindowDisconnected)?;

		self.display_update.notice();
		Ok(())
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, thiserror::Error)]
#[error("the management window has been closed")]
pub struct ManagementWindowDisconnected;
