use std::ffi::OsStr;
use std::io::{IoSlice, IoSliceMut};
use std::os::windows::io::AsRawHandle;
use std::time::Duration;

use winapi::um::commapi;
use winapi::um::winbase;
use winapi::shared::minwindef::BOOL;

pub struct Inner {
	pub file: std::fs::File,
}

pub fn open(path: &OsStr) -> std::io::Result<Inner> {
	let file = std::fs::OpenOptions::new()
		.read(true)
		.write(true)
		.create(false)
		.open(path)?;
	Ok(from_file(file))
}

pub fn from_file(file: std::fs::File) -> Inner {
	Inner { file }
}

pub fn configure(inner: &mut Inner, settings: &crate::SerialSettings) -> std::io::Result<()> {
	unsafe {
		let mut dcb: winbase::DCB = std::mem::zeroed();
		dcb.DCBlength = std::mem::size_of::<winbase::DCB>() as u32;
		dcb.set_fBinary(1);
		dcb.BaudRate = settings.baud_rate;

		dcb::set_char_size(&mut dcb, settings.char_size);
		dcb::set_stop_bits(&mut dcb, settings.stop_bits);
		dcb::set_parity(&mut dcb, settings.parity);
		dcb::set_flow_control(&mut dcb, settings.flow_control);

		check_bool(commapi::SetCommState(inner.file.as_raw_handle(), &mut dcb))
	}
}

pub fn get_configuration(inner: &Inner) -> std::io::Result<crate::SerialSettings> {
	unsafe {
		let mut dcb: winbase::DCB = std::mem::zeroed();
		check_bool(commapi::GetCommState(inner.file.as_raw_handle(), &mut dcb))?;

		Ok(crate::SerialSettings {
			baud_rate: dcb.BaudRate,
			char_size: dcb::get_char_size(&dcb)?,
			stop_bits: dcb::get_stop_bits(&dcb)?,
			parity: dcb::get_parity(&dcb)?,
			flow_control: dcb::get_flow_control(&dcb)?,
		})
	}
}

pub fn set_read_timeout(inner: &mut Inner, timeout: Duration) -> std::io::Result<()> {
	use std::convert::TryInto;
	unsafe {
		let mut timeouts = std::mem::zeroed();
		check_bool(commapi::GetCommTimeouts(inner.file.as_raw_handle(), &mut timeouts))?;
		timeouts.ReadIntervalTimeout = 0;
		timeouts.ReadTotalTimeoutMultiplier = 0;
		timeouts.ReadTotalTimeoutConstant = timeout.as_millis().try_into().unwrap_or(u32::MAX);
		check_bool(commapi::SetCommTimeouts(inner.file.as_raw_handle(), &mut timeouts))
	}
}

pub fn get_read_timeout(inner: &Inner) -> std::io::Result<Duration> {
	unsafe {
		let mut timeouts = std::mem::zeroed();
		check_bool(commapi::GetCommTimeouts(inner.file.as_raw_handle(), &mut timeouts))?;
		Ok(Duration::from_millis(timeouts.ReadTotalTimeoutConstant.into()))
	}
}

pub fn set_write_timeout(inner: &mut Inner, timeout: Duration) -> std::io::Result<()> {
	use std::convert::TryInto;
	unsafe {
		let mut timeouts = std::mem::zeroed();
		check_bool(commapi::GetCommTimeouts(inner.file.as_raw_handle(), &mut timeouts))?;
		timeouts.WriteTotalTimeoutMultiplier = 0;
		timeouts.WriteTotalTimeoutConstant = timeout.as_millis().try_into().unwrap_or(u32::MAX);
		check_bool(commapi::SetCommTimeouts(inner.file.as_raw_handle(), &mut timeouts))
	}
}

pub fn get_write_timeout(inner: &Inner) -> std::io::Result<Duration> {
	unsafe {
		let mut timeouts = std::mem::zeroed();
		check_bool(commapi::GetCommTimeouts(inner.file.as_raw_handle(), &mut timeouts))?;
		Ok(Duration::from_millis(timeouts.WriteTotalTimeoutConstant.into()))
	}
}

pub fn read(inner: &mut Inner, buf: &mut [u8]) -> std::io::Result<usize> {
	use std::io::Read;
	inner.file.read(buf)
}

pub fn read_vectored(inner: &mut Inner, buf: &mut [IoSliceMut<'_>]) -> std::io::Result<usize> {
	// TODO: Use read timeout
	use std::io::Read;
	inner.file.read_vectored(buf)
}

pub fn write(inner: &mut Inner, buf: &[u8]) -> std::io::Result<usize> {
	use std::io::Write;
	inner.file.write(buf)
}

pub fn write_vectored(inner: &mut Inner, buf: &[IoSlice<'_>]) -> std::io::Result<usize> {
	use std::io::Write;
	inner.file.write_vectored(buf)
}

pub fn flush_output(inner: &Inner) -> std::io::Result<()> {
	unsafe {
		check_bool(winapi::um::fileapi::FlushFileBuffers(inner.file.as_raw_handle()))
	}
}

pub fn discard_buffers(inner: &mut Inner, discard_input: bool, discard_output: bool) -> std::io::Result<()> {
	unsafe {
		let mut flags = 0;
		if discard_input {
			flags |= winbase::PURGE_RXCLEAR;
		}
		if discard_output {
			flags |= winbase::PURGE_TXCLEAR;
		}
		check_bool(commapi::PurgeComm(inner.file.as_raw_handle(), flags))
	}
}

fn check_bool(ret: BOOL) -> std::io::Result<()> {
	if ret == 0 {
		Err(std::io::Error::last_os_error())
	} else {
		Ok(())
	}
}

fn other_error<E>(msg: E) -> std::io::Error
where
	E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
	std::io::Error::new(std::io::ErrorKind::Other, msg)
}

mod dcb {
	use super::*;
	use winbase::DCB;

	pub fn set_char_size(dcb: &mut DCB, char_size: crate::CharSize) {
		dcb.ByteSize = match char_size {
			crate::Bits5 => 5,
			crate::Bits6 => 6,
			crate::Bits7 => 7,
			crate::Bits8 => 8,
		};
	}

	pub fn get_char_size(dcb: &DCB) -> std::io::Result<crate::CharSize> {
		match dcb.ByteSize {
			5 => Ok(crate::Bits5),
			6 => Ok(crate::Bits6),
			7 => Ok(crate::Bits7),
			8 => Ok(crate::Bits8),
			_ => Err(other_error("unsupported char size")),
		}
	}

	pub fn set_stop_bits(dcb: &mut DCB, stop_bits: crate::StopBits) {
		dcb.StopBits = match stop_bits {
			crate::Stop1 => winbase::ONESTOPBIT,
			crate::Stop2 => winbase::TWOSTOPBITS,
		};
	}

	pub fn get_stop_bits(dcb: &DCB) -> std::io::Result<crate::StopBits> {
		match dcb.StopBits {
			winbase::ONESTOPBIT => Ok(crate::Stop1),
			winbase::TWOSTOPBITS => Ok(crate::Stop2),
			_ => Err(other_error("unsupported stop bits")),
		}
	}

	pub fn set_parity(dcb: &mut DCB, parity: crate::Parity) {
		match parity {
			crate::ParityNone => {
				dcb.set_fParity(0);
				dcb.Parity = winbase::NOPARITY;
			},
			crate::ParityOdd => {
				dcb.set_fParity(1);
				dcb.Parity = winbase::ODDPARITY;
			},
			crate::ParityEven => {
				dcb.set_fParity(1);
				dcb.Parity = winbase::EVENPARITY;
			},
		}
	}

	pub fn get_parity(dcb: &DCB) -> std::io::Result<crate::Parity> {
		let parity_enabled = dcb.fParity() != 0;
		match dcb.Parity {
			winbase::NOPARITY => Ok(crate::ParityNone),
			winbase::ODDPARITY if parity_enabled => Ok(crate::ParityOdd),
			winbase::EVENPARITY if parity_enabled => Ok(crate::ParityEven),
			_ => Err(other_error("unsupported parity configuration")),
		}
	}

	pub fn set_flow_control(dcb: &mut DCB, flow_control: crate::FlowControl) {
		match flow_control {
			crate::FlowControlNone => {
				dcb.set_fInX(0);
				dcb.set_fOutX(0);
				dcb.set_fDtrControl(winbase::DTR_CONTROL_DISABLE);
				dcb.set_fRtsControl(winbase::RTS_CONTROL_DISABLE);
				dcb.set_fOutxCtsFlow(0);
				dcb.set_fOutxDsrFlow(0);
			},
			crate::FlowControlSoftware => {
				dcb.set_fInX(1);
				dcb.set_fOutX(1);
				dcb.set_fDtrControl(winbase::DTR_CONTROL_DISABLE);
				dcb.set_fRtsControl(winbase::RTS_CONTROL_DISABLE);
				dcb.set_fOutxCtsFlow(0);
				dcb.set_fOutxDsrFlow(0);
			},
			crate::FlowControlHardware => {
				dcb.set_fInX(0);
				dcb.set_fOutX(0);
				dcb.set_fDtrControl(winbase::DTR_CONTROL_DISABLE);
				dcb.set_fRtsControl(winbase::RTS_CONTROL_TOGGLE);
				dcb.set_fOutxCtsFlow(1);
				dcb.set_fOutxDsrFlow(0);
			},
		}
	}

	pub fn get_flow_control(dcb: &DCB) -> std::io::Result<crate::FlowControl> {
		let in_x = dcb.fInX() != 0;
		let out_x = dcb.fOutX() != 0;
		let out_cts = dcb.fOutxCtsFlow() != 0;
		let out_dsr = dcb.fOutxDsrFlow() != 0;

		match (in_x, out_x, out_cts, out_dsr, dcb.fDtrControl(), dcb.fRtsControl()) {
			(false, false, false, false, winbase::DTR_CONTROL_DISABLE, winbase::RTS_CONTROL_DISABLE) => Ok(crate::FlowControlNone),
			(true, true, false, false, winbase::DTR_CONTROL_DISABLE, winbase::RTS_CONTROL_DISABLE) => Ok(crate::FlowControlSoftware),
			(false, false, true, false, winbase::DTR_CONTROL_DISABLE, winbase::RTS_CONTROL_TOGGLE) => Ok(crate::FlowControlHardware),
			_ => Err(other_error("unsupported flow control configuration")),
		}
	}
}
